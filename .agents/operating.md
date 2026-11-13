# Operating Herdr on Oscar's Macs

This file is for an agent session asked to change Herdr: fix a bug, add a feature, adjust a launcher or a key. It says what runs where, what a change needs before Oscar sees it, and how a relaunch is offered and performed. The engineering rules stay in `AGENTS.md`.

## Two programs

Herdr is two programs. The daemon, `herdr server`, owns every space, tab, pane, terminal and agent process. It is built from `~/tools/herdr`, a detached checkout of tag v0.9.3, with `ZIG=/opt/homebrew/opt/zig@0.16/bin/zig cargo build --release --locked`. `~/.local/bin/herdr` links to its release binary. That checkout is a read-only reference for this repository.

The app, `Herdr.app`, is this repository. It is a client of the daemon with bundle id `so.pen.herdr-gpui.dev`, installed at `/Applications/Herdr.app`, and a login item. Quitting it leaves the daemon and every terminal running. Opening it starts the local daemon when none is running, so the daemon is started by opening the app, never from a shell.

Neither program is installed through Homebrew. Do not reinstall either with it.

## Three Macs

| machine | runs | where the app reaches it |
| --- | --- | --- |
| oz-mbp, the MacBook | the app, a daemon, the checkout this guide is for, a signing keychain | local sockets |
| oz-ms, the Mac Studio | the daemon that runs nearly every agent session, from `~/.local/bin/herdr`, and a second copy of the app with its own signing keychain | ssh alias `oz-studio`, saved in the app as device "Studio", endpoint id `ssh:b5b318656ee02cc992706a4dae809c32` |
| oz-mac-mini | the daemon only, from a copy of the same `herdr` binary at `~/.local/bin/herdr` | ssh alias `oz-mini`, saved in the app as device "Mini", endpoint id `ssh:93d87f03a0dd2de8c4df77aa633270d0` |

The app runs on the MacBook, where Oscar works, and on the Studio, where he watches the Studio's daemon on its own screen. The Studio's app is opened on demand and is not a login item. It shows the Studio's daemon as the device "Local". The MacBook's daemon, started by the app on 2026-10-05, reports no detached-daemon support, so `herdr machine add oz-mbp` on the Studio refuses it until that daemon restarts. The Mini has no app and no signing keychain. The Studio's checkout of this repository is on upstream `main` with a `laptop` remote, and the Studio builds the app in the plain clone `~/.agents/worktrees/herdr-gpui/studio-app`, detached at the commit the MacBook runs. The app keeps ssh connections to each remote daemon: one `herdr remote-client-bridge` per window plus helper shells. After a reboot the first bridge starts that Mac's daemon, detached in its own session with its standard streams on `/dev/null`, so the daemon outlives the app and the ssh session that started it. Updating a remote daemon means copying a new binary there and restarting it, which Oscar does.

`device_order` in the local app config lists the Studio first, then this Mac, the Mini and the dev box, in the sidebar, the launcher's device prompt and the devices menu.

A fourth device, "Devbox contributors", is the SetLife dev box `devbox-netcup-01` reached as the `contributors` account through the ssh alias `devbox-contributors`, endpoint id `ssh:c8678aa217fbfcd65bb63f367fa3b0fa`. It runs the daemon from the official v0.9.3 `herdr-linux-x86_64` release binary at `~/.local/bin/herdr`. A daemon runs as one account and its shells run as that account, so each account on a box is a separate device. The directories the launcher chords name, `~/tools`, `~/fedibtc/fedi` and `~/WorkSTEPS`, do not exist there. The `control-setlife-devbox` skill covers the machine itself.

Oscar keeps one macOS desktop per project and one app window per space: tools, fedi, setlife-network and worksteps. Agent sessions run as tabs in those spaces, mostly on the Studio.

## The repository

| remote | repository | role |
| --- | --- | --- |
| origin | `otech47/herdr-gpui`, public | Oscar's fork, where work is pushed |
| upstream | `penso/herdr-gpui` | the project this client is forked from |

Local `main` holds Oscar's work. It is pushed with `git -C /Users/oscarlafarga/tools/herdr-gpui push origin main:ol/main`, because the fork's `main` on GitHub is upstream's untouched copy and only Oscar replaces or renames it. The Studio also pushes to `ol/main`, and local `main` tracks upstream, so start work with `git -C /Users/oscarlafarga/tools/herdr-gpui pull --ff-only origin ol/main`. Finished work is committed and pushed in the same turn. Every git command runs as `git -C <absolute path> <subcommand>` in its own call, never behind `cd`. The commit message is written to a file and committed with `git commit -q -F <file>`. No amend, no force push, no AI attribution lines.

The fork is public. A commit never contains the signing keychain or its password file, a token, machine-local state, or terminal output.

## Files on disk

| path | what it is | edit it |
| --- | --- | --- |
| `~/.config/herdr/config.toml` | the daemon's config: its key commands, `version_check` off | yes, then `herdr server reload-config` |
| `~/.config/herdr/config-gpui.toml` | the app's managed defaults, rewritten on every start and reload | never |
| `~/.config/herdr/config-gpui.local.toml` | the app's overrides: layout, the three sidebar settings, keybindings, `[[launchers]]`. studio-hub keeps it equal on every Mac, newest copy wins | yes, it reloads on save |
| `~/.config/herdr/herdr.sock` | the daemon's JSON API socket, used by the `herdr` command | no |
| `~/.config/herdr/herdr-client.sock` | the daemon's binary client socket, used by the app | no |
| `~/.config/herdr/session.json` | the daemon's spaces, tabs and panes, restored when it starts | no |
| `~/.config/herdr/herdr-server.log` | the daemon's log, UTC timestamps | no |
| `~/.local/state/herdr/gpui/window-state.json` | one entry per window: space, device, bounds | only when Oscar asks |
| `~/.local/state/herdr/gpui/local-<hash>.json` | sidebar chrome: width, split, folded sections, tabs view | only when Oscar asks |
| `~/.local/state/herdr/gpui/browser-tabs.json`, `editor-groups.json` | the app's browser tabs and editor groups | no |
| `~/.local/state/herdr/gpui/logs/herdr-gpui.jsonl` | the app's log | no |
| `~/.local/bin/herdr-launch` | the launcher script the ctrl-cmd chords run, read on every press. studio-hub copies the Studio's version to the other Macs | yes, on the Studio |
| `~/Library/Keychains/herdr-local-signing.keychain-db` and `.password` | this Mac's signing identity, created by the first `just bundle` | never, never in git |

The launcher chords are `[[launchers]]` entries in the local app config: ctrl-cmd with 2, 3, 4 for claude, q, w, e for codex and a, s, d for kimi, in tools, fedi and worksteps. A press asks for a device, then runs the script through `sh -c` with `HERDR_LAUNCH_DEVICE`, `HERDR_LAUNCH_DEVICE_LABEL`, `HERDR_LAUNCH_HOST` for a remote device, `HERDR_ACTIVE_WORKSPACE_ID` and `HERDR_SOCKET_PATH` pointing at the JSON API socket. The script creates a tab in the named space, asks the app to show it with `/Applications/Herdr.app/Contents/MacOS/Herdr workspace focus <space id> [--tab TAB] [--device ENDPOINT]`, waits for the shell prompt and types the command. Chords 5 and 6 stay daemon key commands in `config.toml`.

## What a change needs

| change | takes effect |
| --- | --- |
| `~/.local/bin/herdr-launch` | on the next press |
| `config-gpui.local.toml`: launchers, keybindings, layout, settings | on save |
| `config.toml` key commands | after `herdr server reload-config` |
| a menu shortcut override with `defaults write so.pen.herdr-gpui.dev NSUserKeyEquivalents` | within a minute |
| any code in this repository | after `just bundle release`, the install, and an app relaunch |
| daemon code in `~/tools/herdr` | after `herdr server stop` and reopening the app, on each Mac, by Oscar |
| a deleted signing keychain | after Oscar grants App Management and Full Disk Access again |

## Build, verify, install

```sh
just format
just ci
just bundle release
ditto target/release/Herdr.app /Applications/Herdr.app
```

`just ci` runs the format check, the file size limit, clippy and the workspace tests. `cargo nextest` is not installed. A focused run is `cargo test --locked -p herdr-gpui --bin herdr-gpui -- <filter>`. The test `login_env::unix::tests::timeout_kills_shell_and_descendant_group` fails now and then under the full suite's load and passes alone, so rerun it alone before calling the gate red. After editing `config-gpui.example.toml`, rerun the provider settings test with `HERDR_BLESS_EXAMPLE=1`.

`just bundle` signs the bundle with this Mac's own certificate, so the app's privacy grants survive a reinstall. The executable inside the bundle is `Contents/MacOS/Herdr`.

On the Studio, `cargo` on PATH ignores `rust-toolchain.toml`, so the build runs with the pinned toolchain first on PATH:

```sh
T=~/.rustup/toolchains/1.96.1-aarch64-apple-darwin/bin
env PATH="$T:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin" just bundle release
```

A build from a linked worktree draws a yellow strip with its branch name under the titlebar, so the Studio's app is built in its plain clone. The clone borrows the objects of the Studio's checkout, so a fetch there is enough to move it to the MacBook's commit:

```sh
git -C ~/tools/herdr-gpui fetch origin ol/main
git -C ~/.agents/worktrees/herdr-gpui/studio-app checkout -q --detach <commit>
```

No app on the Studio holds the screen recording grant, so `screencapture` from an agent there fails with "could not create image from window".

Run embedded browser checks from the signed app bundle. A bare Cargo executable does not exercise the bundle's App Transport Security policy. Include an HTTP report and its interactive content in the native check.

A visible change is verified on the real window, not only in headless layout tests. `HERDR_TEST_SIDEBAR_HOLD=all` (or `by-device`, `device`) with the app's `--sidebar-test` flag holds a two-device sidebar fixture open. Run it under `env -i` with HOME and the XDG config and state dirs pointed at a scratch sandbox, so the held app never reads or writes the real config and state. Screenshot a window with `screencapture -x -l <window id> out.png`, with the id from the lister in `.agents/herdr-windows.swift`. System Events sees no gpui window. Keep screenshots, logs and scratch scripts in a `scratch new` run dir.

## Relaunching the app

A relaunch quits the app and opens it again. Every daemon keeps running with every space, tab, pane and agent process: the app's only child processes are its ssh connections to the remote daemons and helper shells, every agent process descends from `herdr server`, and every daemon is detached from any client. The windows come back at their saved bounds, each on its saved space and device, on the desktop currently visible on their display. Oscar drags them to their project desktops himself. Focus moves to the app for a moment, and the ssh connections to the Studio and the Mini reconnect. A window's title can lag behind the space it shows, so the sidebar in a screenshot is the ground truth, not the title.

The protocol after installing a build:

1. Ask one question, in these words: "Ready to relaunch Herdr now, yes or no?" Add the one-line basis: the daemons own every agent, the app owns nothing but its ssh connections. Never write "relaunch when you want", and never relaunch without the yes. A yes inside the request itself, such as "fix it and relaunch", counts.
2. On yes, run `python3 .agents/relaunch.py <scratch run dir>` and report what it printed: the local, Studio and Mini daemon pids unchanged, every agent process on this Mac and the Studio alive, the new pid running the installed binary and its hash, as many windows as saved entries, the screenshots. The script stops before quitting when the app owns anything beyond its helpers, or when it cannot read the local daemon and its agents. Then report what it found and ask.
3. When a change needs more than an app relaunch, say what it needs and ask instead of doing it. That covers a daemon rebuild or restart on any Mac, a binary copy to the Studio or the Mini, deleting the signing keychain, editing a state file, and anything that touches a pane or a tab. Oscar does those or says who does.

## Never

- Stop or restart `herdr server` on any Mac. `herdr server reload-config` is fine.
- Run `herdr integration install`. The session hooks live in `~/.agents/hooks`.
- Use the word "demo" anywhere in Herdr: code, config, docs, UI text.
- Edit a file under `~/.local/state/herdr/gpui` unless Oscar asks for it in that turn.
- Drive the mouse or keyboard while Oscar is active. A scripted native step runs only after 60 seconds of idle, and a scripted press of a launcher chord is never a proof that the launcher runs.
- Close, resize or act on the tab or pane the session itself runs in.
- Relaunch the app unasked, or leave the relaunch to Oscar after he said yes.
- Modify `~/tools/herdr`, or reinstall either program through Homebrew.
- Create a worktree inside or beside the repository. Worktrees live under `~/.agents/worktrees`.
