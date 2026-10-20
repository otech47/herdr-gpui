//! The window chrome an endpoint remembers between runs: sidebar width and
//! split, folded sections, the tabs view and sort, the agents sort, and panel
//! widths,
//! read and written off the UI thread.
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How the agents panel orders its rows, as in the terminal client: grouped
/// keeps the daemon's workspace order, priority floats the agents that want
/// attention. Herdr's `ui.agent_panel_sort` seeds it until the user toggles
/// it, which deserializes upstream's spellings ("spaces", alias "workspaces").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub enum AgentSort {
    #[default]
    #[serde(rename = "spaces", alias = "workspaces")]
    Grouped,
    #[serde(rename = "priority")]
    Priority,
}

impl std::fmt::Display for AgentSort {
    /// Also the stored spelling, which `AgentSort::parse` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grouped => "grouped",
            Self::Priority => "priority",
        })
    }
}

impl AgentSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Priority,
            Self::Priority => Self::Grouped,
        }
    }

    /// An unreadable or unknown stored value is no override at all, so the
    /// daemon's setting applies rather than a guessed sort.
    fn parse(value: Option<&serde_json::Value>) -> Option<Self> {
        match value.and_then(serde_json::Value::as_str)? {
            "grouped" => Some(Self::Grouped),
            "priority" => Some(Self::Priority),
            _ => None,
        }
    }
}

/// A sidebar section the user can fold to its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Spaces,
    Tabs,
    Agents,
}

impl Section {
    pub const ALL: [Self; 3] = [Self::Spaces, Self::Tabs, Self::Agents];

    /// The header label, and the stored spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Spaces => "spaces",
            Self::Tabs => "tabs",
            Self::Agents => "agents",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.name() == name)
    }
}

/// Which sidebar sections are folded to their headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Collapsed {
    spaces: bool,
    tabs: bool,
    agents: bool,
}

impl Collapsed {
    pub fn is(self, section: Section) -> bool {
        match section {
            Section::Spaces => self.spaces,
            Section::Tabs => self.tabs,
            Section::Agents => self.agents,
        }
    }

    pub fn toggle(&mut self, section: Section) {
        let flag = match section {
            Section::Spaces => &mut self.spaces,
            Section::Tabs => &mut self.tabs,
            Section::Agents => &mut self.agents,
        };
        *flag = !*flag;
    }

    fn names(self) -> Vec<&'static str> {
        Section::ALL
            .into_iter()
            .filter(|section| self.is(*section))
            .map(Section::name)
            .collect()
    }

    /// Unknown names are ignored, so a newer build's sections do not undo
    /// the ones this build knows; anything but an array folds nothing.
    fn parse(value: Option<&serde_json::Value>) -> Self {
        let mut collapsed = Self::default();
        for name in value
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            if let Some(section) = Section::parse(name)
                && !collapsed.is(section)
            {
                collapsed.toggle(section);
            }
        }
        collapsed
    }
}

/// Which tabs the sidebar's tabs list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsView {
    /// The shown space's tabs, on the selected device.
    #[default]
    Device,
    /// Every device's tabs of a space with the shown one's name, in one list.
    All,
    /// The same tabs, each device's under a line naming it.
    ByDevice,
}

impl TabsView {
    /// The header's label, and the stored spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::All => "all",
            Self::ByDevice => "by device",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Device => Self::All,
            Self::All => Self::ByDevice,
            Self::ByDevice => Self::Device,
        }
    }

    fn parse(value: Option<&serde_json::Value>) -> Option<Self> {
        match value.and_then(serde_json::Value::as_str)? {
            "device" => Some(Self::Device),
            "all" => Some(Self::All),
            "by device" => Some(Self::ByDevice),
            _ => None,
        }
    }
}

/// How the tabs list orders its rows: as the daemon keeps them, or the tabs
/// that want attention first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsSort {
    /// The daemon's order, which dragging a row changes.
    #[default]
    Manual,
    /// Blocked tabs first, then a used-up plan, working, done, idle, unknown,
    /// each group in the daemon's order.
    Attention,
}

impl TabsSort {
    /// The header's label, and the stored spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Attention => "attention",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Manual => Self::Attention,
            Self::Attention => Self::Manual,
        }
    }

    fn parse(value: Option<&serde_json::Value>) -> Option<Self> {
        match value.and_then(serde_json::Value::as_str)? {
            "manual" => Some(Self::Manual),
            "attention" => Some(Self::Attention),
            _ => None,
        }
    }
}

/// The window chrome this endpoint remembers between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    pub sidebar_width: Option<f32>,
    pub sidebar_split: Option<f32>,
    /// The sort the user picked with the panel toggle, like upstream's
    /// `agent_panel_sort` preference. `None` follows the daemon's config.
    pub agent_sort: Option<AgentSort>,
    /// The notes panel's width, once dragged; `None` is its default.
    pub notes_width: Option<f32>,
    /// The review's file list's width, once dragged.
    pub review_files_width: Option<f32>,
    /// The sidebar sections folded to their headers.
    pub collapsed: Collapsed,
    /// The spaces list's height once dragged, with a tabs list below it;
    /// `None` fits the list to its rows.
    pub spaces_height: Option<f32>,
    /// Which tabs the tabs list shows.
    pub tabs_view: TabsView,
    /// How the tabs list orders them.
    pub tabs_sort: TabsSort,
}

/// Asynchronous, endpoint-local preferences. Dropping lets the worker drain queued
/// saves without waiting; process exit may interrupt pending writes.
pub struct Preferences {
    saves: Option<Sender<Chrome>>,
    loaded: Option<Receiver<Chrome>>,
    worker: Option<JoinHandle<()>>,
}

impl Preferences {
    pub fn new(socket: &Path) -> Self {
        Self::start(
            state_dir()
                .map(|dir| endpoint_path(&dir, socket))
                .ok_or(crate::Error::MissingStateRoot),
        )
    }

    fn start(path: crate::Result<PathBuf>) -> Self {
        let (saves, requests) = mpsc::channel();
        let (loaded_tx, loaded) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("gpui-preferences".into())
            .spawn(move || {
                let path = match path {
                    Ok(path) => path,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_location",
                            "Cannot locate GPUI preferences"
                        );
                        let _ = loaded_tx.send(Chrome::default());
                        return;
                    }
                };
                let chrome = match read_chrome(&path) {
                    Ok(chrome) => chrome,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_read",
                            "Cannot read GPUI preferences"
                        );
                        Chrome::default()
                    }
                };
                let _ = loaded_tx.send(chrome);
                for chrome in requests {
                    if write_chrome(&path, chrome).is_err() {
                        tracing::warn!(
                            category = "preferences_write",
                            "Cannot save GPUI preferences"
                        );
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(category = "preferences_worker_start", error_kind = ?error.kind(), "Cannot start GPUI preferences worker");
                None
            }
        };
        Self {
            saves: Some(saves),
            loaded: Some(loaded),
            worker,
        }
    }

    /// Takes the initial result once; `None` means pending or already taken.
    /// Defaults stand in for a failed initial load.
    pub fn loaded(&mut self) -> Option<Chrome> {
        let result = match self.loaded.as_ref()?.try_recv() {
            Ok(chrome) => Some(chrome),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Some(Chrome::default()),
        };
        self.loaded = None;
        result
    }

    /// Queues the whole chrome, so saving one field never drops the others.
    pub fn save(&self, chrome: Chrome) {
        if let Some(saves) = &self.saves
            && saves.send(chrome).is_err()
        {
            tracing::warn!(
                category = "preferences_worker_disconnected",
                "Cannot queue GPUI preferences save"
            );
        }
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        // Disconnect and detach: the worker drains queued saves while the process
        // remains alive, without making the UI wait for disk I/O.
        self.saves.take();
        drop(self.worker.take());
    }
}

/// The GPUI client's own state directory, shared by preferences and logs.
pub(crate) fn state_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })
        .map(|root| root.join("herdr/gpui"))
}

fn endpoint_path(dir: &Path, socket: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in socket.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    dir.join(format!("local-{hash:016x}.json"))
}

fn read_chrome(path: &Path) -> crate::Result<Chrome> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Chrome::default()),
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object()
        .ok_or(crate::Error::PreferencesNotObject)?;
    // Earlier builds always wrote `agent_sort`, so its default value cannot be
    // told from a choice; only "priority" proves the user toggled it.
    let agent_sort = AgentSort::parse(object.get("agent_sort_manual")).or_else(|| {
        AgentSort::parse(object.get("agent_sort")).filter(|sort| *sort == AgentSort::Priority)
    });
    let sidebar_width = match object.get("sidebar_width_px") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let width = value.as_f64().map(|width| width as f32);
            match width {
                Some(width) if width.is_finite() && width > 0.0 => Some(width),
                _ => return Err(crate::Error::InvalidStoredWidth),
            }
        }
    };
    let sidebar_split = object
        .get("sidebar_split")
        .and_then(serde_json::Value::as_f64)
        .map(|split| split as f32)
        .filter(|split| split.is_finite() && (0.1..=0.9).contains(split));
    // A damaged panel width is forgotten rather than failing the whole file.
    let notes_width = object
        .get("notes_width_px")
        .and_then(serde_json::Value::as_f64)
        .map(|width| width as f32)
        .filter(|width| width.is_finite() && *width > 0.0);
    let review_files_width = object
        .get("review_files_width_px")
        .and_then(serde_json::Value::as_f64)
        .map(|width| width as f32)
        .filter(|width| width.is_finite() && *width > 0.0);
    let spaces_height = object
        .get("sidebar_spaces_height_px")
        .and_then(serde_json::Value::as_f64)
        .map(|height| height as f32)
        .filter(|height| height.is_finite() && *height > 0.0);
    Ok(Chrome {
        sidebar_width,
        sidebar_split,
        agent_sort,
        notes_width,
        review_files_width,
        collapsed: Collapsed::parse(object.get("sidebar_collapsed")),
        spaces_height,
        tabs_view: TabsView::parse(object.get("sidebar_tabs_view")).unwrap_or_default(),
        tabs_sort: TabsSort::parse(object.get("sidebar_tabs_sort")).unwrap_or_default(),
    })
}

fn write_chrome(path: &Path, chrome: Chrome) -> crate::Result<()> {
    let width = chrome.sidebar_width;
    if width.is_some_and(|width| !width.is_finite() || width <= 0.0) {
        return Err(crate::Error::InvalidSidebarWidth);
    }
    let parent = path.parent().ok_or(crate::Error::PreferencesPath)?;
    fs::create_dir_all(parent)?;
    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".preferences-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| -> crate::Result<()> {
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "sidebar_width_px": width,
                "sidebar_split": chrome.sidebar_split.filter(|split| {
                    split.is_finite() && (0.1..=0.9).contains(split)
                }),
                "agent_sort_manual": chrome.agent_sort.map(|sort| sort.to_string()),
                "notes_width_px": chrome.notes_width.filter(|width| width.is_finite() && *width > 0.0),
                "review_files_width_px": chrome
                    .review_files_width
                    .filter(|width| width.is_finite() && *width > 0.0),
                "sidebar_collapsed": chrome.collapsed.names(),
                "sidebar_spaces_height_px": chrome
                    .spaces_height
                    .filter(|height| height.is_finite() && *height > 0.0),
                "sidebar_tabs_view": chrome.tabs_view.name(),
                "sidebar_tabs_sort": chrome.tabs_sort.name(),
            }),
        )?;
        file.write_all(b"\n")?;
        Ok(file.sync_all()?)
    })();
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path).map_err(crate::Error::from));
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
    {
        tracing::warn!(category = "preferences_cleanup", error_kind = ?error.kind(), "Cannot clean up GPUI preferences");
    }
    result
}

#[cfg(test)]
mod tests;
