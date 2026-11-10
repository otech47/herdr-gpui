//! Links that leave a page's site. The page reports a click on one, or a
//! choice in the bubble over one, and the window opens or copies the address.
use crate::browser::{MAX_URL_BYTES, WebUrl};
use serde::Deserialize;

pub(super) const SCRIPT: &str = include_str!("link.js");

/// One address and a few words around it.
const MAX_MESSAGE_BYTES: usize = MAX_URL_BYTES + 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Open,
    Copy,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Message {
    Link { action: Action, url: WebUrl },
}

/// Any script in the page can post a report, which at most opens or copies
/// one web address.
pub(super) fn parse(body: &str) -> Option<(Action, WebUrl)> {
    if body.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let Message::Link { action, url } = serde_json::from_str(body).ok()?;
    Some((action, url))
}

#[cfg(test)]
mod tests;
