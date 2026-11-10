# Session board

The session board is the list of agent sessions a Herdr client draws: every device, its spaces, one row per tab, the order the rows want attention in, and the tabs waiting on an answer. This crate computes it from daemon snapshots and has no GUI dependency. The desktop sidebar calls its functions directly. `herdr-board-server` publishes the same board as JSON for a client in another process, such as a phone app.

## Computing a board

1. Describe each device as a `Source`: its id, label, and latest `ClientShellSnapshot` if it has one.
2. Call `settle` with an `AttentionLedger` you keep between calls. The ledger remembers when each tab entered its place, which is what orders rows of equal urgency.
3. Call `build` to get a `Board`. Set `Board::revision` yourself before you publish it.

Both calls take a `mark` function. It turns a tab's daemon status into the `Mark` to show, so a caller that knows more than the daemon can say so. The desktop app marks a tab `limited` when its plan is used up and `monitoring` when a shell is left running. A caller without that knowledge returns `Mark::Status(status)`.

The functions `build` is made of are public too: `listed_tabs`, `sort_by_attention`, `waiting_elsewhere`, `tab_title`, `workspace_entries` and the rest. A client that draws its own list uses those and skips `Board`.

## Serving a board

```text
herdr-board-server --port 7480 [--label NAME] [--saved-devices] [--ssh LABEL=TARGET]...
```

The server follows the Herdr daemon of the machine it runs on. With `--saved-devices` it also follows the ssh devices saved in the Herdr app, and each `--ssh` adds one more. It attaches to a daemon the way the app does, as a client whose surface is inactive. It receives snapshots, never takes part in sizing a terminal, and cannot send input. It reconnects on its own, waiting at most 30 seconds between attempts.

It listens on `127.0.0.1` only and authenticates nobody. A request whose `Host` header names anything but `127.0.0.1` or `localhost` gets `403`. Put a proxy that authenticates callers in front of it to reach it from another machine, and have the proxy send a loopback `Host`.

| Request | Answer |
| --- | --- |
| `GET /board` | `200` with the current board. |
| `GET /board?after=N&wait=S` | `200` with the board once its revision is greater than `N`. `204` with no body when `S` seconds pass first. `S` defaults to 25 and is capped at 60. |
| `GET /events` | A `text/event-stream`. It sends the current board at once and every later board as it is published. |

Each event on the stream has the name `board`, the revision as its id, and the board's JSON on one `data` line. A comment line arrives every 15 seconds while nothing changes.

```text
event: board
id: 1794300000123
data: {"version":1,"revision":1794300000123,"devices":[...],...}
```

Every answer closes its connection. Long polls and streams are limited to 64 at a time. A request past that gets `503`.

## The JSON contract

```json
{
  "version": 1,
  "revision": 1794300000123,
  "devices": [
    {
      "id": "local",
      "label": "Mini",
      "status": "connected",
      "detail": null,
      "state": "blocked",
      "spaces": [
        {
          "id": "w1",
          "label": "herdr",
          "number": 1,
          "child": false,
          "state": "blocked",
          "focused": true,
          "tabs": [
            {
              "id": "w1:t2",
              "device_id": "local",
              "space_id": "w1",
              "pane_id": "w1:p3",
              "terminal_id": "w1:p3",
              "number": 2,
              "title": "Fix the sidebar width",
              "agent": { "id": "claude", "name": "Claude Code" },
              "state": "blocked",
              "state_text": "needs approval",
              "symbol": "×",
              "attention_rank": 0,
              "focused": false
            }
          ]
        }
      ]
    }
  ],
  "attention": [{ "device_id": "local", "tab_id": "w1:t2" }],
  "waiting": [{ "row": { "id": "w1:t2", "...": "..." }, "place": "herdr" }],
  "usage": []
}
```

### Board

| Field | Meaning |
| --- | --- |
| `version` | The contract's version, `1`. Refuse a version you do not know. |
| `revision` | A number that grows each time anything else in the board changes. It never falls, also across a server restart. It does not start at 1 and its steps are not always 1. It fits a double exactly. |
| `devices` | Every device, in a fixed order. The server's own machine is first. |
| `attention` | A reference to every tab row on every device, the row that most wants attention first. Draw this order for a list sorted by attention. |
| `waiting` | The rows whose agent is blocked on an answer, the latest to start waiting first. Each has the full row and a `place` tag. |
| `usage` | Plan usage per agent. The server sends an empty list, because the readings come from the desktop app's signed-in accounts. |

### Device

| Field | Meaning |
| --- | --- |
| `id` | `local`, or `ssh:<id>` for an ssh device. Stable across restarts. |
| `label` | The name to show. |
| `status` | `connected`, `connecting`, `disconnected` or `disabled`. |
| `detail` | Why the device is not connected, when the connection said. Otherwise `null`. |
| `state` | The most urgent state among its tabs. |
| `spaces` | Its spaces in sidebar order. Empty while the device has sent no snapshot. The server drops a device's snapshot when its connection ends. |

### Space

| Field | Meaning |
| --- | --- |
| `id` | The daemon's workspace id, unique within its device. |
| `label` | The name to show. A worktree shows its branch. |
| `number` | The space's number on its device, from one. |
| `child` | `true` for a linked worktree. Draw it indented under the row before it that is not a child. |
| `state` | The daemon's state for the space. |
| `focused` | The space in front on its device. |
| `tabs` | Its tab rows in tab order. |

### Tab row

| Field | Meaning |
| --- | --- |
| `id` | The daemon's tab id, unique within its device. `device_id` and `id` together name a row. |
| `device_id`, `space_id` | Where the row belongs. |
| `pane_id` | The pane the row stands for: its agent's pane, else the tab's focused pane. `null` when the tab reports no pane. |
| `terminal_id` | What `herdr terminal session control` and `observe` take to open the row's terminal on its device. `null` when `pane_id` is. |
| `number` | The daemon's number for the tab, the one its terminal client shows. |
| `title` | The row's text: the tab's own name when the user gave one, else its agent's terminal title, else the agent's name, else the name of the shell's directory, else the tab's label. |
| `agent` | `null` for a tab with no agent. `id` is the agent's kind, such as `claude` or `codex`, and names its icon. It can be `null`. `name` is the agent's display name. |
| `state` | `blocked`, `limited`, `working`, `done`, `monitoring`, `idle` or `unknown`. |
| `state_text` | The words to show for the state. The agent's own words when it gave any, else the same as `state`. |
| `symbol` | The one-character mark the terminal client shows for the state. |
| `attention_rank` | The row's urgency, lowest first: `0` blocked, `1` limited, `2` working, `3` done, `4` monitoring, `5` idle, `6` unknown. |
| `focused` | The tab in front within its space. |

`title`, `label`, `detail`, `state_text` and `place` are terminal text. Control characters are removed and each is cut to 200 characters. Treat them as plain text, never as markup.

### Waiting row

| Field | Meaning |
| --- | --- |
| `row` | The tab row, the same object as under its space. |
| `place` | A short tag for the row: its space, then ` · ` and its device when the device is not the first. Each name is cut to 16 characters. |

### Usage row

| Field | Meaning |
| --- | --- |
| `device_id` | The device the reading was taken on. |
| `agent` | The agent kind the limits belong to, as in a tab row's `agent.id`. |
| `name` | The name to show. |
| `limited` | `true` when a window has nothing left. |
| `gauges` | One per window to show. `label` is short (`5h`, `wk`), `title` is long, `percent` is the share used from 0 to 100, `severity` is `normal`, `warning` or `critical`, and `resets_at` is in Unix seconds or `null`. |

## Following the board

1. `GET /board` and draw it. Keep its `revision`.
2. `GET /board?after=<revision>`. On `200`, draw the new board and keep its revision. On `204`, ask again.
3. If a request fails, wait and go to step 1.

A board is always complete. A client never merges two of them.
