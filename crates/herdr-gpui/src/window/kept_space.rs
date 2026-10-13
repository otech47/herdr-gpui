//! The daemon moves every attached window when a CLI focuses a tab, or
//! creates one with focus. A window that did not ask for the move asks for
//! its own space back.

use super::HerdrWindow;
use crate::NavigationTarget;
use std::time::{Duration, Instant};

/// A space change this long after the window's own focus request is that
/// request's answer, whatever the request was for.
const ASKED_GRACE: Duration = Duration::from_secs(5);
/// How long a window waits for its space to come back before it follows
/// the daemon.
const RESTORE_WAIT: Duration = Duration::from_secs(5);

/// The daemon and space a window shows, and the requests that explain a
/// snapshot showing another one.
#[derive(Debug, Default)]
pub(crate) struct KeptSpace {
    shown: Option<(String, String)>,
    asked: Option<Instant>,
    restoring: Option<Instant>,
}

/// What a window does with a snapshot that focuses another space.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Keep {
    Accept,
    /// A restore is in flight and this snapshot predates its answer.
    Wait,
    Restore(String),
}

impl KeptSpace {
    pub(crate) fn asked(&mut self) {
        self.asked = Some(Instant::now());
    }

    pub(crate) fn decide(
        &self,
        (boot, focused): (&str, &str),
        exists: impl FnOnce(&str) -> bool,
        now: Instant,
    ) -> Keep {
        let Some((kept_boot, kept)) = &self.shown else {
            return Keep::Accept;
        };
        if kept_boot != boot || kept == focused || !exists(kept) {
            return Keep::Accept;
        }
        let within = |since: Option<Instant>, limit| {
            since.is_some_and(|since| now.duration_since(since) <= limit)
        };
        if within(self.asked, ASKED_GRACE) {
            Keep::Accept
        } else if within(self.restoring, RESTORE_WAIT) {
            Keep::Wait
        } else {
            Keep::Restore(kept.clone())
        }
    }

    pub(crate) fn accept(&mut self, boot: &str, focused: &str) {
        self.restoring = None;
        if self
            .shown
            .as_ref()
            .is_none_or(|(kept_boot, kept)| kept_boot != boot || kept != focused)
        {
            self.shown = Some((boot.to_owned(), focused.to_owned()));
            self.asked = None;
        }
    }

    fn restoring(&mut self) {
        self.restoring = Some(Instant::now());
        // Cleared so the restore itself does not excuse the next foreign space.
        self.asked = None;
    }
}

impl HerdrWindow {
    /// Follows the space the snapshot just taken focuses when the window asked
    /// for it or its own space is gone, and otherwise asks for its space back.
    pub(crate) fn keep_space(&mut self) {
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let Some(focused) = snapshot.focused_workspace_id.clone() else {
            return;
        };
        let boot = snapshot.boot_id.clone();
        let exists = |kept: &str| {
            snapshot
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == kept)
        };
        match self
            .kept_space
            .decide((&boot, &focused), exists, Instant::now())
        {
            Keep::Wait => {}
            Keep::Restore(kept)
                if self.request_focus_change(
                    "Keep space",
                    Some(NavigationTarget::Workspace(kept.clone())),
                    |handle, boot| handle.focus_workspace(boot, &kept),
                ) =>
            {
                self.kept_space.restoring();
            }
            Keep::Accept | Keep::Restore(_) => self.kept_space.accept(&boot, &focused),
        }
    }
}
