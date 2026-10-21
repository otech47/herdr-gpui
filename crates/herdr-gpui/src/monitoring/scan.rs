//! The step a host's shell runs to list its processes, and the text it prints.
//!
//! Herdr exports `HERDR_TAB_ID` into every pane it starts, and whatever the
//! pane's agent runs inherits it, so a process's environment names its tab
//! exactly. A shell under the agent is work the agent left running: a
//! background command, a watch on CI, a monitor. A shell or python process
//! that outlived its parent is a disowned timer. Either keeps a tab watched.
//!
//! macOS stops showing a shell's environment once the shell has changed it,
//! which bash does on its first `cd` or `export`, so a shell is placed by
//! its nearest ancestor whose environment still names a tab: the agent.

use crate::{Error, Result};
use std::collections::HashMap;

/// Process lines read from one answer; the rest are ignored.
const MAX_PROCESSES: usize = 8192;
/// Process names are compared, never shown.
const MAX_NAME: usize = 64;
/// Tab ids are short daemon tokens such as `w7V:t1A`.
const MAX_TAB_ID: usize = 64;
/// Longer socket paths than any Unix socket can have are not trusted.
const MAX_SOCKET: usize = 1024;
/// How far up a shell's ancestry is read; a deeper tree is a loop.
const MAX_DEPTH: usize = 64;

/// Prints `P <pid> <ppid> <name>` for every process, the name being the
/// last path component of its first argument, then `E <pid> <tab> <api
/// socket>` for each process started inside a Herdr pane, the socket naming
/// which daemon's pane that was, or `N` when the host can list neither.
/// Linux reads `/proc`, where `grep` finds the tagged environments in one
/// pass; macOS has `ps -E`, which prints each environment after its command.
/// An argument holding a newline spreads a process over several `ps` lines,
/// so a line is a new process only when it opens with two numbers.
/// The step runs in a long-lived shell, so it must not `exit`.
pub(crate) const COMMAND: &str = r#"PATH="$PATH:/usr/sbin:/sbin"
if [ -r /proc/self/environ ]; then
    ps -eo pid=,ppid=,args= 2>/dev/null | awk '
        $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ {
            n = split($3, part, "/"); name = part[n]; sub(/^-/, "", name)
            print "P", $1, $2, name
        }'
    for herdr_env in $(grep -lsa 'HERDR_TAB_ID=' /proc/[0-9]*/environ 2>/dev/null | head -n 512); do
        herdr_pid=${herdr_env#/proc/}
        herdr_pid=${herdr_pid%/environ}
        herdr_tab=$(tr '\0' '\n' < "$herdr_env" | sed -n 's/^HERDR_TAB_ID=//p' | head -n 1)
        herdr_socket=$(tr '\0' '\n' < "$herdr_env" | sed -n 's/^HERDR_SOCKET_PATH=//p' | head -n 1)
        printf 'E %s %s %s\n' "$herdr_pid" "$herdr_tab" "$herdr_socket"
    done
elif ps -E -o pid= -p $$ >/dev/null 2>&1; then
    ps -E -axww -o pid=,ppid=,command= 2>/dev/null | awk '
        function flush() {
            if (pid != "") print "P", pid, ppid, name
            if (tab != "") print "E", pid, tab, socket
        }
        $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ {
            flush()
            pid = $1; ppid = $2; tab = ""; socket = ""; from = 4
            n = split($3, part, "/"); name = part[n]; sub(/^-/, "", name)
        }
        {
            for (i = from; i <= NF; i++) {
                if (index($i, "HERDR_TAB_ID=") == 1) tab = substr($i, 14)
                else if (index($i, "HERDR_SOCKET_PATH=") == 1) socket = substr($i, 19)
            }
            from = 1
        }
        END { flush() }'
else
    echo N
fi
true"#;

/// A tab of one daemon, as a process's environment names it: the tab id,
/// and the JSON API socket of the daemon that owns it, empty when the
/// process did not say.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Tab {
    pub id: String,
    pub socket: String,
}

/// What keeps a tab watched while its agent waits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Watched {
    /// The names of the tab's processes a shell runs under, the agent among
    /// them when the shell is its own.
    pub under: Vec<String>,
    /// A shell or python process of the tab outlived its parent.
    pub disowned: bool,
}

/// Every watched tab, by tab.
pub(crate) type Watchers = HashMap<Tab, Watched>;

struct Process {
    parent: u32,
    name: String,
}

fn shell(name: &str) -> bool {
    matches!(
        name,
        "sh" | "bash" | "zsh" | "dash" | "fish" | "ksh" | "csh" | "tcsh"
    )
}

fn timer(name: &str) -> bool {
    shell(name) || name.starts_with("python")
}

/// Parses what [`COMMAND`] printed.
pub(crate) fn parse(text: &str) -> Result<Watchers> {
    let mut processes: HashMap<u32, Process> = HashMap::new();
    let mut tabs: HashMap<u32, Tab> = HashMap::new();
    for line in text.lines() {
        let mut words = line.splitn(4, ' ');
        match words.next() {
            Some("N") => return Err(Error::MonitoringTool),
            Some("P") if processes.len() < MAX_PROCESSES => {
                let (Some(pid), Some(parent), Some(name)) =
                    (words.next(), words.next(), words.next())
                else {
                    continue;
                };
                let (Ok(pid), Ok(parent)) = (pid.parse(), parent.parse()) else {
                    continue;
                };
                // macOS names its framework python `Python`.
                let name = name
                    .trim_end()
                    .chars()
                    .filter(|c| !c.is_control())
                    .map(|c| c.to_ascii_lowercase())
                    .take(MAX_NAME)
                    .collect();
                processes.insert(pid, Process { parent, name });
            }
            Some("E") => {
                let (Some(pid), Some(id)) = (words.next(), words.next()) else {
                    continue;
                };
                let id = id.trim_end();
                let plain = !id.is_empty()
                    && id.len() <= MAX_TAB_ID
                    && id.chars().all(|c| c.is_ascii_graphic() && c != '=');
                // Paths are compared, never opened; an unusable one is unknown.
                let socket = words
                    .next()
                    .map(str::trim_end)
                    .filter(|socket| {
                        socket.len() <= MAX_SOCKET && !socket.chars().any(char::is_control)
                    })
                    .unwrap_or_default();
                if let (Ok(pid), true) = (pid.parse(), plain) {
                    tabs.insert(
                        pid,
                        Tab {
                            id: id.to_owned(),
                            socket: socket.to_owned(),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    let mut watchers = Watchers::new();
    for (pid, tab) in &tabs {
        if let Some(process) = processes.get(pid)
            && process.parent == 1
            && timer(&process.name)
        {
            watchers.entry(tab.clone()).or_default().disowned = true;
        }
    }
    for (pid, process) in &processes {
        if !shell(&process.name) {
            continue;
        }
        let mut tab = tabs.get(pid);
        let mut under: Vec<String> = Vec::new();
        let mut parent = process.parent;
        for _ in 0..MAX_DEPTH {
            let Some(ancestor) = processes.get(&parent) else {
                break;
            };
            if let Some(owner) = tabs.get(&parent) {
                let tab = tab.get_or_insert(owner);
                if *tab == owner && !shell(&ancestor.name) && !under.contains(&ancestor.name) {
                    under.push(ancestor.name.clone());
                }
            }
            parent = ancestor.parent;
        }
        if let Some(tab) = tab.filter(|_| !under.is_empty()) {
            let watched = watchers.entry(tab.clone()).or_default();
            for name in under {
                if !watched.under.contains(&name) {
                    watched.under.push(name);
                }
            }
        }
    }
    Ok(watchers)
}
