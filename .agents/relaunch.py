#!/usr/bin/env python3
"""Quit and reopen the installed Herdr.app, proving the daemons and every agent process survive it.

usage: python3 .agents/relaunch.py [--reason TEXT] [--no-countdown] [scratch run dir]

Every Herdr window first shows a five second countdown with a Cancel button. When Oscar cancels,
the script waits five minutes and asks again, up to twelve times. One relaunch runs at a time on a
Mac, and a relaunch is skipped when the running app already started from the installed build.
--no-countdown is only for a running app that predates the countdown.

With a scratch run dir, each reopened window is screenshotted into its screenshots/ folder.
Exit 0 when every check passed or the app was already current, 1 when a check before the quit
stopped the relaunch, 2 when a check after it failed, 3 when Oscar cancelled every countdown.
"""
import argparse
import fcntl
import hashlib
import json
import os
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
LOCK = Path.home() / ".local/state/herdr-relaunch.lock"
MINI = "oz-mini"
STUDIO = "oz-ms"
COUNTDOWN_SECONDS = 5
RETRY_AFTER = 300
TRIES = 12
AGENT = re.compile(r"(^|/)(claude|codex|kimi-code|kimi|agent)$")
HELPERS = ("ssh", "sh")


def run(*args, timeout=30):
    return subprocess.run(args, capture_output=True, text=True, check=False, timeout=timeout)


def app_pids():
    return [int(pid) for pid in run("pgrep", "-x", "Herdr").stdout.split()]


def local_server_pid():
    # -a: a session running in a herdr pane has the server as an ancestor, which pgrep skips by default
    out = run("pgrep", "-af", f"^{HERDR} server$").stdout.split()
    return int(out[0]) if out else None


def mini_server_pid():
    out = run("ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", MINI, "pgrep -f 'herdr server$'").stdout.split()
    return int(out[0]) if out else None


def process_table(host=None):
    command = ("ps", "-axo", "pid=,ppid=,comm=")
    if host:
        command = ("ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", host, " ".join(command))
    table = {}
    for row in run(*command).stdout.splitlines():
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


def studio_server():
    """The Studio daemon's pid and its agent processes, or (None, set()) when it does not answer."""
    table = process_table(STUDIO)
    servers = [pid for pid, (_, comm) in table.items() if comm.endswith("/herdr")]
    for pid in servers:
        agents = agent_pids(pid, table)
        if agents:
            return pid, agents
    return (servers[0] if servers else None), set()


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


def app_log(since):
    if not APP_LOG.exists():
        return []
    records = []
    for line in APP_LOG.read_text(errors="replace").splitlines()[-4000:]:
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("timestamp", "").strip("[]") >= since:
            records.append(record)
    return records


def app_log_problems(records):
    return [
        f"{record['timestamp'].strip('[]')} {record['level']} {record.get('message')} {record.get('fields', '')}"[:200]
        for record in records
        if record.get("level") in ("ERROR", "WARN")
    ]


def started_at(pid):
    out = subprocess.run(
        ("ps", "-o", "lstart=", "-p", str(pid)), capture_output=True, text=True, check=False, env={**os.environ, "LC_ALL": "C"}
    ).stdout.strip()
    return time.mktime(time.strptime(out, "%a %b %d %H:%M:%S %Y")) if out else None


def runs_installed_build(pid):
    """True when the app started after the installed binary was last written."""
    started = started_at(pid)
    return started is not None and started >= APP_BINARY.stat().st_ctime


def countdown(reason):
    """Asks every window for a countdown: "proceed", "cancelled", or an error message."""
    command = [str(APP_BINARY), "relaunch", "countdown", "--seconds", str(COUNTDOWN_SECONDS), "--reason", reason]
    result = run(*command, timeout=COUNTDOWN_SECONDS + 20)
    if result.returncode == 0:
        return "proceed"
    if result.returncode == 4:
        return "cancelled"
    return (result.stderr or result.stdout).strip() or f"exit {result.returncode}"


def take_lock():
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    handle = LOCK.open("w")
    try:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print("another relaunch is running on this Mac, waiting for it to finish")
        fcntl.flock(handle, fcntl.LOCK_EX)
    return handle


def desktop_lines(records):
    return [
        record.get("message")
        for record in records
        if record.get("message") in ("Window back on its desktop", "Saved desktop no longer exists", "Window did not move to its saved desktop")
    ]


def screenshot(rows, scratch):
    folder = scratch / "screenshots"
    folder.mkdir(parents=True, exist_ok=True)
    for row in rows:
        run("screencapture", "-x", "-l", row[0], str(folder / f"window-{row[0]}.png"))
    print(f"screenshots: {folder}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reason", default="A new Herdr build is installed.")
    parser.add_argument("--no-countdown", action="store_true")
    parser.add_argument("scratch", nargs="?")
    args = parser.parse_args()
    scratch = Path(args.scratch).resolve() if args.scratch else None
    on_studio = run("scutil", "--get", "LocalHostName").stdout.strip() == STUDIO
    lock = take_lock()
    tries = 0
    while True:
        app = app_pids()
        if len(app) != 1:
            print(f"expected one Herdr process, found {app}", file=sys.stderr)
            return 1
        if runs_installed_build(app[0]):
            print("the running app already started from the installed build, nothing to relaunch")
            return 0
        before = before_quit(app[0], on_studio)
        if isinstance(before, str):
            print(before, file=sys.stderr)
            return 1
        if args.no_countdown:
            break
        answer = countdown(args.reason)
        if answer == "proceed":
            print("countdown ran out")
            break
        if answer != "cancelled":
            print(f"the countdown failed: {answer}", file=sys.stderr)
            print("an app that predates the countdown needs one relaunch with --no-countdown", file=sys.stderr)
            return 1
        tries += 1
        if tries >= TRIES:
            print(f"Oscar cancelled all {TRIES} countdowns, the relaunch is still pending")
            return 3
        print(f"Oscar cancelled the countdown, asking again in {RETRY_AFTER // 60} minutes ({tries} of {TRIES})", flush=True)
        time.sleep(RETRY_AFTER)
    result = relaunch(before, on_studio, scratch)
    lock.close()
    return result


def before_quit(app, on_studio):
    """What the checks after the relaunch compare against, or why the relaunch must not happen."""
    table = process_table()
    children = app_children(app, table)
    foreign = {pid: comm for pid, comm in children.items() if comm.rsplit("/", 1)[-1] not in HELPERS}
    if foreign:
        return f"the app owns processes other than its ssh and sh helpers, stopping: {foreign}"
    local_server = local_server_pid()
    agents = agent_pids(local_server, table) if local_server else set()
    # On the Studio the local daemon is the Studio's.
    studio, studio_agents = (None, set()) if on_studio else studio_server()
    if local_server is None or not (agents or studio_agents):
        return "could not read the local daemon or its agents, stopping"
    return {
        "app": app,
        "children": len(children),
        "local_server": local_server,
        "mini_server": mini_server_pid(),
        "agents": agents,
        "studio": studio,
        "studio_agents": studio_agents,
        "status": server_status(),
        "saved": len(json.loads(STATE.read_text())),
        # Window titles need Screen Recording, which an ssh session does not have.
        "titled": len(windows()),
    }


def relaunch(before, on_studio, scratch):
    failures = []
    app, local_server, mini_server = before["app"], before["local_server"], before["mini_server"]
    agents_before, studio_before, studio_agents = before["agents"], before["studio"], before["studio_agents"]
    status_before, saved, titled = before["status"], before["saved"], before["titled"]
    print(f"app pid before: {app}, children: {before['children']}")
    print(f"local server pid: {local_server}, mini server pid: {mini_server or 'unreachable'}")
    print(f"studio server pid: {'this Mac' if on_studio else studio_before or 'unreachable'}")
    print(f"agent processes under the local server: {len(agents_before)}, under the studio server: {len(studio_agents)}")
    print(f"saved windows: {saved}, titled windows now: {titled}")

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

    if not titled:
        print("window titles are unreadable from this session, so windows are not checked or screenshotted")
    elif not wait(lambda: len(windows()) >= saved, 60, step=0.5):
        failures.append(f"windows did not come back: {len(windows())} titled of {saved} saved")
    # A title lags behind the space its window shows, so the screenshots prove the spaces.
    rows = windows() if titled else []
    for row in rows:
        print("  ", "\t".join(row))
    if scratch and rows:
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
    studio_after, _ = (None, set()) if on_studio else studio_server()
    studio_lost = studio_agents - set(process_table(STUDIO)) if studio_after else set()
    if not on_studio:
        print(f"studio server pid after: {studio_after or 'unreachable'}, studio agents still alive: {len(studio_agents - studio_lost)} of {len(studio_agents)}")
    if studio_before and studio_after != studio_before:
        failures.append("the studio server pid changed")
    if studio_lost:
        failures.append(f"studio agent processes gone: {sorted(studio_lost)}")
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
    # Windows go back to their desktops a moment after they open.
    wait(lambda: len(desktop_lines(app_log(since))) >= saved, 10, step=0.5)
    records = app_log(since)
    desktops = desktop_lines(records)
    print(f"windows back on their saved desktops: {desktops.count('Window back on its desktop')} of {saved} saved")
    if desktops.count("Saved desktop no longer exists"):
        print(f"windows whose saved desktop no longer exists: {desktops.count('Saved desktop no longer exists')}")
    problems = app_log_problems(records)
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
