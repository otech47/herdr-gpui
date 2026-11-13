"""The model and reasoning effort each agent of one Herdr daemon runs on, by pane.

The caller passes back the `known` map it was given last time, so an
unchanged transcript is only stat'ed and a growing one is read from where it ended.
"""
import glob
import json
import os
import re
import subprocess
import sys

TAIL = 256 * 1024
WIDE = 4 * 1024 * 1024
AGENT_LIMIT = 128
SESSION = re.compile(r"[A-Za-z0-9_-]{8,80}")
AGENTS = ("claude", "codex", "kimi")


def transcript(agent, session, home, env):
    """The newest transcript file of `session`, or None before the agent writes one."""
    if agent == "claude":
        base = env.get("CLAUDE_CONFIG_DIR") or os.path.join(home, ".claude")
        parts = ("projects", "*", session + ".jsonl")
    elif agent == "codex":
        base = env.get("CODEX_HOME") or os.path.join(home, ".codex")
        parts = ("sessions", "*", "*", "*", "rollout-*-" + session + ".jsonl")
    else:
        base = os.path.join(home, ".kimi-code")
        parts = ("sessions", "*", session, "agents", "main", "wire.jsonl")
    found = glob.glob(os.path.join(glob.escape(base), *parts))
    return max(found, key=os.path.getmtime) if found else None


def named(agent, row):
    """The model and effort a transcript record names, or None."""
    if agent == "claude" and row.get("type") == "assistant":
        message = row.get("message") if isinstance(row.get("message"), dict) else {}
        model = row.get("requestedModel") or message.get("model")
        effort = row.get("perTurnEffort") or row.get("effort")
    elif agent == "codex" and row.get("type") == "turn_context":
        payload = row.get("payload") if isinstance(row.get("payload"), dict) else {}
        model, effort = payload.get("model"), payload.get("effort")
    elif agent == "kimi" and row.get("type") == "llm.request":
        model = row.get("modelAlias") or row.get("model")
        effort = row.get("thinkingEffort")
    else:
        return None
    if not isinstance(model, str) or not model or model.startswith("<"):
        return None
    return model, effort if isinstance(effort, str) else ""


def newest(agent, path, start, end):
    """The newest model record in bytes `start` to `end` of `path`."""
    with open(path, "rb") as source:
        source.seek(start)
        data = source.read(end - start)
    for line in reversed(data.splitlines()):
        if b'odel"' not in line:
            continue
        try:
            row = json.loads(line)
        except ValueError:
            continue
        found = named(agent, row) if isinstance(row, dict) else None
        if found:
            return found
    return None


def read(agent, path, known):
    """`known` brought up to date with `path`: its size, its modification
    time, and the newest model and effort it records."""
    status = os.stat(path)
    size, mtime = status.st_size, status.st_mtime_ns
    if known and known.get("path") == path and known.get("size") == size and known.get("mtime") == mtime:
        return known
    grew = bool(known and known.get("path") == path and known.get("model") and known.get("size", size + 1) <= size)
    start = max(known["size"] if grew else size - TAIL, size - WIDE, 0)
    found = newest(agent, path, start, size)
    if not found and not grew and start > max(size - WIDE, 0):
        found = newest(agent, path, max(size - WIDE, 0), size)
    if not found and grew:
        found = known["model"], known.get("effort", "")
    model, effort = found or ("", "")
    return {"path": path, "size": size, "mtime": mtime, "model": model, "effort": effort}


def display(model):
    """`claude-opus-5-5` is Opus 5.5, `gpt-6-astra` GPT-6 Astra and
    `kimi-code/k3` Kimi K3. An unknown shape stays as it is."""
    model = model.rsplit("/", 1)[-1]
    match = re.fullmatch(r"claude-(opus|sonnet|haiku|fable)-(\d+)(?:-(\d{1,2}))?(?:-\d{8})?(\[1m\])?", model)
    if match:
        version = match[2] + ("." + match[3] if match[3] else "")
        return match[1].title() + " " + version + (" 1M" if match[4] else "")
    match = re.fullmatch(r"gpt-(\d+(?:\.\d+)?)(?:-(.+))?", model)
    if match:
        return "GPT-" + match[1] + (" " + match[2].replace("-", " ").title() if match[2] else "")
    match = re.fullmatch(r"k(\d+(?:\.\d+)?)(?:-(.+))?", model)
    if match:
        return "Kimi K" + match[1] + (" " + match[2].replace("-", " ").title() if match[2] else "")
    return model


def agents(cli):
    result = subprocess.run(cli + ["agent", "list"], capture_output=True, text=True, timeout=10)
    if result.returncode:
        raise RuntimeError("herdr agent list failed")
    return json.loads(result.stdout)["result"]["agents"]


def handle(request, home=None, listed=None):
    env = os.environ if home is None else {}
    home = home or os.path.expanduser("~")
    known = request.get("known") if isinstance(request.get("known"), dict) else {}
    panes, kept = {}, {}
    for agent in (listed if listed is not None else agents(request["herdr"]))[:AGENT_LIMIT]:
        session = agent.get("agent_session") or {}
        kind, value = session.get("agent") or agent.get("agent"), session.get("value") or ""
        if session.get("kind") != "id" or kind not in AGENTS or not SESSION.fullmatch(value):
            continue
        key = kind + ":" + value
        entry = known.get(key) if isinstance(known.get(key), dict) else None
        # A resumed Codex session continues in a new rollout file.
        cached = entry and kind != "codex" and os.path.isfile(entry.get("path") or "")
        path = entry["path"] if cached else transcript(kind, value, home, env)
        if not path:
            continue
        try:
            entry = read(kind, path, entry)
        except OSError:
            continue
        kept[key] = entry
        if entry["model"]:
            panes[agent["pane_id"]] = {"model": display(entry["model"]), "effort": entry["effort"]}
    return {"panes": panes, "known": kept}


if __name__ == "__main__":
    try:
        print(json.dumps(handle(json.loads(sys.argv[1]))))
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, subprocess.SubprocessError) as error:
        print(json.dumps({"error": type(error).__name__}))
        sys.exit(1)
