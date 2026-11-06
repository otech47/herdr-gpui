//! The cursor over a page. The window draws its own cursor above its native
//! pages, which hides the one the page would show, so every page reports
//! the CSS cursor under the mouse and the window draws the closest match.
use gpui::CursorStyle;
use serde::Deserialize;

/// Installed in every page's main frame before its own scripts run.
pub(super) const SCRIPT: &str = include_str!("cursor.js");

/// A report is one short keyword; anything longer is some other message.
const MAX_MESSAGE_BYTES: usize = 256;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Message {
    Cursor { cursor: String },
}

/// Any script in the page can post a report, which at most changes the
/// cursor over that page.
pub(super) fn parse(body: &str) -> Option<CursorStyle> {
    if body.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let Message::Cursor { cursor } = serde_json::from_str(body).ok()?;
    Some(style(&cursor))
}

fn style(keyword: &str) -> CursorStyle {
    match keyword {
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "vertical-text" => CursorStyle::IBeamCursorForVerticalLayout,
        "crosshair" | "cell" => CursorStyle::Crosshair,
        "grab" => CursorStyle::OpenHand,
        "grabbing" => CursorStyle::ClosedHand,
        "not-allowed" | "no-drop" => CursorStyle::OperationNotAllowed,
        "alias" => CursorStyle::DragLink,
        "copy" => CursorStyle::DragCopy,
        "context-menu" => CursorStyle::ContextualMenu,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "ew-resize" => CursorStyle::ResizeLeftRight,
        "ns-resize" => CursorStyle::ResizeUpDown,
        "e-resize" => CursorStyle::ResizeRight,
        "w-resize" => CursorStyle::ResizeLeft,
        "n-resize" => CursorStyle::ResizeUp,
        "s-resize" => CursorStyle::ResizeDown,
        "nwse-resize" | "nw-resize" | "se-resize" => CursorStyle::ResizeUpLeftDownRight,
        "nesw-resize" | "ne-resize" | "sw-resize" => CursorStyle::ResizeUpRightDownLeft,
        _ => CursorStyle::Arrow,
    }
}

#[cfg(test)]
mod tests;
