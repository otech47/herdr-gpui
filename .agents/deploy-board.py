#!/usr/bin/env python3
"""Build herdr-board-server from this checkout and run it on the Mini, where Jarvis reads the board.

usage: python3 .agents/deploy-board.py [--check] [--force] [--owner SESSION]

The Mini records the commit its board server was built from. Nothing is built or copied when no
board code changed since that commit. --check only reports whether the Mini is behind.
Exit 0 when the Mini runs this checkout's board code, 1 when a step failed, 4 when --check finds
the Mini behind.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MINI = "oz-mini"
LABEL = "dev.oscar.herdr.board"
PORT = 7480
REMOTE_BINARY = ".local/bin/herdr-board-server"
DEPLOYED = "Library/Application Support/jarvis/board-server.commit"
BOARD_CODE = ("crates/herdr-board", "crates/herdr-board-server", "crates/herdr-client", "crates/herdr-protocol", "Cargo.lock")
# cargo on the Studio's PATH ignores rust-toolchain.toml
TOOLCHAIN = Path.home() / ".rustup/toolchains/1.96.1-aarch64-apple-darwin/bin"
BINARY = REPO / "target/release/herdr-board-server"


def run(*args, timeout=60):
    return subprocess.run(args, capture_output=True, text=True, check=False, timeout=timeout)


def ssh(command, timeout=30):
    return run("ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", MINI, command, timeout=timeout)


def git(*args):
    return run("git", "-C", str(REPO), *args)


def deployed():
    """The commit the Mini's board server was built from, "" when none is recorded, None when the Mini is unreachable."""
    out = ssh(f'cat "$HOME/{DEPLOYED}" 2>/dev/null; true')
    return None if out.returncode != 0 else out.stdout.strip()


def behind(commit, head):
    if not commit or git("cat-file", "-e", commit + "^{commit}").returncode != 0:
        return True
    return git("diff", "--quiet", commit, head, "--", *BOARD_CODE).returncode != 0


def board():
    out = ssh(f"curl -s -m 5 http://127.0.0.1:{PORT}/board", timeout=20)
    try:
        value = json.loads(out.stdout)
    except ValueError:
        return None
    return value if isinstance(value, dict) and value.get("version") == 1 else None


def statuses(value):
    return {device.get("label"): device.get("status") for device in (value or {}).get("devices") or []}


def build(owner):
    path = os.environ.get("PATH", "")
    env = dict(os.environ, PATH=f"{TOOLCHAIN}:{path}" if TOOLCHAIN.is_dir() else path)
    command = ["build-storage", "run", "--owner", owner, "--", "cargo", "build", "--locked", "--release",
               "--target-dir", "target", "-p", "herdr-board-server"]
    return subprocess.run(command, cwd=REPO, env=env, check=False).returncode == 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--owner", default=os.environ.get("CLAUDE_CODE_SESSION_ID") or os.environ.get("CODEX_THREAD_ID"))
    args = parser.parse_args()
    head = git("rev-parse", "HEAD").stdout.strip()
    commit = deployed()
    if commit is None:
        print("board server: the Mini did not answer over ssh", file=sys.stderr)
        return 1
    if not args.force and not behind(commit, head):
        print(f"board server: the Mini runs this checkout's board code, built from {commit[:8]}")
        return 0
    if args.check:
        print(f"board server: the Mini runs {commit[:8] or 'an unrecorded build'}, behind {head[:8]}. "
              "Run python3 .agents/deploy-board.py")
        return 4
    if git("status", "--porcelain", "--", *BOARD_CODE).stdout.strip():
        print("board server: the board code has uncommitted changes, commit them first", file=sys.stderr)
        return 1
    if not args.owner:
        print("board server: pass --owner with this session's id for build-storage", file=sys.stderr)
        return 1
    if not build(args.owner):
        print("board server: the build failed", file=sys.stderr)
        return 1
    digest = hashlib.sha256(BINARY.read_bytes()).hexdigest()
    before = statuses(board())
    if run("scp", "-q", "-o", "BatchMode=yes", str(BINARY), f"{MINI}:{REMOTE_BINARY}.new", timeout=120).returncode != 0:
        print("board server: the copy to the Mini failed", file=sys.stderr)
        return 1
    swap = ssh(f"chmod 755 {REMOTE_BINARY}.new && mv -f {REMOTE_BINARY}.new {REMOTE_BINARY} && "
               f"launchctl kickstart -k gui/$(id -u)/{LABEL} && shasum -a 256 {REMOTE_BINARY}")
    if swap.returncode != 0 or digest not in swap.stdout:
        print(f"board server: the swap on the Mini failed: {swap.stderr.strip()}", file=sys.stderr)
        return 1
    deadline = time.monotonic() + 90
    after = {}
    while time.monotonic() < deadline:
        after = statuses(board())
        if after and all(after.get(label) == "connected" for label, status in before.items() if status == "connected"):
            break
        time.sleep(2)
    ssh(f'printf %s {head} > "$HOME/{DEPLOYED}"')
    print(f"board server: the Mini runs {head[:8]}, sha256 {digest[:16]}")
    print("devices before:", before)
    print("devices after: ", after)
    if not after:
        print("RESULT: check, the board does not answer on the Mini")
        return 1
    lost = [label for label, status in before.items() if status == "connected" and after.get(label) != "connected"]
    print("RESULT:", "check, not connected again: " + ", ".join(lost) if lost else "ok")
    return 1 if lost else 0


if __name__ == "__main__":
    sys.exit(main())
