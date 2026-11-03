import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("agent_sessions", Path(__file__).with_name("agent_sessions.py"))
sessions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sessions)
SID = "11111111-2222-3333-4444-555555555555"


class FakeHost:
    def __init__(self):
        self.calls = []
        self.state = {"workspaces": [{"workspace_id": "w7", "cwd": "/project"}], "panes": []}
        self.created = {"pane_id": "w7:p9", "workspace_id": "w7", "tab_id": "w7:t9", "terminal_id": "stable-id"}
        self.fail_submission = False

    def snapshot(self):
        self.calls.append(("snapshot",))
        return self.state

    def call(self, *args, **kwargs):
        self.calls.append(args)
        if args[:2] in (("tab", "create"), ("workspace", "create")):
            self.state["panes"].append(self.created.copy())
            return {"root_pane": self.created}
        if args[:2] == ("pane", "process-info"):
            return {"process_info": {"shell_pid": 123, "foreground_process_group_id": 123}}
        if args[:2] == ("pane", "run"):
            if self.fail_submission:
                raise TimeoutError("lost acknowledgement")
            return {}
        raise AssertionError(args)


class SessionsTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        self.path = self.home / f".codex/sessions/2026/11/02/rollout-test-{SID}.jsonl"
        self.write(self.path, [
            {"type": "session_meta", "payload": {"id": SID, "cwd": "/project", "thread_source": "user"}},
            {"type": "event_msg", "payload": {"type": "user_message", "message": "Fix navigation"}},
            {"type": "event_msg", "payload": {"type": "agent_message", "message": "Checking the route."}},
        ])
        os.utime(self.path, (100, 100))
        self.entry = sessions.catalog(self.home)[0]
        self.host = FakeHost()

    def write(self, path, rows):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(json.dumps(r) for r in rows) + "\n")

    def job(self, status="running"):
        root = self.home / "work/roboz/runtime/jarvis-jobs/task"
        root.mkdir(parents=True)
        (root / "request.json").write_text(json.dumps({"body": {"text": "Check the mobile release"}}))
        (root / "status.json").write_text(json.dumps({"status": status, "session_id": SID, "finished": 1 if status == "completed" else None}))
        return root

    def test_jarvis_job_keeps_native_identity_and_workspace(self):
        self.job()
        row = sessions.catalog(self.home)[0]
        self.assertEqual((row["session"], row["agent"], row["cwd"]), (SID, "codex", "/project"))
        self.assertEqual((row["source"], row["title"]), ("Jarvis", "Check the mobile release"))
        self.assertTrue(row["running"])

    def test_background_jobs_are_not_lost_behind_newer_saved_sessions(self):
        self.job()
        other = self.home / ".codex/sessions/2026/11/02/rollout-new-99999999-2222-3333-4444-555555555555.jsonl"
        self.write(other, [{"type": "session_meta", "payload": {"id": "99999999-2222-3333-4444-555555555555", "cwd": "/project"}}])
        with patch.object(sessions, "LIMIT", 1):
            rows = sessions.catalog(self.home)
        self.assertEqual(rows[0]["source"], "Jarvis")
        self.assertEqual(rows[0]["session"], SID)
        self.assertEqual(len(rows), 2)

    def test_latest_claude_title_is_used_after_a_long_conversation(self):
        path = self.home / f".claude/projects/project/{SID}.jsonl"
        self.write(path, [{"type": "user", "cwd": "/project", "message": {"content": "Initial prompt"}},
                          {"type": "assistant", "message": {"content": "x" * sessions.READ_LIMIT}},
                          {"type": "custom-title", "customTitle": "Useful task name"}])
        self.assertEqual(sessions.native(path, "claude")["title"], "Useful task name")

    def test_another_identified_session_does_not_block_this_one(self):
        responses = [subprocess.CompletedProcess([], 1, "", ""),
                     subprocess.CompletedProcess([], 0, "123 /usr/local/bin/codex resume another-session-id\n", "")]
        with patch.object(sessions.subprocess, "run", side_effect=responses):
            self.assertEqual(sessions.activity(self.entry), "ready")

    def test_subagents_are_excluded_and_context_is_not_task_title(self):
        rows = list(sessions.lines(self.path))
        rows[0]["payload"]["thread_source"] = "subagent"
        self.write(self.path, rows)
        self.assertEqual(sessions.catalog(self.home), [])
        rows[0]["payload"]["thread_source"] = "user"
        rows.insert(1, {"type": "event_msg", "payload": {"type": "user_message", "message": "<environment_context>injected</environment_context>"}})
        self.write(self.path, rows)
        self.assertEqual(sessions.catalog(self.home)[0]["title"], "Fix navigation")

    def test_all_four_native_harnesses_are_discovered(self):
        self.write(self.home / f".claude/projects/project/{SID}.jsonl", [
            {"type": "user", "cwd": "/project", "message": {"content": "Review the change"}}])
        kimi = self.home / f".kimi-code/sessions/wd_project/session_{SID}"
        self.write(kimi / "agents/main/wire.jsonl", [{"message": {"role": "user", "content": "Check tests"}}])
        (kimi / "state.json").write_text(json.dumps({"cwd": "/project"}))
        self.write(self.home / f".cursor/sessions/{SID}.jsonl", [
            {"event": "beforeSubmitPrompt", "cwd": "/project", "payload": {"prompt": "Fix the menu"}}])
        self.assertEqual({r["agent"] for r in sessions.catalog(self.home)}, sessions.KINDS)

    def test_watch_never_mutates_herdr_or_signals_worker(self):
        self.job()
        with patch.object(sessions.os, "kill", side_effect=AssertionError("must not signal")):
            result = sessions.handle({"operation": "watch", "key": self.entry}, self.home, self.host)
        self.assertEqual(result["state"], "watch")
        self.assertIn("Checking the route.", result["output"])
        self.assertEqual(self.host.calls, [("snapshot",)])

    def test_failed_read_does_not_report_ready(self):
        self.host.snapshot = lambda: (_ for _ in ()).throw(OSError("offline"))
        with self.assertRaises(OSError):
            sessions.handle({"operation": "watch", "key": self.entry}, self.home, self.host)

    def test_existing_tab_is_focused_without_spawning_another(self):
        self.host.state["panes"] = [{**self.host.created, "agent_status": "idle", "agent_session": {"agent": "codex", "value": SID}}]
        result = sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(result["opened"]["pane_id"], "w7:p9")
        self.assertEqual(self.host.calls, [("snapshot",)])
        watched = sessions.handle({"operation": "watch", "key": self.entry}, self.home, self.host)
        self.assertEqual(watched["state"], "attached")

    def test_an_existing_busy_herdr_tab_stays_read_only(self):
        self.host.state["panes"] = [{**self.host.created, "agent_status": "working", "agent_session": {"agent": "codex", "value": SID}}]
        self.assertEqual(sessions.handle({"operation": "watch", "key": self.entry}, self.home, self.host)["state"], "watch")
        with self.assertRaisesRegex(ValueError, "still running in Herdr"):
            sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(self.host.calls, [("snapshot",), ("snapshot",)])

    def test_running_job_cannot_be_resumed_even_if_ui_is_stale(self):
        self.job()
        entry = sessions.catalog(self.home)[0]
        with self.assertRaisesRegex(ValueError, "still running"):
            sessions.open_session(self.home, self.host, entry)
        self.assertEqual(self.host.calls, [("snapshot",)])

    def test_unknown_activity_cannot_be_resumed(self):
        with patch.object(sessions, "activity", return_value="unknown"):
            with self.assertRaisesRegex(ValueError, "cannot be verified"):
                sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(self.host.calls, [("snapshot",)])

    def test_repeated_open_does_not_create_or_submit_twice(self):
        with patch.object(sessions, "activity", return_value="ready"), patch.object(sessions, "resume_command", return_value="codex resume " + SID):
            first = sessions.open_session(self.home, self.host, self.entry)
            second = sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(first, second)
        self.assertEqual(sum(c[:2] == ("tab", "create") for c in self.host.calls), 1)
        self.assertEqual(sum(c[:2] == ("pane", "run") for c in self.host.calls), 1)

    def test_lost_submission_acknowledgement_is_not_replayed(self):
        self.host.fail_submission = True
        with patch.object(sessions, "activity", return_value="ready"), patch.object(sessions, "resume_command", return_value="codex resume " + SID):
            with self.assertRaises(TimeoutError):
                sessions.open_session(self.home, self.host, self.entry)
            result = sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(result["opened"]["pane_id"], "w7:p9")
        self.assertEqual(sum(c[:2] == ("pane", "run") for c in self.host.calls), 1)

    def test_shell_startup_timeout_reuses_the_unsent_tab(self):
        call = self.host.call
        def starting(*args, **kwargs):
            if args[:2] == ("pane", "process-info"):
                return {"process_info": {}}
            return call(*args, **kwargs)
        with patch.object(sessions, "activity", return_value="ready"), patch.object(sessions, "resume_command", return_value="codex resume " + SID):
            with patch.object(self.host, "call", side_effect=starting), patch.object(sessions.time, "sleep"):
                with self.assertRaisesRegex(ValueError, "no command was sent"):
                    sessions.open_session(self.home, self.host, self.entry)
            self.assertFalse(any(c[:2] == ("pane", "run") for c in self.host.calls))
            sessions.open_session(self.home, self.host, self.entry)
        self.assertEqual(sum(c[:2] == ("tab", "create") for c in self.host.calls), 1)
        self.assertEqual(sum(c[:2] == ("pane", "run") for c in self.host.calls), 1)

    def test_idle_cli_without_session_argument_blocks_continuation_in_its_workspace(self):
        responses = [
            subprocess.CompletedProcess([], 1, "", ""),
            subprocess.CompletedProcess([], 0, "123 /usr/local/bin/codex\n", ""),
            subprocess.CompletedProcess([], 0, "p123\nfcwd\nn/project\n", ""),
        ]
        with patch.object(sessions.subprocess, "run", side_effect=responses):
            self.assertEqual(sessions.activity(self.entry), "unknown")

    def test_workspace_uses_longest_matching_path(self):
        state = {"workspaces": [{"workspace_id": "w1", "cwd": "/work"}, {"workspace_id": "w2", "cwd": "/work/project"}]}
        self.assertEqual(sessions.workspace(state, "/work/project/ui"), "w2")
        self.assertIsNone(sessions.workspace(state, "/work-other/project"))

    def test_workspace_and_session_arguments_cannot_be_shell_code(self):
        cwd = self.home / "checkout with 'quotes' $(touch nope)"
        cwd.mkdir()
        entry = {**self.entry, "cwd": str(cwd)}
        with patch.object(sessions.shutil, "which", return_value="codex"):
            command = sessions.resume_command(entry)
        self.assertEqual(sessions.shlex.split(command), ["codex", "resume", SID])
        with self.assertRaises(ValueError):
            sessions.resolve(self.home, {"agent": "codex", "session": "x; touch nope"})

    def test_open_transcript_is_read_only_even_after_job_completed(self):
        self.job("completed")
        entry = sessions.catalog(self.home)[0]
        def run(argv, **kwargs):
            self.assertIn("lsof", argv[0])
            return subprocess.CompletedProcess(argv, 0, "123\n", "")
        with patch.object(sessions.subprocess, "run", side_effect=run):
            self.assertEqual(sessions.activity(entry), "watch")

    def test_missing_lsof_permission_does_not_enable_continuation(self):
        with patch.object(sessions.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "permission denied")):
            self.assertEqual(sessions.activity(self.entry), "unknown")

    def test_watch_output_is_bounded_and_does_not_execute_terminal_escapes(self):
        self.write(self.path, [{"type": "event_msg", "payload": {"type": "agent_message", "message": "\x1b]52;c;secret\x07" + "x" * 50000}}])
        result = sessions.output(self.entry)
        self.assertLessEqual(len(result), 24000)
        self.assertNotIn("\x1b", result)
        self.assertNotIn("\x07", result)


if __name__ == "__main__":
    unittest.main()
