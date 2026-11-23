import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("agent_models", Path(__file__).with_name("agent_models.py"))
models = importlib.util.module_from_spec(spec)
spec.loader.exec_module(models)
SID = "11111111-2222-3333-4444-555555555555"


def claude(model, effort, per_turn=None):
    return {"type": "assistant", "requestedModel": model, "effort": effort, "perTurnEffort": per_turn,
            "message": {"model": model, "content": [{"type": "text", "text": "x"}]}}


def write(path, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a") as file:
        for row in rows:
            file.write((row if isinstance(row, str) else json.dumps(row)) + "\n")


def listed(agent, session, pane="w1:p1"):
    return [{"pane_id": pane, "agent": agent,
             "agent_session": {"agent": agent, "kind": "id", "source": "herdr:" + agent, "value": session}}]


class Names(unittest.TestCase):
    def test_model_ids_read_as_people_say_them(self):
        cases = {
            "claude-opus-5-5": "Opus 5.5",
            "claude-fable-5-2": "Fable 5.2",
            "claude-sonnet-5": "Sonnet 5",
            "claude-opus-4-5-20251101": "Opus 4.5",
            "claude-opus-5-5[1m]": "Opus 5.5 1M",
            "gpt-6-astra": "GPT-6 Astra",
            "gpt-6.1-sol": "GPT-6.1 Sol",
            "gpt-5.1-codex-max": "GPT-5.1 Codex Max",
            "kimi-code/k3": "Kimi K3",
            "kimi-code/k3.1-pro": "Kimi K3.1 Pro",
            "codex-auto-review": "codex-auto-review",
        }
        for model, shown in cases.items():
            self.assertEqual(models.display(model), shown, model)


class Transcripts(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.TemporaryDirectory()
        self.addCleanup(self.home.cleanup)
        self.root = Path(self.home.name)

    def test_the_newest_claude_turn_wins_and_a_per_turn_effort_overrides(self):
        path = self.root / ".claude/projects/-tmp/" / (SID + ".jsonl")
        write(path, [claude("claude-opus-5-5", "high"), {"type": "user", "message": {"content": "hi"}},
                     claude("claude-fable-5-2", "high", per_turn="max"), '{"type": "assistant", "model": broken'])
        answer = models.handle({}, home=self.home.name, listed=listed("claude", SID))
        self.assertEqual(answer["panes"], {"w1:p1": {"model": "Fable 5.2", "effort": "max"}})

    def test_codex_and_kimi_records(self):
        codex = self.root / ".codex/sessions/2026/11/13" / ("rollout-2026-11-13T09-00-00-" + SID + ".jsonl")
        write(codex, [{"type": "turn_context", "payload": {"model": "gpt-6-astra", "effort": "xhigh"}}])
        kimi_id = "session_" + SID
        kimi = self.root / ".kimi-code/sessions/wd_tools_abc" / kimi_id / "agents/main/wire.jsonl"
        write(kimi, [{"type": "llm.request", "model": "k3", "modelAlias": "kimi-code/k3", "thinkingEffort": "high"},
                     {"type": "usage.record", "model": "kimi-code/k3"}])
        agents = listed("codex", SID, "w1:p1") + listed("kimi", kimi_id, "w1:p2")
        answer = models.handle({}, home=self.home.name, listed=agents)
        self.assertEqual(answer["panes"], {"w1:p1": {"model": "GPT-6 Astra", "effort": "xhigh"},
                                           "w1:p2": {"model": "Kimi K3", "effort": "high"}})

    def test_a_grown_transcript_is_read_from_where_it_ended(self):
        path = self.root / ".claude/projects/-tmp/" / (SID + ".jsonl")
        write(path, [claude("claude-opus-5-5", "high")])
        first = models.handle({}, home=self.home.name, listed=listed("claude", SID))
        write(path, [{"type": "user", "message": {"content": "x" * 1000}}])
        second = models.handle({"known": first["known"]}, home=self.home.name, listed=listed("claude", SID))
        self.assertEqual(second["panes"]["w1:p1"], {"model": "Opus 5.5", "effort": "high"})
        self.assertGreater(second["known"]["claude:" + SID]["size"], first["known"]["claude:" + SID]["size"])
        write(path, [claude("claude-opus-5-5", "xhigh")])
        third = models.handle({"known": second["known"]}, home=self.home.name, listed=listed("claude", SID))
        self.assertEqual(third["panes"]["w1:p1"]["effort"], "xhigh")

    def test_an_unchanged_transcript_is_not_read_again(self):
        path = self.root / ".claude/projects/-tmp/" / (SID + ".jsonl")
        write(path, [claude("claude-opus-5-5", "high")])
        first = models.handle({}, home=self.home.name, listed=listed("claude", SID))
        entry = first["known"]["claude:" + SID]
        entry["model"] = "claude-sonnet-5-5"
        again = models.handle({"known": first["known"]}, home=self.home.name, listed=listed("claude", SID))
        self.assertEqual(again["panes"]["w1:p1"]["model"], "Sonnet 5.5")

    def test_a_record_past_the_first_tail_is_found_on_the_first_look(self):
        path = self.root / ".claude/projects/-tmp/" / (SID + ".jsonl")
        filler = {"type": "user", "message": {"content": "x" * (models.TAIL + 10)}}
        write(path, [claude("claude-opus-5-5", "low"), filler])
        answer = models.handle({}, home=self.home.name, listed=listed("claude", SID))
        self.assertEqual(answer["panes"]["w1:p1"], {"model": "Opus 5.5", "effort": "low"})

    def test_sessions_without_a_transcript_or_a_usable_id_are_left_out(self):
        agents = listed("claude", SID) + listed("claude", "../../etc", "w1:p2") + listed("cursor", SID, "w1:p3")
        agents.append({"pane_id": "w1:p4", "agent": "claude", "agent_session": None})
        self.assertEqual(models.handle({}, home=self.home.name, listed=agents),
                         {"panes": {}, "reports": {}, "known": {}})


BASE = "http://oz-ms.tail5b90f8.ts.net:8788/.agent-scratch/tools/run/reports/"


def wrote(text, stamp="2026-11-23T10:00:00.000Z"):
    return {"type": "assistant", "timestamp": stamp, "message": {"content": [{"type": "text", "text": text}]}}


class Reports(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.TemporaryDirectory()
        self.addCleanup(self.home.cleanup)
        self.root = Path(self.home.name)
        self.path = self.root / ".claude/projects/-tmp/" / (SID + ".jsonl")

    def ask(self, known=None, panes=("w1:p1",), agents=None):
        request = {"known": known, "reports": list(panes)}
        return models.handle(request, home=self.home.name, listed=agents or listed("claude", SID))

    def test_only_pages_the_agent_wrote_to_its_user_are_listed_newest_first(self):
        write(self.path, [
            wrote("Report: " + BASE + "audit.html."),
            {"type": "user", "timestamp": "2026-11-23T10:01:00.000Z",
             "message": {"content": [{"type": "text", "text": BASE + "pasted.html"}]}},
            {"type": "assistant", "isSidechain": True, "message": {"content": [{"type": "text", "text": BASE + "side.html"}]}},
            wrote("See (" + BASE + "plan.md) and https://example.com:8788/x.html and " + BASE + "data.json",
                  "2026-11-23T11:00:00.000Z"),
        ])
        write(self.path, [{"type": "assistant", "timestamp": "2026-11-23T11:00:00.000Z", "message": {"content": [
            {"type": "tool_use", "name": "AskUserQuestion", "input": {"questions": [{"question": "Pick at " + BASE + "ask.html"}]}},
            {"type": "tool_use", "name": "Bash", "input": {"command": "curl " + BASE + "fetched.html"}}]}}])
        reports = self.ask()["reports"]["w1:p1"]
        self.assertEqual([report["url"] for report in reports],
                         [BASE + "ask.html", BASE + "plan.md", BASE + "audit.html"])
        reports = reports[1:]
        self.assertEqual(reports[1]["at"], 1795428000)
        self.assertEqual(reports[0]["at"] - reports[1]["at"], 3600)

    def test_transcripts_are_read_for_reports_only_in_the_panes_asked_for(self):
        write(self.path, [claude("claude-opus-5-5", "high"), wrote(BASE + "audit.html")])
        answer = self.ask(panes=())
        self.assertEqual(answer["reports"], {})
        self.assertNotIn("reports", answer["known"]["claude:" + SID])
        self.assertEqual(answer["panes"]["w1:p1"]["model"], "Opus 5.5")
        self.assertEqual(self.ask(panes=("w1:p1",))["reports"], {"w1:p1": [
            {"url": BASE + "audit.html", "title": "", "at": 1795428000}]})

    def test_a_grown_transcript_is_read_for_reports_from_where_it_ended(self):
        write(self.path, [wrote(BASE + "audit.html"), wrote(BASE + "plan.html")])
        first = self.ask()
        entry = first["known"]["claude:" + SID]
        self.assertEqual(entry["scanned"], self.path.stat().st_size)
        entry["reports"][0]["title"] = "kept from the last pass"
        with open(self.path, "a") as file:
            file.write(json.dumps(wrote(BASE + "plan.html", "2026-11-23T12:00:00.000Z")) + "\n")
            file.write(json.dumps(wrote(BASE + "late.html"))[:-5])
        complete = self.path.stat().st_size - len(json.dumps(wrote(BASE + "late.html"))[:-5])
        second = self.ask(known=first["known"])
        self.assertEqual(second["known"]["claude:" + SID]["scanned"], complete)
        self.assertEqual([(report["url"], report["title"]) for report in second["reports"]["w1:p1"]],
                         [(BASE + "plan.html", ""), (BASE + "audit.html", "kept from the last pass")])
        self.assertEqual(second["reports"]["w1:p1"][0]["at"], 1795435200)

    def test_a_page_this_host_holds_is_listed_by_its_own_title(self):
        page = self.root / ".agent-scratch/tools/run/reports/audit.html"
        page.parent.mkdir(parents=True)
        page.write_text("<html><head><title>\n  Disk audit &amp; cleanup\n</title></head></html>")
        write(self.path, [wrote(BASE + "audit.html and " + BASE + "../../../../../etc/passwd.html")])
        reports = self.ask()["reports"]["w1:p1"]
        self.assertEqual([report["title"] for report in reports], ["", "Disk audit & cleanup"])

    def test_codex_and_kimi_records(self):
        codex = self.root / ".codex/sessions/2026/11/13" / ("rollout-2026-11-13T09-00-00-" + SID + ".jsonl")
        write(codex, [{"type": "response_item", "timestamp": "2026-11-23T10:00:00.000Z",
                       "payload": {"type": "message", "role": "assistant",
                                   "content": [{"type": "output_text", "text": BASE + "codex.html"}]}},
                      {"type": "response_item", "payload": {"type": "message", "role": "user",
                                                            "content": [{"type": "input_text", "text": BASE + "no.html"}]}}])
        kimi_id = "session_" + SID
        kimi = self.root / ".kimi-code/sessions/wd_tools_abc" / kimi_id / "agents/main/wire.jsonl"
        write(kimi, [{"type": "context.append_loop_event", "time": 1795428000123,
                      "event": {"type": "content.part", "part": {"type": "text", "text": BASE + "kimi.html"}}},
                     {"type": "context.append_loop_event", "time": 1795428000123,
                      "event": {"type": "content.part", "part": {"type": "think", "think": BASE + "no.html"}}}])
        agents = listed("codex", SID, "w1:p1") + listed("kimi", kimi_id, "w1:p2")
        reports = self.ask(panes=("w1:p1", "w1:p2"), agents=agents)["reports"]
        self.assertEqual(reports["w1:p1"], [{"url": BASE + "codex.html", "title": "", "at": 1795428000}])
        self.assertEqual(reports["w1:p2"], [{"url": BASE + "kimi.html", "title": "", "at": 1795428000}])


if __name__ == "__main__":
    unittest.main()
