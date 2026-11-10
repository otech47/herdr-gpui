//! Where each tab sits under the attention sort. The focused tab keeps its
//! row until focus leaves it, so the list never moves under the row the user
//! is on.

use crate::HerdrWindow;

pub(crate) use herdr_board::{AttentionLedger, attention_rank};

impl HerdrWindow {
    /// Settle every tab's place for this frame. The focused tab on the
    /// selected device is held.
    pub(crate) fn settle_attention(&mut self) {
        let mut ledger = std::mem::take(&mut self.attention);
        let focused = self
            .live
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_tab_id.as_deref());
        herdr_board::settle(
            &self.sources(),
            &mut ledger,
            |index, agent, status| self.mark(index, agent, status),
            |index, tab| index == self.selected_endpoint && focused == Some(tab.tab_id.as_str()),
        );
        self.attention = ledger;
    }
}
