//! Reordering a space's tabs by dragging their sidebar rows, with the
//! gestures the spaces list has: holding a press on a row, or moving it a few
//! pixels, lifts the row; the rows it passes shift as if it had already been
//! dropped under the pointer. Releasing sends `tab.move` to the row's device.
//! The daemon owns the order, so the preview holds until the next snapshot
//! reorders the list. In the views across devices a row stays among its own
//! device's rows: a space's order belongs to one daemon.

use crate::{
    HerdrWindow,
    reorder::{Drag, LIFT_DELAY, SETTLE_TIMEOUT, resting_spans, slot_for},
};
use gpui::{Context, Pixels, Point, ScrollHandle};
use herdr_client::Method;
use std::ops::Range;

/// A press on a tab row that may become a reorder. Its gap is the position
/// among the listed rows the row lands before; less its device's first row,
/// that is the daemon's insert index.
pub(crate) type TabRowDrag = Drag<usize>;

/// What a drop asks of the carried row's device.
pub(in crate::sidebar) struct TabMove {
    pub(in crate::sidebar) endpoint: usize,
    pub(in crate::sidebar) tab: String,
    pub(in crate::sidebar) insert: usize,
    /// The device's rows as listed, to hold the preview against.
    pub(in crate::sidebar) order: Vec<String>,
}

/// A listed row's name in a drag: its device and tab together, as two
/// devices may number their tabs alike.
pub(super) fn row_key(endpoint_id: &str, tab_id: &str) -> String {
    format!("{endpoint_id}/{tab_id}")
}

/// The positions of the rows on the device of the row at `position`, given
/// every row's device: a contiguous run, as the list goes device by device.
pub(super) fn device_rows(endpoints: &[usize], position: usize) -> Range<usize> {
    let device = endpoints[position];
    let start = endpoints[..position]
        .iter()
        .rposition(|endpoint| *endpoint != device)
        .map_or(0, |before| before + 1);
    let end = endpoints[position..]
        .iter()
        .position(|endpoint| *endpoint != device)
        .map_or(endpoints.len(), |after| position + after);
    start..end
}

impl HerdrWindow {
    /// Arms a reorder for a left press on the row of `tab` on `endpoint`.
    pub(super) fn press_tab_row(
        &mut self,
        endpoint: usize,
        tab: &str,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            return;
        }
        let live = self.live_of(endpoint);
        let Some(snapshot) = &live.snapshot else {
            return;
        };
        if !live.status.is_connected() || !live.supports_tab_move {
            return;
        }
        let rows = self
            .listed_tabs()
            .iter()
            .filter(|row| row.endpoint == endpoint)
            .count();
        if rows < 2 {
            return;
        }
        let boot = snapshot.boot_id.clone();
        let key = row_key(&self.endpoints[endpoint].id, tab);
        let lift = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIFT_DELAY).await;
            let _ = this.update(cx, |this, cx| this.lift_tab_row(cx));
        });
        self.tab_row_drag = Some(Drag::new(boot, key, position, lift));
    }

    fn lift_tab_row(&mut self, cx: &mut Context<Self>) {
        if self.tab_row_drag.as_mut().is_some_and(|drag| drag.lift()) {
            cx.notify();
        }
    }

    /// Follows the pointer. `target` resolves a lifted drag's gap from how far
    /// the row has moved, against the geometry the list last laid out.
    /// Returns whether the move belongs to the drag.
    pub(super) fn move_tab_row_drag(
        &mut self,
        position: Point<Pixels>,
        held: bool,
        target: impl FnOnce(Pixels) -> Option<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.tab_row_drag.as_mut().filter(|drag| !drag.is_dropped()) else {
            return false;
        };
        // A release outside the window never reaches the sidebar.
        if !held {
            let lifted = drag.lifted;
            self.tab_row_drag = None;
            cx.notify();
            return lifted;
        }
        if drag.follow(position) {
            self.lift_tab_row(cx);
        }
        let Some(drag) = self.tab_row_drag.as_mut().filter(|drag| drag.lifted) else {
            return false;
        };
        drag.target = target(drag.offset());
        cx.notify();
        true
    }

    /// Ends the drag on release, sending its move if it was lifted over a gap.
    /// Returns whether it had lifted, in which case the release is not a click.
    pub(super) fn release_tab_row_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mut drag) = self.tab_row_drag.take() else {
            return false;
        };
        if !drag.floating() {
            return false;
        }
        cx.notify();
        let Some(slot) = drag.target else {
            return true;
        };
        let Some(moved) = self.tab_move(&drag.key, &drag.boot, slot) else {
            return true;
        };
        let Some(handle) = &self.endpoints[moved.endpoint].connection.handle else {
            return true;
        };
        let params = serde_json::json!({ "tab_id": moved.tab, "insert_index": moved.insert });
        match handle.request(&drag.boot, Method::TabMove, params) {
            Ok(_) => {
                let settle = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SETTLE_TIMEOUT).await;
                    let _ = this.update(cx, |this, cx| {
                        this.tab_row_drag = None;
                        cx.notify();
                    });
                });
                drag.drop(moved.order, settle);
                self.tab_row_drag = Some(drag);
            }
            Err(error) => self.local_error = Some(format!("Tab move not sent: {error}")),
        }
        true
    }

    /// The move dropping the row `key` at `slot` asks for, if the row is
    /// still listed from the daemon boot the drag began on.
    pub(in crate::sidebar) fn tab_move(
        &self,
        key: &str,
        boot: &str,
        slot: usize,
    ) -> Option<TabMove> {
        let listed = self.listed_tabs();
        let keys: Vec<String> = listed
            .iter()
            .map(|row| row_key(&self.endpoints[row.endpoint].id, &row.tab.tab_id))
            .collect();
        let dragged = keys.iter().position(|candidate| candidate == key)?;
        let row = &listed[dragged];
        if row.snapshot.boot_id != boot {
            return None;
        }
        let endpoints: Vec<usize> = listed.iter().map(|row| row.endpoint).collect();
        let rows = device_rows(&endpoints, dragged);
        Some(TabMove {
            endpoint: row.endpoint,
            tab: row.tab.tab_id.clone(),
            insert: slot.saturating_sub(rows.start),
            order: keys[rows].to_vec(),
        })
    }

    /// Drops a lifted row where it came from. Returns whether one was lifted.
    pub(crate) fn cancel_tab_row_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.tab_row_drag.as_ref().is_some_and(TabRowDrag::floating) {
            return false;
        }
        self.tab_row_drag = None;
        cx.notify();
        true
    }
}

/// Resolves the pointer to the gap the carried row would land in, from each
/// listed row's child index and painted shift, kept among the rows of its
/// device. `None` over the row's own place, which moves nothing.
pub(super) fn resolve(
    rows: &[(usize, usize, Pixels)],
    dragged: usize,
    device: &Range<usize>,
    scroll: &ScrollHandle,
    lift: Pixels,
) -> Option<usize> {
    let spans = resting_spans(rows, rows.len(), scroll);
    let (top, bottom) = (*spans.get(dragged)?)?;
    let lift = f32::from(lift);
    let slot =
        slot_for(&spans, dragged, (top + lift, bottom + lift)).clamp(device.start, device.end);
    (slot != dragged && slot != dragged + 1).then_some(slot)
}
