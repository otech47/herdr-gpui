//! Reordering the shown space's tabs by dragging their sidebar rows, with the
//! gestures the spaces list has: holding a press on a row, or moving it a few
//! pixels, lifts the row; the rows it passes shift as if it had already been
//! dropped under the pointer. Releasing sends `tab.move`. The daemon owns the
//! order, so the preview holds until the next snapshot reorders the list.

use super::tabs::shown_tabs;
use crate::{
    HerdrWindow,
    reorder::{Drag, LIFT_DELAY, SETTLE_TIMEOUT, resting_spans, slot_for},
};
use gpui::{Context, Pixels, Point, ScrollHandle};
use herdr_client::Method;

/// A press on a tab row that may become a reorder. Its gap is the position
/// among the listed tabs the row lands before, which is also the daemon's
/// insert index, since the list shows every tab of the space in its order.
pub(crate) type TabRowDrag = Drag<usize>;

impl HerdrWindow {
    /// Arms a reorder for a left press on a tab row.
    pub(super) fn press_tab_row(
        &mut self,
        tab: &str,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        if self.menu.page.is_some()
            || !self.live.status.is_connected()
            || !self.live.supports_tab_move
            || shown_tabs(snapshot).len() < 2
        {
            return;
        }
        let lift = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIFT_DELAY).await;
            let _ = this.update(cx, |this, cx| this.lift_tab_row(cx));
        });
        self.tab_row_drag = Some(Drag::new(
            snapshot.boot_id.clone(),
            tab.to_owned(),
            position,
            lift,
        ));
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
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let (Some(handle), Some(snapshot)) = (&connection.handle, &self.live.snapshot) else {
            return true;
        };
        if snapshot.boot_id != drag.boot {
            return true;
        }
        let params = serde_json::json!({ "tab_id": drag.key, "insert_index": slot });
        match handle.request(&drag.boot, Method::TabMove, params) {
            Ok(_) => {
                let order = shown_tabs(snapshot)
                    .into_iter()
                    .map(|tab| tab.tab_id.clone())
                    .collect();
                let settle = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SETTLE_TIMEOUT).await;
                    let _ = this.update(cx, |this, cx| {
                        this.tab_row_drag = None;
                        cx.notify();
                    });
                });
                drag.drop(order, settle);
                self.tab_row_drag = Some(drag);
            }
            Err(error) => self.local_error = Some(format!("Tab move not sent: {error}")),
        }
        true
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
/// listed row's child index and painted shift. `None` over the row's own
/// place, which moves nothing.
pub(super) fn resolve(
    rows: &[(usize, usize, Pixels)],
    dragged: usize,
    scroll: &ScrollHandle,
    lift: Pixels,
) -> Option<usize> {
    let spans = resting_spans(rows, rows.len(), scroll);
    let (top, bottom) = (*spans.get(dragged)?)?;
    let lift = f32::from(lift);
    let slot = slot_for(&spans, dragged, (top + lift, bottom + lift));
    (slot != dragged && slot != dragged + 1).then_some(slot)
}
