#!/usr/bin/env python3
"""Quit and reopen the installed Herdr.app, proving the daemons and every agent process survive it.

usage: python3 .agents/relaunch.py [scratch run dir]

With a scratch run dir, each reopened window is screenshotted into its screenshots/ folder.
Exit 0 when every check passed, 1 when a check before the quit stopped the relaunch, 2 when a check after it failed.
"""
import hashlib
import json
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LISTER_SOURCE = REPO / ".agents/herdr-windows.swift"
LISTER = REPO / "target/herdr-windows"
STATE = Path.home() / ".local/state/herdr/gpui/window-state.json"
APP_LOG = Path.home() / ".local/state/herdr/gpui/logs/herdr-gpui.jsonl"
APP_BINARY = Path("/Applications/Herdr.app/Contents/MacOS/Herdr")
HERDR = Path.home() / ".local/bin/herdr"
MINI = "oz-mini"
AGENT = re.compile(r"(^|/)(claude|codex|kimi-code|kimi|agent)$")
HELPERS = ("ssh", "sh")


def run(*args, timeout=30):
    return subprocess.run(args, capture_output=True, text=True, check=False, timeout=timeout)


def app_pids():
    return [int(pid) for pid in run("pgrep", "-x", "Herdr").stdout.split()]


def local_server_pid():
    out = run("pgrep", "-f", f"^{HERDR} server$").stdout.split()
    return int(out[0]) if out else None


def mini_server_pid():
    out = run("ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", MINI, "pgrep -f 'herdr server$'").stdout.split()
    return int(out[0]) if out else None


def process_table():
    table = {}
    for row in run("ps", "-axo", "pid=,ppid=,comm=").stdout.splitlines():
        parts = row.split(None, 2)
        if len(parts) == 3:
            table[int(parts[0])] = (int(parts[1]), parts[2])
    return table


def descendants(root, table):
    found = set()
    for pid in table:
        chain = pid
        while chain in table and chain not in (root, 1):
            chain = table[chain][0]
        if chain == root:
            found.add(pid)
    return found


def agent_pids(root, table):
    return {pid for pid in descendants(root, table) if AGENT.search(table[pid][1])}


def app_children(app, table):
    return {pid: comm for pid, (ppid, comm) in table.items() if ppid == app}


def lister():
    if not LISTER.exists() or LISTER.stat().st_mtime < LISTER_SOURCE.stat().st_mtime:
        LISTER.parent.mkdir(parents=True, exist_ok=True)
        built = run("xcrun", "swiftc", "-O", str(LISTER_SOURCE), "-o", str(LISTER), timeout=300)
        if built.returncode != 0:
            raise SystemExit(f"could not build the window lister: {built.stderr.strip()}")
    return LISTER


def windows():
    rows = []
    for line in run(str(lister())).stdout.splitlines():
        parts = line.split("\t")
        if len(parts) == 4 and parts[3]:
            rows.append(parts)
    return rows


def wait(predicate, seconds, step=0.25):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(step)
    return predicate()


def server_status():
    return run(str(HERDR), "status", "server").stdout.strip()


def app_log_problems(since):
    problems = []
    if not APP_LOG.exists():
        return problems
    for line in APP_LOG.read_text(errors="replace").splitlines()[-4000:]:
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        stamp = record.get("timestamp", "").strip("[]")
        if record.get("level") in ("ERROR", "WARN") and stamp >= since:
            problems.append(f"{stamp} {record['level']} {record.get('message')} {record.get('fields', '')}"[:200])
    return problems


def screenshot(rows, scratch):
    folder = scratch / "screenshots"
    folder.mkdir(parents=True, exist_ok=True)
    for row in rows:
        run("screencapture", "-x", "-l", row[0], str(folder / f"window-{row[0]}.png"))
    print(f"screenshots: {folder}")


def main():
    scratch = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else None
    failures = []
    app = app_pids()
    if len(app) != 1:
        print(f"expected one Herdr process, found {app}", file=sys.stderr)
        return 1
    app = app[0]
    table = process_table()
    children = app_children(app, table)
    foreign = {pid: comm for pid, comm in children.items() if comm.rsplit("/", 1)[-1] not in HELPERS}
    print(f"app pid before: {app}, children: {len(children)}, ssh and sh helpers only: {not foreign}")
    if foreign:
        print(f"the app owns processes other than its ssh and sh helpers, stopping: {foreign}", file=sys.stderr)
        return 1
    local_server = local_server_pid()
    mini_server = mini_server_pid()
    agents_before = agent_pids(local_server, table) if local_server else set()
    status_before = server_status()
    saved = len(json.loads(STATE.read_text()))
    print(f"local server pid: {local_server}, mini server pid: {mini_server or 'unreachable'}")
    print(f"agent processes under the local server: {len(agents_before)}")
    print(f"saved windows: {saved}, titled windows now: {len(windows())}")
    if local_server is None or not agents_before:
        print("could not read the local daemon or its agents, stopping", file=sys.stderr)
        return 1

    since = time.strftime("%Y-%m-%d %H:%M:%S")
    run("osascript", "-e", 'tell application "Herdr" to quit')
    if not wait(lambda: not app_pids(), 30):
        print("Herdr did not quit within 30 s", file=sys.stderr)
        return 1
    print("app quit")
    run("open", "-a", "Herdr")
    if not wait(lambda: len(app_pids()) == 1, 30):
        print("Herdr did not start within 30 s", file=sys.stderr)
        return 1
    new_app = app_pids()[0]
    print(f"app pid after: {new_app}")

    if not wait(lambda: len(windows()) >= saved, 60, step=0.5):
        failures.append(f"windows did not come back: {len(windows())} titled of {saved} saved")
    # A title lags behind the space its window shows, so the screenshots prove the spaces.
    rows = windows()
    for row in rows:
        print("  ", "\t".join(row))
    if scratch:
        screenshot(rows, scratch)

    open_files = run("lsof", "-p", str(new_app), "-a", "-d", "txt").stdout
    digest = hashlib.sha256(APP_BINARY.read_bytes()).hexdigest()[:12]
    print(f"running binary: {APP_BINARY if str(APP_BINARY) in open_files else 'not the installed app'}, installed hash: {digest}")
    if str(APP_BINARY) not in open_files:
        failures.append("the new process does not run /Applications/Herdr.app")

    table = process_table()
    lost = agents_before - set(table)
    local_after = local_server_pid()
    mini_after = mini_server_pid()
    print(f"local server pid after: {local_after}, mini server pid after: {mini_after or 'unreachable'}")
    print(f"agent processes still alive: {len(agents_before - lost)} of {len(agents_before)}")
    if local_after != local_server:
        failures.append("the local server pid changed")
    if mini_server and mini_after and mini_after != mini_server:
        failures.append("the mini server pid changed")
    if lost:
        failures.append(f"agent processes gone: {sorted(lost)}")
    if server_status() != status_before:
        failures.append("herdr status server changed")
    wait(lambda: len(app_children(new_app, process_table())) >= 4, 30, step=0.5)
    print(f"app children after: {len(app_children(new_app, process_table()))}")
    problems = app_log_problems(since)
    print(f"app log warnings or errors since the relaunch: {len(problems)}")
    for problem in problems[:10]:
        print("  ", problem)
    print(run("ps", "-o", "lstart=,comm=", "-p", str(new_app)).stdout.strip())
    for failure in failures:
        print("FAIL:", failure)
    print("RESULT:", "ok" if not failures else "check")
    return 0 if not failures else 2


if __name__ == "__main__":
    sys.exit(main())
