"""Host-side agent discovery and explicit continuation for the native picker.

The watch operation is read-only. It never writes to a worker's terminal,
signals a worker, or resumes a conversation. Only open can create a tab.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

LIMIT = 200
READ_LIMIT = 256 * 1024
KINDS = {"claude", "codex", "kimi", "cursor"}
SID = re.compile(r"[a-zA-Z0-9_-]{8,100}\Z")


def clean(value, limit=512):
    return "".join(c for c in str(value or "") if c >= " " or c == "\n")[:limit]


def document(path):
    try:
        with path.open("rb") as stream:
            return json.loads(stream.read(READ_LIMIT))
    except (OSError, ValueError):
        return {}


def lines(path, tail=False):
    try:
        with path.open("rb") as stream:
            if tail:
                stream.seek(0, 2)
                if stream.tell() > READ_LIMIT:
                    stream.seek(-READ_LIMIT, 2)
                    stream.readline()
                else:
                    stream.seek(0)
            data = stream.read(READ_LIMIT)
        for line in data.splitlines():
            try:
                row = json.loads(line)
                if isinstance(row, dict):
                    yield row
            except ValueError:
                continue
    except OSError:
        return


def text(value):
    if isinstance(value, str):
        return clean(value, 24000)
    if isinstance(value, list):
        return "\n".join(clean(part.get("text"), 24000) for part in value
                         if isinstance(part, dict) and part.get("type") in
                         ("text", "input_text", "output_text"))
    return ""


def task_title(value):
    value = text(value).strip()
    if value.startswith(("<", "# AGENTS", "AGENTS.md", "You are ", "Caveat:", "This session is being continued")):
        return ""
    return value


def native(path, agent):
    rows = list(lines(path))
    if not rows:
        return None
    sid, cwd, title = path.stem, "", ""
    if agent == "codex":
        meta = rows[0].get("payload", {})
        if rows[0].get("type") != "session_meta" or meta.get("thread_source", "user") != "user":
            return None
        if isinstance(meta.get("source"), dict):
            return None
        sid, cwd = meta.get("id", ""), meta.get("cwd", "")
    elif agent == "kimi":
        directory = path.parents[2]
        sid = directory.name.removeprefix("session_")
        state = document(directory / "state.json")
        cwd = state.get("work_dir") or state.get("cwd") or ""
        context = document(directory.parent / "metadata.json")
        cwd = cwd or context.get("work_dir", "")
    for row in rows:
        cwd = cwd or row.get("cwd", "")
        if agent == "claude":
            if row.get("isSidechain"):
                return None
            if row.get("type") == "user" and not title:
                title = task_title(row.get("message", {}).get("content"))
            if row.get("type") in ("custom-title", "summary"):
                title = row.get("customTitle") or row.get("summary") or title
        elif agent == "codex":
            payload = row.get("payload", {})
            if row.get("type") == "event_msg" and payload.get("type") == "user_message" and not title:
                title = task_title(payload.get("message"))
        elif agent == "cursor":
            payload = row.get("payload", {})
            if payload.get("parent_conversation_id") not in (None, "", sid):
                return None
            cwd = cwd or payload.get("cwd", "")
            if not title:
                title = task_title(payload.get("prompt"))
        elif agent == "kimi":
            message = row.get("message", {})
            if message.get("role") == "user" and not title:
                title = task_title(message.get("content"))
    if not SID.fullmatch(str(sid)) or not isinstance(cwd, str) or not cwd.startswith("/"):
        return None
    if agent == "claude":
        for row in lines(path, tail=True):
            if row.get("type") == "custom-title" and row.get("customTitle"):
                title = clean(row["customTitle"], 160)
    title = title.strip().splitlines()[0] if title.strip() else f"{agent.title()} session"
    # Injected context is not a useful task title.
    if title.startswith(("<", "# AGENTS", "AGENTS.md", "You are ")):
        title = f"{Path(cwd).name} session"
    return {"agent": agent, "session": sid, "cwd": cwd, "title": clean(title, 160),
            "source": "Saved session", "modified": int(path.stat().st_mtime),
            "path": str(path), "job": None}


def jobs(home):
    for source, pattern in (("Jarvis", "*/roboz/runtime/jarvis-jobs/*/request.json"),
                            ("Roboz", "*/roboz/runtime/owner-jobs/*/request.json")):
        for request in home.glob(pattern):
            job = request.parent
            state = document(job / "status.json")
            sid = state.get("session_id")
            if not sid:
                sid = next((r.get("thread_id") for r in lines(job / "events.jsonl")
                            if r.get("type") == "thread.started"), None)
            if not sid:
                continue
            data = document(request)
            title = data.get("body", {}).get("text") or data.get("command", {}).get("text")
            yield str(sid), {"source": source, "job": str(job), "title": clean(title, 160),
                             "running": state.get("status") not in ("completed", "failed") or not state.get("finished")}


def catalog(home):
    patterns = {"codex": ".codex/sessions/*/*/*/rollout-*.jsonl",
                "claude": ".claude/projects/*/*.jsonl",
                "kimi": ".kimi-code/sessions/wd_*/session_*/agents/main/wire.jsonl",
                "cursor": ".cursor/sessions/*.jsonl"}
    enrichment = dict(jobs(home))
    result = []
    for agent, pattern in patterns.items():
        paths = sorted(home.glob(pattern), key=lambda p: p.stat().st_mtime, reverse=True)[:LIMIT]
        for path in paths:
            try:
                entry = native(path, agent)
                if entry:
                    if agent == "codex" and entry["session"] in enrichment:
                        entry.update(enrichment[entry["session"]])
                    result.append(entry)
            except (OSError, TypeError, AttributeError):
                continue
    known = {entry["session"] for entry in result if entry["agent"] == "codex"}
    for sid in list(enrichment)[-100:]:
        if sid in known or not SID.fullmatch(sid):
            continue
        for path in home.glob(f".codex/sessions/*/*/*/rollout-*{sid}.jsonl"):
            try:
                entry = native(path, "codex")
                if entry:
                    entry.update(enrichment[sid])
                    result.append(entry)
                    break
            except (OSError, TypeError, AttributeError):
                continue
    names = {}
    for row in lines(home / ".codex/session_index.jsonl", tail=True):
        if row.get("id") and row.get("thread_name"):
            names[row["id"]] = clean(row["thread_name"], 160)
    for entry in result:
        if entry["agent"] == "codex" and entry["session"] in names:
            entry["title"] = names[entry["session"]]
    ordered = sorted(result, key=lambda r: r["modified"], reverse=True)
    background = [row for row in ordered if row.get("job")][:100]
    recent = [row for row in ordered if not row.get("job")][:LIMIT]
    return background + recent


class Rejected(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


class SessionMissing(ValueError):
    pass


class Host:
    def __init__(self, args):
        self.args = args

    def call(self, *args, timeout=15):
        result = subprocess.run(self.args + list(args), capture_output=True, text=True, timeout=timeout)
        if result.returncode == 0 and not result.stdout.strip():
            return {}
        reply = json.loads(result.stdout or result.stderr)
        if result.returncode or "error" in reply:
            error = reply.get("error", {})
            raise Rejected(error.get("code", "herdr_error"), error.get("message", "Herdr refused the request."))
        return reply["result"]

    def snapshot(self):
        return self.call("api", "snapshot")["snapshot"]


def attached(snapshot, entry):
    for pane in snapshot.get("panes", []):
        native_session = pane.get("agent_session") or {}
        if native_session.get("value") == entry["session"] and native_session.get("agent") == entry["agent"]:
            return {**{key: pane[key] for key in ("pane_id", "tab_id", "workspace_id")},
                    "agent_status": pane.get("agent_status", "unknown")}
    return None


def attached_activity(pane, entry):
    if entry.get("job"):
        state = document(Path(entry["job"]) / "status.json")
        if state.get("status") not in ("completed", "failed") or not state.get("finished"):
            return "watch"
    if pane["agent_status"] in ("idle", "done"):
        return "attached"
    return "watch" if pane["agent_status"] in ("working", "blocked") else "unknown"


def activity(entry):
    job = entry.get("job")
    if job:
        state = document(Path(job) / "status.json")
        if state.get("status") not in ("completed", "failed") or not state.get("finished"):
            # A missing process does not make an unfinished job safe to resume.
            return "watch"
    path = entry["path"]
    executable = shutil.which("lsof") or "/usr/sbin/lsof"
    result = subprocess.run([executable, "-t", "--", path], capture_output=True, text=True, timeout=5)
    if result.stdout.strip():
        return "watch"
    if result.returncode not in (0, 1) or result.stderr.strip():
        return "unknown"
    # Some CLIs close their transcript between writes. Match their explicit
    # resume arguments too, and never call a file being updated ready.
    processes = subprocess.run(["/bin/ps", "-axo", "pid=,args="], capture_output=True, text=True, timeout=5)
    if processes.returncode:
        return "unknown"
    pids = set()
    ambiguous = []
    for line in processes.stdout.splitlines():
        pid, _, command = line.strip().partition(" ")
        if pid.isdigit():
            pids.add(int(pid))
        if not pid.isdigit() or int(pid) == os.getpid():
            continue
        try:
            argv = shlex.split(command)
        except ValueError:
            continue
        names = {Path(arg).name for arg in argv[:2]}
        binaries = {"cursor": {"agent", "cursor-agent", "cursor"},
                    "codex": {"codex", "codex.js"}, "claude": {"claude", "claude.js"},
                    "kimi": {"kimi"}}[entry["agent"]]
        if names & binaries:
            if entry["session"] in argv:
                return "watch"
            if any(flag in argv and argv.index(flag) + 1 < len(argv)
                   and SID.fullmatch(argv[argv.index(flag) + 1])
                   for flag in ("resume", "--resume", "--session")):
                continue
            ambiguous.append(pid)
    if ambiguous:
        owners = subprocess.run([executable, "-a", "-p", ",".join(ambiguous), "-d", "cwd", "-Fn"],
                                capture_output=True, text=True, timeout=5)
        if owners.returncode not in (0, 1) or owners.stderr.strip() or not owners.stdout.strip():
            return "unknown"
        if "n" + entry["cwd"] in owners.stdout.splitlines():
            # A fresh CLI may omit its session ID and close the transcript
            # between turns. Its presence in this workspace prevents a handoff.
            return "unknown"
    if entry["agent"] == "claude":
        for registry in (Path.home() / ".claude/sessions").glob("*.json"):
            registered = document(registry)
            if registered.get("sessionId") == entry["session"] and registered.get("pid") in pids:
                return "watch"
    if entry["agent"] == "codex" and not job:
        turn_running = False
        for row in lines(Path(path), tail=True):
            event = row.get("payload", {}).get("type") if row.get("type") == "event_msg" else None
            if event == "task_started":
                turn_running = True
            elif event in ("task_complete", "task_aborted", "turn_aborted"):
                turn_running = False
        if turn_running:
            return "watch"
    if time.time() - Path(path).stat().st_mtime < 5:
        return "watch"
    return "ready"


def output(entry):
    messages = []
    for row in lines(Path(entry["path"]), tail=True):
        kind = row.get("type")
        payload = row.get("payload", {})
        if entry["agent"] == "codex":
            if kind == "event_msg" and payload.get("type") == "agent_message":
                messages.append(text(payload.get("message")))
            elif kind == "response_item" and payload.get("type") == "message" and payload.get("role") == "assistant":
                value = text(payload.get("content"))
                if not messages or messages[-1] != value:
                    messages.append(value)
            elif kind == "response_item" and payload.get("type") == "function_call":
                messages.append("Working: " + clean(payload.get("name")))
        elif entry["agent"] == "claude" and kind == "assistant":
            messages.append(text(row.get("message", {}).get("content")))
        elif entry["agent"] == "cursor":
            if row.get("event") == "afterAgentResponse":
                messages.append(text(payload.get("text")))
        elif entry["agent"] == "kimi":
            message = row.get("message", {})
            if message.get("role") == "assistant":
                messages.append(text(message.get("content")))
    if entry.get("job") and not any(messages):
        result = Path(entry["job"]) / "result.md"
        if result.is_file():
            with result.open("rb") as stream:
                messages.append(clean(stream.read(READ_LIMIT).decode("utf-8", "replace"), 24000))
    return "\n\n".join(m for m in messages if m)[-24000:]


def resolve(home, key):
    agent, sid = key.get("agent"), key.get("session")
    if agent not in KINDS or not isinstance(sid, str) or not SID.fullmatch(sid):
        raise ValueError("That session identity is invalid.")
    pattern = {"codex": f".codex/sessions/*/*/*/rollout-*{sid}.jsonl",
               "claude": f".claude/projects/*/{sid}.jsonl",
               "kimi": f".kimi-code/sessions/wd_*/session_{sid}/agents/main/wire.jsonl",
               "cursor": f".cursor/sessions/{sid}.jsonl"}[agent]
    entry = next((value for path in home.glob(pattern)
                  if (value := native(path, agent)) and value["session"] == sid), None)
    if not entry:
        raise SessionMissing("That saved session is no longer available. Refresh the list.")
    if agent == "codex":
        entry.update(dict(jobs(home)).get(sid, {}))
    return entry


def workspace(snapshot, cwd):
    candidates = []
    for space in snapshot.get("workspaces", []):
        paths = [space.get("cwd"), (space.get("worktree") or {}).get("checkout_path")]
        paths.extend(p.get("cwd") for p in snapshot.get("panes", [])
                     if p.get("workspace_id") == space["workspace_id"])
        for value in paths:
            if value and (cwd == value or cwd.startswith(value.rstrip("/") + "/")):
                candidates.append((len(value), space["workspace_id"]))
    return max(candidates)[1] if candidates else None


def resume_command(entry):
    binary = {"cursor": "agent"}.get(entry["agent"], entry["agent"])
    flag = {"codex": "resume", "kimi": "--session"}.get(entry["agent"], "--resume")
    argv = [binary, flag, entry["session"]]
    cwd = Path(entry["cwd"])
    if not cwd.is_dir():
        raise ValueError("The original workspace is missing on this device.")
    if not shutil.which(binary):
        raise ValueError(f"{binary} is not installed on this device.")
    for directory in (cwd, *cwd.parents):
        if (directory / "flake.nix").is_file():
            flake = str(directory)
            if (directory / ".git").is_file() and (directory / "dev/worktree-cache.py").is_file():
                flake += "#worktree"
            argv = ["env", "-u", "CARGO_BUILD_TARGET_DIR", "-u", "CARGO_TARGET_DIR",
                    "-u", "TS_RS_EXPORT_DIR", "-u", "REPO_ROOT", "nix", "develop", flake, "-c", *argv]
            break
    return shlex.join(argv)


def open_session(home, host, entry):
    key = hashlib.sha256((entry["agent"] + ":" + entry["session"]).encode()).hexdigest()
    directory = home / ".local/state/herdr/agent-opens"
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (directory / (key + ".lock")).open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError("This session is already being opened. Try again in a moment.") from None
        snapshot = host.snapshot()
        existing = attached(snapshot, entry)
        if existing:
            if attached_activity(existing, entry) != "attached":
                raise ValueError("This agent is still running in Herdr. Keep watching until it is ready for input.")
            return {"opened": existing}
        receipt = directory / (key + ".json")
        previous = document(receipt)
        pane = None
        if previous.get("opened"):
            old = previous["opened"]
            pane = next((p for p in snapshot.get("panes", []) if p.get("pane_id") == old.get("pane_id")
                         and p.get("terminal_id") == old.get("terminal_id")), None)
            if pane and (pane.get("agent_session") or {}).get("value") not in (None, entry["session"]):
                pane = None
            if pane and previous.get("phase") != "created":
                return {"opened": old}
        if activity(entry) != "ready":
            raise ValueError("This session is still running or its activity cannot be verified. Watch it until it finishes.")
        command = resume_command(entry)
        if pane is None:
            target = workspace(snapshot, entry["cwd"])
            if target:
                created = host.call("tab", "create", "--workspace", target, "--cwd", entry["cwd"],
                                    "--label", entry["title"], "--no-focus")
            else:
                created = host.call("workspace", "create", "--cwd", entry["cwd"],
                                    "--label", Path(entry["cwd"]).name, "--no-focus")
            pane = created["root_pane"]
        opened = {k: pane[k] for k in ("workspace_id", "tab_id", "pane_id", "terminal_id")}
        save_receipt(receipt, opened, "created")
        # Never retry a terminal submission whose outcome is uncertain. The
        # saved receipt takes the next click to the exact tab already created.
        for attempt in range(40):
            info = host.call("pane", "process-info", "--pane", pane["pane_id"])
            info = info.get("process_info", info)
            if info.get("shell_pid") and info.get("foreground_process_group_id") == info.get("shell_pid"):
                break
            if attempt == 39:
                raise ValueError("The new tab is still starting. Try Continue again; no command was sent.")
            time.sleep(0.25)
        current = host.snapshot()
        if not any(p.get("pane_id") == opened["pane_id"] and p.get("terminal_id") == opened["terminal_id"]
                   and not p.get("agent") for p in current.get("panes", [])):
            raise ValueError("The new tab changed before continuation. Refresh the list.")
        if activity(entry) != "ready":
            raise ValueError("The session started running again. Keep watching until it finishes.")
        save_receipt(receipt, opened, "submitting")
        host.call("pane", "run", pane["pane_id"], command)
        save_receipt(receipt, opened, "sent")
        return {"opened": opened}


def save_receipt(path, opened, phase):
    temporary = path.with_suffix(".pending")
    with temporary.open("w") as stream:
        json.dump({"opened": opened, "phase": phase}, stream)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def handle(request, home=None, host=None):
    home = home or Path.home()
    host = host or Host(request.get("herdr", ["herdr"]))
    operation = request["operation"]
    if operation == "list":
        return {"entries": [{k: v for k, v in e.items() if k not in ("path", "job")}
                            for e in catalog(home)]}
    try:
        entry = resolve(home, request["key"])
    except SessionMissing:
        if operation == "watch":
            return {"state": "missing", "output": ""}
        raise
    if operation == "watch":
        existing = attached(host.snapshot(), entry)
        return {"state": attached_activity(existing, entry) if existing else activity(entry), "output": output(entry)}
    if operation == "open":
        return open_session(home, host, entry)
    raise ValueError("Unknown session operation.")


if __name__ == "__main__":
    try:
        print(json.dumps(handle(json.loads(sys.argv[1]))))
    except (OSError, ValueError, KeyError, Rejected, subprocess.SubprocessError) as error:
        print(json.dumps({"error": clean(str(error), 1000)}))
