//! The `[[launchers]]` chords: each runs a shell command on this computer for
//! a device chosen in a prompt. The chosen device reaches the command through
//! its environment, so one script can drive the local daemon or a saved
//! device's over SSH. A plain new tab never asks; only these chords do.

mod render;
#[cfg(test)]
mod tests;

use crate::{HerdrWindow, config::Launcher, menu::Page};
use gpui::{Context, KeyDownEvent, Keystroke, Window};
use herdr_client::ConnectTarget;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::process::{Command, Stdio};

/// The prompt a chord opened: which launcher, and the highlighted device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Prompt {
    pub launcher: usize,
    pub selected: usize,
}

/// A device the prompt lists: an enabled endpoint, by ID and label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub id: String,
    pub label: String,
    /// The SSH destination of a saved device; none for this computer.
    pub host: Option<String>,
}

/// What a launcher runs, with the variables that name its device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchPlan {
    pub command: String,
    pub env: Vec<(String, String)>,
}

/// The JSON API socket the `herdr` command line needs, `herdr.sock` beside
/// the app's `herdr-client.sock`. None for a client socket named some other way.
pub(crate) fn api_socket(client: &Path) -> Option<PathBuf> {
    let stem = client.file_stem()?.to_str()?.strip_suffix("-client")?;
    Some(client.with_file_name(format!("{stem}.sock")))
}

/// The command and environment a launcher runs with for `choice`:
/// `HERDR_LAUNCH_DEVICE` and `HERDR_LAUNCH_DEVICE_LABEL` always,
/// `HERDR_LAUNCH_HOST` for a saved device, and the focused space of the
/// device's daemon as `HERDR_ACTIVE_WORKSPACE_ID`, as the daemon's own
/// commands get. The local daemon's API socket is named so the script
/// reaches the daemon this window shows.
pub(crate) fn plan(
    launcher: &Launcher,
    choice: &Choice,
    workspace: Option<&str>,
    socket: Option<&str>,
) -> LaunchPlan {
    let mut env = vec![
        ("HERDR_LAUNCH_DEVICE".to_owned(), choice.id.clone()),
        ("HERDR_LAUNCH_DEVICE_LABEL".to_owned(), choice.label.clone()),
    ];
    if let Some(host) = &choice.host {
        env.push(("HERDR_LAUNCH_HOST".to_owned(), host.clone()));
    }
    if let Some(workspace) = workspace {
        env.push(("HERDR_ACTIVE_WORKSPACE_ID".to_owned(), workspace.to_owned()));
    }
    if let Some(socket) = socket {
        env.push(("HERDR_SOCKET_PATH".to_owned(), socket.to_owned()));
    }
    LaunchPlan {
        command: launcher.command.clone(),
        env,
    }
}

/// Runs the plan through `sh`, detached from the app so quitting never takes
/// a launched agent down, and with the user's tool directories on `PATH` as
/// a GUI launch lacks them.
#[cfg(not(test))]
fn spawn(plan: &LaunchPlan) -> std::io::Result<()> {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(&plan.command);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(&plan.command);
        command
    };
    command
        .env("PATH", crate::local_path::local_path())
        .envs(plan.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(home) = crate::config::home() {
        command.current_dir(home);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    // Reaped off the UI thread, so the shell never lingers as a zombie.
    std::thread::Builder::new()
        .name("herdr-launcher-wait".into())
        .spawn(move || {
            let _ = child.wait();
        })?;
    Ok(())
}

impl HerdrWindow {
    /// The `[[launchers]]` entry `keystroke` runs, if any.
    pub(crate) fn launcher_for(&self, keystroke: &Keystroke) -> Option<usize> {
        self.config
            .launchers
            .iter()
            .position(|launcher| crate::keymap::typed_matches(keystroke, &launcher.key))
    }

    /// The devices a launcher can run for: this computer first, then every
    /// enabled saved device, in sidebar order.
    pub(crate) fn launcher_choices(&self) -> Vec<Choice> {
        self.endpoints
            .iter()
            .filter(|endpoint| endpoint.enabled)
            .map(|endpoint| Choice {
                id: endpoint.id.clone(),
                label: endpoint.label.clone(),
                host: match &endpoint.connection.target {
                    ConnectTarget::Ssh { target, .. } => Some(target.clone()),
                    _ => None,
                },
            })
            .collect()
    }

    /// Runs launcher `index`: at once for this computer when the entry does
    /// not ask, or when there is no saved device to choose, else after the
    /// prompt.
    pub(crate) fn open_launcher(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(launcher) = self.config.launchers.get(index) else {
            return;
        };
        let choices = self.launcher_choices();
        if !launcher.ask_device || choices.len() < 2 {
            let local = choices
                .iter()
                .find(|choice| choice.id == crate::endpoint::LOCAL)
                .or(choices.first())
                .cloned();
            if let Some(choice) = local {
                self.run_launcher(index, &choice, cx);
            }
            return;
        }
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Launcher);
        self.menu.launcher = Some(Prompt {
            launcher: index,
            selected: 0,
        });
    }

    /// Keys while the prompt is open: up and down move, a digit picks that
    /// row, Enter runs the highlighted device and Escape cancels.
    pub(crate) fn launcher_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let Some(prompt) = self.menu.launcher.clone() else {
            self.dismiss_menu(window, cx);
            return;
        };
        let count = self.launcher_choices().len();
        let modifiers = event.keystroke.modifiers;
        let modified = modifiers.platform || modifiers.control || modifiers.alt;
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "enter" => self.confirm_launcher(prompt.selected, window, cx),
            "up" | "down" if count > 0 => {
                let selected = if event.keystroke.key == "up" {
                    (prompt.selected + count - 1) % count
                } else {
                    (prompt.selected + 1) % count
                };
                self.menu.launcher = Some(Prompt { selected, ..prompt });
                cx.notify();
            }
            digit if !modified => {
                if let Some(row) = digit
                    .parse::<usize>()
                    .ok()
                    .filter(|row| (1..=count).contains(row))
                {
                    self.confirm_launcher(row - 1, window, cx);
                }
            }
            _ => {}
        }
    }

    /// Runs the prompt's launcher for the device in row `selected`.
    pub(crate) fn confirm_launcher(
        &mut self,
        selected: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.menu.launcher.take() else {
            return;
        };
        let choice = self.launcher_choices().get(selected).cloned();
        self.dismiss_menu(window, cx);
        if let Some(choice) = choice {
            self.run_launcher(prompt.launcher, &choice, cx);
        }
    }

    fn run_launcher(&mut self, index: usize, choice: &Choice, cx: &mut Context<Self>) {
        let Some(launcher) = self.config.launchers.get(index) else {
            return;
        };
        let endpoint = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.id == choice.id);
        let workspace = endpoint.and_then(|index| {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &self.endpoints[index].live
            };
            live.snapshot.as_ref()?.focused_workspace_id.clone()
        });
        let socket = match (choice.host.is_none(), endpoint) {
            (true, Some(index)) => self.endpoints[index]
                .connection
                .target
                .socket_path()
                .ok()
                .as_deref()
                .and_then(api_socket)
                .and_then(|path| path.to_str().map(str::to_owned)),
            _ => None,
        };
        let plan = plan(launcher, choice, workspace.as_deref(), socket.as_deref());
        #[cfg(test)]
        {
            self.launched.push(plan);
            let _ = cx;
        }
        #[cfg(not(test))]
        if let Err(error) = spawn(&plan) {
            self.local_error = Some(format!("{}: {error}", launcher.label));
            cx.notify();
        }
    }
}
