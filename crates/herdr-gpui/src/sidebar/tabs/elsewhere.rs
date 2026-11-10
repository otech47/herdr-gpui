//! Tabs waiting on an answer in a space this window does not list, on any
//! connected device. Every window lists them above its own tabs while they
//! wait, and a press brings forward the window that shows their space.

use super::{ListedTab, tab_lines, tab_title, views::workspace_label_by_id};
use crate::{
    HerdrWindow,
    sidebar::{
        agents::Mark,
        cell::{AgentRow, Cell, RowContext, RowData, RowLayout},
        label_text,
    },
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

impl HerdrWindow {
    /// Blocked tabs the list does not hold, the latest to start waiting first.
    pub(crate) fn waiting_elsewhere<'a>(&'a self, listed: &[ListedTab<'a>]) -> Vec<ListedTab<'a>> {
        herdr_board::waiting_elsewhere(&self.sources(), listed, &self.attention)
    }

    /// The rows carry no number: cmd-1 to cmd-9 count only the list's own rows.
    pub(super) fn waiting_rows(
        &self,
        rows: &'static dyn RowLayout,
        row_cx: &RowContext<'_>,
        content_x: f32,
        listed: &[ListedTab<'_>],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let waiting = self.waiting_elsewhere(listed);
        if waiting.is_empty() {
            return Vec::new();
        }
        let gap = row_cx.look.density.gap();
        let mut elements = Vec::with_capacity(waiting.len() + 2);
        elements.push(
            div()
                .debug_selector(|| "tabs-waiting".into())
                .flex_none()
                .px(px(content_x))
                .pt(px(gap))
                .text_color(rgb(row_cx.theme.muted))
                .child(label_text("waiting elsewhere"))
                .into_any_element(),
        );
        for row in &waiting {
            let endpoint = &self.endpoints[row.endpoint];
            let name = tab_title(row.tab, row.snapshot);
            let place = herdr_board::waiting_place(
                workspace_label_by_id(row.snapshot, &row.tab.workspace_id).unwrap_or(""),
                (row.endpoint != 0).then_some(endpoint.label.as_str()),
            );
            let agent = row
                .snapshot
                .agents
                .iter()
                .find(|agent| agent.tab_id == row.tab.tab_id);
            let key = format!("waiting-{}-{}", endpoint.id, row.tab.tab_id);
            let device = endpoint.id.clone();
            let workspace = row.tab.workspace_id.clone();
            let tab = row.tab.tab_id.clone();
            elements.push(
                Cell::new(
                    rows,
                    RowData::Agent(AgentRow {
                        key: key.clone(),
                        name: name.as_str(),
                        icon: match agent {
                            Some(agent) => {
                                crate::icons::AgentIcon::from_identity(agent.agent.as_deref())
                            }
                            None => crate::icons::AgentIcon::Generic,
                        },
                        status: Mark::Status(AgentStatus::Blocked),
                        place: None,
                        status_text: None,
                        lines: tab_lines(&name, "", Some(&place)),
                        icon_line: Some(0),
                    }),
                    row_cx,
                )
                .row()
                .id(SharedString::from(key))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.show_waiting(device.clone(), workspace.clone(), tab.clone(), cx);
                }))
                .into_any_element(),
            );
        }
        elements.push(div().flex_none().h(px(gap)).into_any_element());
        elements
    }

    /// Deferred, since finding the space's window updates every window,
    /// this one included.
    #[cfg(unix)]
    fn show_waiting(
        &mut self,
        device: String,
        workspace: String,
        tab: String,
        cx: &mut Context<Self>,
    ) {
        cx.defer(move |cx| {
            let target = crate::control::Target {
                daemon: None,
                workspace: Some(&workspace),
                pane: None,
                device: Some(&device),
            };
            crate::control::show_workspace(&target, Some(&tab), cx);
        });
    }

    #[cfg(not(unix))]
    fn show_waiting(
        &mut self,
        device: String,
        _workspace: String,
        tab: String,
        cx: &mut Context<Self>,
    ) {
        self.navigate_endpoint(&device, crate::NavigationTarget::Tab(&tab), cx);
    }
}
