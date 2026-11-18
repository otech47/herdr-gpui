//! The requests popup, opened from the titlebar: each request sent to an agent
//! from outside Herdr, newest first, with how it stands. A request with a tab
//! opens that tab, on whichever device runs it.

use super::Page;
use crate::{HerdrWindow, navigation::NavigationTarget, requests::Request};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn open_requests(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            self.dismiss_menu(window, cx);
            return;
        }
        self.requests.refresh();
        self.menu.reset();
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Requests);
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    /// The saved device whose name matches the request's host, else this
    /// computer.
    fn request_endpoint(&self, request: &Request) -> Option<String> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.label.eq_ignore_ascii_case(&request.host_name))
            .or_else(|| {
                self.endpoints
                    .iter()
                    .find(|endpoint| endpoint.id == crate::endpoint::LOCAL)
            })
            .map(|endpoint| endpoint.id.clone())
    }

    fn open_request(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.requests.items.iter().find(|r| r.id == id).cloned() else {
            return;
        };
        let (Some(tab), Some(endpoint)) = (request.tab_id.clone(), self.request_endpoint(&request))
        else {
            return;
        };
        self.dismiss_menu(window, cx);
        self.navigate_endpoint(&endpoint, NavigationTarget::Tab(&tab), cx);
    }

    pub(super) fn render_requests(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0., |elapsed| elapsed.as_secs_f64());
        let mut panel = div()
            .debug_selector(|| "requests-menu".into())
            .flex()
            .flex_col()
            .gap(px(2.));
        if let Some(error) = &self.requests.error {
            panel = panel.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(rgb(theme.palette[1]))
                    .child(format!("The list could not be read: {error}")),
            );
        }
        if self.requests.items.is_empty() {
            return panel.child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .text_color(rgb(theme.muted))
                    .child("No requests yet."),
            );
        }
        for request in &self.requests.items {
            let color = match request.state.as_str() {
                "blocked" | "failed" => theme.palette[1],
                "working" | "starting" => theme.palette[3],
                "done" => theme.palette[2],
                _ => theme.muted,
            };
            let id = request.id.clone();
            let opens = request.tab_id.is_some() && request.state != "closed";
            let place = request.title.as_deref().map_or_else(
                || request.host_name.clone(),
                |title| format!("{} · {title}", request.host_name),
            );
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("request-{id}")))
                    .debug_selector(|| format!("request-{id}"))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(opens, |row| {
                        row.cursor_pointer()
                            .hover(|row| row.bg(rgb(theme.active)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_request(&id, window, cx);
                            }))
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .text_color(rgb(theme.muted))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgb(color))
                                    .child(request.state.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .child(crate::requests::age(request.created, now)),
                            )
                            .child(div().min_w_0().truncate().child(place)),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.foreground))
                            .line_clamp(2)
                            .child(request.text.clone()),
                    )
                    .children(request.outcome().map(|outcome| {
                        div()
                            .text_color(rgb(theme.muted))
                            .line_clamp(3)
                            .child(outcome.to_owned())
                    })),
            );
        }
        panel
    }
}
