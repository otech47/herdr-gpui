//! External paths paste locally; one supported image on SSH uses the daemon's
//! image bridge. A drop never submits the terminal command.

use super::HerdrWindow;
use crate::{
    Error, Result,
    connection::ConnectionBridge,
    terminal::{InputTarget, wheel_target},
};
use gpui::{Context, ExternalPaths, Pixels, Point, Window};
use herdr_client::protocol::ClientPaneInputEvent;
use std::path::PathBuf;

const MAX_PATHS: usize = 256;
const MAX_PASTE_BYTES: usize = 64 * 1024;

impl HerdrWindow {
    pub(crate) fn drop_terminal_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI translates the platform Submit position into window mouse state
        // before invoking on_drop; the payload itself carries no coordinates.
        if self.drop_paths(paths.paths(), window.mouse_position(), window, cx) {
            cx.stop_propagation();
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn accept_file_promises(window: &Window) {
        if let Err(error) = promise::accept(window) {
            tracing::warn!(%error, "Dragged screenshots and images will not drop");
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn poll_file_promises(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for drop in promise::take(window.window_handle().window_id()) {
            if let Some(error) = drop.errors.first() {
                self.local_error = Some(format!("Files not pasted: {error}"));
                cx.notify();
            }
            if !drop.paths.is_empty() {
                self.drop_paths(&drop.paths, drop.position, window, cx);
            }
        }
    }

    /// Whether a terminal under `position` took the drop.
    fn drop_paths(
        &mut self,
        paths: &[PathBuf],
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_session_tab(cx).is_some() {
            return false;
        }
        let Some(target) = self.file_drop_target(position) else {
            return false;
        };
        if paths.is_empty() {
            return true;
        }
        let result = quote_paths(paths).and_then(|text| {
            if self.accepts_remote_images()
                && let [path] = paths
                && let Some(source) = super::image_source::from_path(path)
            {
                self.start_remote_image(target.clone(), source, Some(text), cx);
                return Ok(());
            }
            if self.accepts_remote_images() {
                self.start_file_transfer(target.clone(), paths.to_vec(), cx);
                return Ok(());
            }
            let endpoint = &self.endpoints[self.selected_endpoint];
            let handle = endpoint
                .connection
                .handle
                .as_ref()
                .ok_or(Error::NotConnected)?;
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            ConnectionBridge::send_input(
                handle,
                &snapshot.boot_id,
                &target,
                ClientPaneInputEvent::Paste(text),
            )?;
            Ok(())
        });
        match result {
            Ok(()) => window.focus(&self.focus, cx),
            Err(error) => {
                self.local_error = Some(format!("Files not pasted: {error}"));
                cx.notify();
            }
        }
        true
    }

    fn file_drop_target(&self, position: Point<Pixels>) -> Option<InputTarget> {
        if self.menu.page.is_some()
            || !self.live.status.is_connected()
            || !self.input_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        wheel_target(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
        .map(|hit| hit.target)
    }
}

/// POSIX shell words, separated by spaces without a trailing newline. Validate
/// the whole drop before allocating its output so failures never paste a prefix.
pub(super) fn quote_paths(paths: &[PathBuf]) -> Result<String> {
    if paths.len() > MAX_PATHS {
        return Err(Error::FileDropSize);
    }
    let mut bytes = paths.len().saturating_sub(1);
    for path in paths {
        // Bound UTF-8 validation and quote counting as well as output allocation.
        if path.as_os_str().len() > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
        let text = path.to_str().ok_or(Error::FileDropEncoding)?;
        if text.is_empty() {
            return Err(Error::FileDropEmptyPath);
        }
        if text.chars().any(char::is_control) {
            return Err(Error::FileDropControl);
        }
        bytes += text.len() + 2 + text.bytes().filter(|byte| *byte == b'\'').count() * 3;
        if bytes > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
    }
    let mut quoted = String::with_capacity(bytes);
    for path in paths {
        if !quoted.is_empty() {
            quoted.push(' ');
        }
        quoted.push('\'');
        for ch in path.to_str().ok_or(Error::FileDropEncoding)?.chars() {
            if ch == '\'' {
                quoted.push_str("'\\''");
            } else {
                quoted.push(ch);
            }
        }
        quoted.push('\'');
    }
    Ok(quoted)
}

#[cfg(target_os = "macos")]
mod promise;
#[cfg(test)]
mod tests;
