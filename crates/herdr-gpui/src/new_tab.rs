//! The prompt a new tab opens with when `[new_tab]` is set. Each row starts
//! on a guess from the window, so Enter alone usually opens the tab wanted.

mod render;
#[cfg(test)]
mod tests;

use crate::{
    HerdrWindow,
    config::{NewTabAgent, NewTabSpace},
    controls::Command,
    launcher::{Choice, plan},
    menu::Page,
};
use gpui::{Context, Global, KeyDownEvent, Window};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prompt {
    /// Index into `launcher_choices`.
    pub device: usize,
    pub agent: usize,
    pub space: usize,
    /// The configured spaces, then the window's own when it is not one of them.
    pub spaces: Vec<NewTabSpace>,
}

/// The agent key last opened in each space, shared by every window, so the
/// guess follows how each space is used.
#[derive(Default)]
struct Recent(HashMap<String, char>);

impl Global for Recent {}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `<script> <space> <dir> [<wrap> <agent command>]`. A shell passes no command.
pub(crate) fn command_line(script: &str, space: &NewTabSpace, agent: &NewTabAgent) -> String {
    let mut line = format!("{script} {} {}", quote(&space.label), quote(&space.dir));
    if !agent.command.is_empty() {
        let text = if space.wrap.is_empty() {
            agent.command.clone()
        } else {
            format!("{} {}", space.wrap, agent.command)
        };
        line.push(' ');
        line.push_str(&quote(&text));
    }
    line
}

impl HerdrWindow {
    fn window_space(&self) -> Option<(String, String)> {
        let snapshot = self.live.snapshot.as_ref()?;
        let focused = snapshot.focused_workspace_id.as_ref()?;
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| &workspace.workspace_id == focused)?;
        Some((workspace.label.clone(), workspace.new_workspace_cwd.clone()))
    }

    pub(crate) fn open_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(new_tab) = &self.config.new_tab else {
            return;
        };
        let mut spaces = new_tab.spaces.clone();
        let here = self.window_space();
        if let Some((label, dir)) = &here
            && !spaces.iter().any(|space| &space.label == label)
        {
            spaces.push(NewTabSpace {
                label: label.clone(),
                dir: if dir.is_empty() {
                    "~".into()
                } else {
                    dir.clone()
                },
                wrap: String::new(),
            });
        }
        if spaces.is_empty() {
            self.request_command(Command::Tab, window, cx);
            return;
        }
        let space = here
            .and_then(|(label, _)| spaces.iter().position(|space| space.label == label))
            .unwrap_or(0);
        let recent = cx
            .try_global::<Recent>()
            .and_then(|recent| recent.0.get(&spaces[space].label).copied());
        let agent = recent
            .and_then(|key| new_tab.agents.iter().position(|agent| agent.key == key))
            .or_else(|| {
                new_tab
                    .agents
                    .iter()
                    .position(|agent| !agent.command.is_empty())
            })
            .unwrap_or(0);
        let shown = &self.endpoints[self.selected_endpoint].id;
        let device = self
            .launcher_choices()
            .iter()
            .position(|choice| &choice.id == shown)
            .unwrap_or(0);
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::NewTab);
        self.menu.new_tab = Some(Prompt {
            device,
            agent,
            space,
            spaces,
        });
    }

    pub(crate) fn new_tab_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let Some(mut prompt) = self.menu.new_tab.clone() else {
            self.dismiss_menu(window, cx);
            return;
        };
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let devices = self.launcher_choices().len();
        let spaces = prompt.spaces.len();
        match event.keystroke.key.as_str() {
            "escape" => {
                self.dismiss_menu(window, cx);
                return;
            }
            "enter" => {
                self.confirm_new_tab(window, cx);
                return;
            }
            "left" => prompt.space = (prompt.space + spaces - 1) % spaces,
            "right" => prompt.space = (prompt.space + 1) % spaces,
            key => {
                if let Some(row) = key
                    .parse::<usize>()
                    .ok()
                    .filter(|row| (1..=devices).contains(row))
                {
                    prompt.device = row - 1;
                } else if let Some(index) = self.config.new_tab.as_ref().and_then(|new_tab| {
                    let mut chars = key.chars();
                    let key = chars.next().filter(|_| chars.next().is_none())?;
                    new_tab.agents.iter().position(|agent| agent.key == key)
                }) {
                    prompt.agent = index;
                } else {
                    return;
                }
            }
        }
        self.menu.new_tab = Some(prompt);
        cx.notify();
    }

    pub(crate) fn pick_new_tab(&mut self, pick: impl FnOnce(&mut Prompt), cx: &mut Context<Self>) {
        if let Some(prompt) = &mut self.menu.new_tab {
            pick(prompt);
            cx.notify();
        }
    }

    /// Opens the prompt's tab. A plain shell in this window's space on its
    /// own device is an ordinary new tab; anything else runs the script.
    pub(crate) fn confirm_new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.menu.new_tab.take() else {
            return;
        };
        self.dismiss_menu(window, cx);
        let Some(new_tab) = self.config.new_tab.clone() else {
            return;
        };
        let choices = self.launcher_choices();
        let (Some(choice), Some(space), Some(agent)) = (
            choices.get(prompt.device),
            prompt.spaces.get(prompt.space),
            new_tab.agents.get(prompt.agent),
        ) else {
            return;
        };
        cx.default_global::<Recent>()
            .0
            .insert(space.label.clone(), agent.key);
        let here = choice.id == self.endpoints[self.selected_endpoint].id
            && self
                .window_space()
                .is_some_and(|(label, _)| label == space.label);
        if agent.command.is_empty() && here {
            if !self.open_name_prompt(Command::Tab, window, cx) {
                self.request_command(Command::Tab, window, cx);
            }
            return;
        }
        self.launch_new_tab(&new_tab.command, choice, space, agent, cx);
    }

    fn launch_new_tab(
        &mut self,
        script: &str,
        choice: &Choice,
        space: &NewTabSpace,
        agent: &NewTabAgent,
        cx: &mut Context<Self>,
    ) {
        let (workspace, socket) = self.launch_context(choice);
        let line = command_line(script, space, agent);
        let plan = plan(&line, choice, workspace.as_deref(), socket.as_deref());
        let label = format!("{} in {}", agent.label, space.label);
        self.run_plan(plan, &label, cx);
    }
}
