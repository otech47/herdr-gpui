//! `[new_tab]`: the prompt a new tab opens with, which picks the device, the
//! agent and the space, and the script that creates the tab.
use crate::{Error, Result};
use serde::Deserialize;

const MAX_ENTRIES: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewTab {
    /// Run as `<command> <space> <directory> [<agent command>]`, with the
    /// device in its environment as a `[[launchers]]` command gets it.
    pub command: String,
    pub spaces: Vec<NewTabSpace>,
    pub agents: Vec<NewTabAgent>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewTabSpace {
    pub label: String,
    pub dir: String,
    /// Put before every agent command started in this space, such as
    /// `nix develop -c`.
    pub wrap: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewTabAgent {
    pub key: char,
    pub label: String,
    /// Empty for a plain shell.
    pub command: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct NewTabSettings {
    command: String,
    spaces: Vec<SpaceSettings>,
    agents: Option<Vec<AgentSettings>>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SpaceSettings {
    label: String,
    dir: String,
    wrap: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AgentSettings {
    key: String,
    label: String,
    command: String,
}

fn default_agents() -> Vec<NewTabAgent> {
    [
        ('q', "Shell", ""),
        ('w', "Claude", "claude"),
        ('e', "Codex", "codex"),
        ('r', "Kimi", "kimi"),
        ('t', "Neon", "agy --model gemini-4-neon-medium"),
    ]
    .into_iter()
    .map(|(key, label, command)| NewTabAgent {
        key,
        label: label.to_owned(),
        command: command.to_owned(),
    })
    .collect()
}

pub(super) fn new_tab(settings: Option<NewTabSettings>) -> Result<Option<NewTab>> {
    let Some(settings) = settings else {
        return Ok(None);
    };
    let command = settings.command.trim();
    if command.is_empty() {
        return Err(Error::EmptyNewTabCommand);
    }
    if settings.spaces.len() > MAX_ENTRIES {
        return Err(Error::TooManyNewTabEntries(MAX_ENTRIES));
    }
    let mut spaces: Vec<NewTabSpace> = Vec::with_capacity(settings.spaces.len());
    for entry in settings.spaces {
        let label = entry.label.trim();
        let dir = entry.dir.trim();
        if label.is_empty() || dir.is_empty() {
            return Err(Error::NewTabSpace(entry.label));
        }
        if spaces.iter().any(|space| space.label == label) {
            return Err(Error::DuplicateNewTabSpace(label.to_owned()));
        }
        spaces.push(NewTabSpace {
            label: label.to_owned(),
            dir: dir.to_owned(),
            wrap: entry.wrap.trim().to_owned(),
        });
    }
    let agents = match settings.agents {
        None => default_agents(),
        Some(entries) => {
            if entries.is_empty() || entries.len() > MAX_ENTRIES {
                return Err(Error::TooManyNewTabEntries(MAX_ENTRIES));
            }
            let mut agents: Vec<NewTabAgent> = Vec::with_capacity(entries.len());
            for entry in entries {
                let mut chars = entry.key.trim().chars();
                let key = match (chars.next(), chars.next()) {
                    (Some(key), None) if key.is_ascii_lowercase() => key,
                    _ => return Err(Error::NewTabAgentKey(entry.key)),
                };
                if agents.iter().any(|agent| agent.key == key) {
                    return Err(Error::DuplicateNewTabAgentKey(entry.key));
                }
                let command = entry.command.trim().to_owned();
                let label = match entry.label.trim() {
                    "" if command.is_empty() => "Shell".to_owned(),
                    "" => command.clone(),
                    label => label.to_owned(),
                };
                agents.push(NewTabAgent {
                    key,
                    label,
                    command,
                });
            }
            agents
        }
    };
    Ok(Some(NewTab {
        command: command.to_owned(),
        spaces,
        agents,
    }))
}
