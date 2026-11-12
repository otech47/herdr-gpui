//! `[[links]]`: buttons beside the sidebar toggle that open a page in the
//! system browser.
use crate::{Error, Result};
use serde::Deserialize;

const MAX_LINKS: usize = 8;

/// Each name needs an entry in the asset table in `icons.rs`.
const ICONS: [&str; 8] = [
    "external", "pulse", "chart", "globe", "bell", "github", "sessions", "settings",
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Link {
    /// The hover text.
    pub label: String,
    pub url: String,
    /// An asset path such as `icons/pulse.svg`.
    pub icon: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct LinkSettings {
    label: Option<String>,
    url: String,
    icon: Option<String>,
}

pub(super) fn links(settings: Vec<LinkSettings>) -> Result<Vec<Link>> {
    if settings.len() > MAX_LINKS {
        return Err(Error::TooManyLinks(MAX_LINKS));
    }
    settings
        .into_iter()
        .map(|entry| {
            let url = entry.url.trim();
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(Error::LinkUrl(entry.url));
            }
            let icon = entry.icon.as_deref().map_or("external", str::trim);
            if !ICONS.contains(&icon) {
                return Err(Error::UnknownLinkIcon(icon.to_owned()));
            }
            let label = entry
                .label
                .map(|label| label.trim().to_owned())
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| url.to_owned());
            Ok(Link {
                label,
                url: url.to_owned(),
                icon: format!("icons/{icon}.svg"),
            })
        })
        .collect()
}
