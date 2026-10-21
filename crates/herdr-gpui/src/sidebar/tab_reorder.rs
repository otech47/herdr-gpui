//! Reordering a space's tabs by dragging their sidebar rows, with the
//! gestures the spaces list has: holding a press on a row, or moving it a few
//! pixels, lifts the row; the rows it passes shift as if it had already been
//! dropped under the pointer. Releasing sends `tab.move` to the row's device.
//! The daemon owns the order, so the preview holds until the next snapshot
//! reorders the list. In the views across devices a row stays among its own
//! device's rows: a space's order belongs to one daemon.
//!
//! Under the attention sort a row stays among the rows of its rank, so a
//! drag never undoes the sort. The drop places the rank in the ledger, which
//! owns that order, and asks the daemon to put the tab beside its nearest
//! neighbor of its own device, so the daemon's order follows as far as it
//! can.

use crate::{
    HerdrWindow,
    preferences::{TabsSort, TabsView},
    reorder::{Beside, Drag, LIFT_DELAY, SETTLE_TIMEOUT, resting_spans, slot_for},
    sidebar::tabs::ListedTab,
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
    /// The daemon's gap, None when its order has nothing to change.
    pub(in crate::sidebar) insert: Option<usize>,
    pub(in crate::sidebar) hold: Hold,
}

/// How the list keeps the dropped order.
pub(in crate::sidebar) enum Hold {
    /// The device's rows as listed, to hold the preview against until the
    /// daemon's order differs.
    Daemon(Vec<String>),
    /// A rank's rows in their new order, by device id and tab id, placed in
    /// the ledger at once.
    Ledger(u8, Vec<(String, String)>),
}

/// A listed row's name in a drag: its device and tab together, as two
/// devices may number their tabs alike.
pub(super) fn row_key(endpoint_id: &str, tab_id: &str) -> String {
    format!("{endpoint_id}/{tab_id}")
}

/// The positions of the rows grouped with the one at `position`: a
/// contiguous run of equal groups.
fn run<G: PartialEq>(groups: &[G], position: usize) -> Range<usize> {
    let group = &groups[position];
    let start = groups[..position]
        .iter()
        .rposition(|candidate| candidate != group)
        .map_or(0, |before| before + 1);
    let end = groups[position..]
        .iter()
        .position(|candidate| candidate != group)
        .map_or(groups.len(), |after| position + after);
    start..end
}

/// A tab's index among its space's tabs in the daemon's order.
fn daemon_index(row: &ListedTab<'_>) -> Option<usize> {
    row.snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == row.tab.workspace_id)
        .position(|tab| tab.tab_id == row.tab.tab_id)
}

impl HerdrWindow {
    /// The positions of the rows the row at `position` may land among. In
    /// the daemon's order those are its device's rows, as the list goes
    /// device by device. Under the attention sort they are the rows of its
    /// rank, of its device alone where the view groups by device.
    pub(super) fn drag_rows(&self, listed: &[ListedTab<'_>], position: usize) -> Range<usize> {
        if self.sidebar_tabs_sort == TabsSort::Attention {
            let grouped = self.sidebar_tabs_view == TabsView::ByDevice;
            let groups: Vec<(Option<usize>, u8)> = listed
                .iter()
                .map(|row| (grouped.then_some(row.endpoint), self.listed_rank(row)))
                .collect();
            run(&groups, position)
        } else {
            let endpoints: Vec<usize> = listed.iter().map(|row| row.endpoint).collect();
            run(&endpoints, position)
        }
    }

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
        let listed = self.listed_tabs();
        let Some(pressed) = listed
            .iter()
            .position(|row| row.endpoint == endpoint && row.tab.tab_id == tab)
        else {
            return;
        };
        if self.drag_rows(&listed, pressed).len() < 2 {
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
        if let Hold::Ledger(rank, tabs) = &moved.hold {
            self.attention.reorder(*rank, tabs.iter().cloned());
        }
        let Some(insert) = moved.insert else {
            return true;
        };
        let Some(handle) = &self.endpoints[moved.endpoint].connection.handle else {
            return true;
        };
        let params = serde_json::json!({ "tab_id": moved.tab, "insert_index": insert });
        match handle.request(&drag.boot, Method::TabMove, params) {
            Ok(_) => {
                if let Hold::Daemon(order) = moved.hold {
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
        let rows = self.drag_rows(&listed, dragged);
        if self.sidebar_tabs_sort != TabsSort::Attention {
            return Some(TabMove {
                endpoint: row.endpoint,
                tab: row.tab.tab_id.clone(),
                insert: Some(slot.saturating_sub(rows.start)),
                hold: Hold::Daemon(keys[rows].to_vec()),
            });
        }
        // The rank in its new order: the carried row at its gap, counted
        // without the row itself.
        let mut order: Vec<usize> = rows
            .clone()
            .filter(|&position| position != dragged)
            .collect();
        let at = slot
            .saturating_sub(rows.start)
            .saturating_sub(usize::from(slot > dragged))
            .min(order.len());
        order.insert(at, dragged);
        let own = |position: &&usize| listed[**position].endpoint == row.endpoint;
        let neighbor = order[..at]
            .iter()
            .rev()
            .find(own)
            .map(|&position| Beside::After(position))
            .or_else(|| {
                order[at + 1..]
                    .iter()
                    .find(own)
                    .map(|&position| Beside::Before(position))
            });
        let insert = neighbor
            .and_then(|beside| beside.and_then(|position| daemon_index(&listed[position])))
            .map(|beside| match beside {
                Beside::Before(index) => index,
                Beside::After(index) => index + 1,
            });
        let settled = order
            .iter()
            .map(|&position| {
                let row = &listed[position];
                (
                    self.endpoints[row.endpoint].id.clone(),
                    row.tab.tab_id.clone(),
                )
            })
            .collect();
        Some(TabMove {
            endpoint: row.endpoint,
            tab: row.tab.tab_id.clone(),
            insert,
            hold: Hold::Ledger(self.listed_rank(row), settled),
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
/// listed row's child index and painted shift, kept among the rows it may
/// land among. `None` over the row's own place, which moves nothing.
pub(super) fn resolve(
    rows: &[(usize, usize, Pixels)],
    dragged: usize,
    among: &Range<usize>,
    scroll: &ScrollHandle,
    lift: Pixels,
) -> Option<usize> {
    let spans = resting_spans(rows, rows.len(), scroll);
    let (top, bottom) = (*spans.get(dragged)?)?;
    let lift = f32::from(lift);
    let slot = slot_for(&spans, dragged, (top + lift, bottom + lift)).clamp(among.start, among.end);
    (slot != dragged && slot != dragged + 1).then_some(slot)
}
