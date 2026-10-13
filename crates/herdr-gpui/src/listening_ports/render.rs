//! Drawing a workspace's listening ports: the status bar's globe for the
//! focused workspace, and the menu a globe opens, one line per port, each
//! opening its page in a browser tab of that workspace.

use super::{
    Link, Port,
    tunnel::{self, Key},
};
use crate::{
    browser::WebUrl,
    config::Theme,
    window::{Flash, HerdrWindow},
};
use gpui::{prelude::*, *};

const GAP: f32 = 6.;

impl HerdrWindow {
    /// The focused workspace's ports in the status bar; nothing while hidden
    /// or while it listens on none.
    pub(crate) fn render_listening_ports(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.config.show_listening_ports {
            return None;
        }
        let endpoint = self.endpoints.get(self.selected_endpoint)?;
        let workspace = self
            .live
            .snapshot
            .as_ref()?
            .focused_workspace_id
            .as_deref()?;
        let daemon = super::Daemon::from(&endpoint.connection.target);
        let listed = self.listening_ports.get(&daemon, workspace)?;
        Some(
            div()
                .id("listening-ports")
                .debug_selector(|| "listening-ports".into())
                .flex_none()
                .flex()
                .items_center()
                .h_full()
                .px(px(2.))
                .child(self.ports_bar_globe((&endpoint.id, workspace), listed, cx)),
        )
    }
}

/// The ports as a menu, one line each: the number, then the process that
/// listens on it. A line opens its page and closes the menu; a port this
/// machine cannot reach is listed but stays inert, its hint saying why.
pub(crate) fn menu(
    listed: super::Listed<'_>,
    (endpoint, workspace): (&str, &str),
    theme: &Theme,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    let (muted, foreground, surface, active) =
        (theme.muted, theme.foreground, theme.surface, theme.active);
    div()
        .p(px(GAP))
        .flex()
        .flex_col()
        .rounded(px(crate::config::corners::CONTROL))
        .border_1()
        .border_color(rgb(active))
        .bg(rgb(surface))
        .text_color(rgb(foreground))
        .shadow_lg()
        .children(listed.ports.iter().map(|port| {
            let link = port.link(listed.origin);
            let hint = SharedString::from(hint(port, link.as_ref()));
            let id =
                SharedString::from(format!("port-menu-{endpoint}-{workspace}-{}", port.number));
            let line = div()
                .id(id.clone())
                .debug_selector(|| id.to_string())
                .flex()
                .items_center()
                .gap(px(GAP * 2.))
                .px(px(GAP))
                .py(px(4.))
                .rounded(px(crate::config::corners::SMALL))
                .child(div().flex_none().child(format!(":{}", port.number)))
                .when(!port.process.is_empty(), |line| {
                    line.child(
                        div()
                            .flex_none()
                            .text_color(rgb(muted))
                            .child(port.process.clone()),
                    )
                });
            let Some(link) = link else {
                return line.text_color(rgb(muted)).tooltip(move |_, cx| {
                    cx.new(|_| crate::usage::Hint {
                        text: hint.clone(),
                        foreground,
                        surface,
                    })
                    .into()
                });
            };
            let (endpoint, workspace) = (endpoint.to_owned(), workspace.to_owned());
            line.cursor_pointer()
                .hover(move |line| line.bg(rgb(active)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_ports_menu(window, cx);
                    this.open_port_link(&endpoint, &workspace, &link, window, cx);
                }))
        }))
}

/// `node listening on *:3000` over what clicking does.
fn hint(port: &Port, link: Option<&Link>) -> String {
    let process = if port.process.is_empty() {
        "A process"
    } else {
        port.process.as_str()
    };
    let action = match link {
        Some(Link::Page(url)) => format!("Open {}", url.as_str()),
        Some(Link::Tunnel(key)) => format!("Open through an SSH tunnel to {}", key.target),
        None => "Not reachable from this machine".into(),
    };
    format!("{process} listening on {}\n{action}", port.address())
}

impl HerdrWindow {
    /// Opens what a listed port links to: its page in a browser tab of the
    /// workspace, through a tunnel first when the port is remote and local
    /// to its host.
    pub(crate) fn open_port_link(
        &mut self,
        endpoint: &str,
        workspace: &str,
        link: &Link,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match link {
            Link::Page(url) => {
                self.open_workspace_page(endpoint, workspace, url.clone(), window, cx);
            }
            Link::Tunnel(key) => {
                self.open_tunneled_page(endpoint, workspace, key.clone(), window, cx);
            }
        }
    }

    /// Opens a remote loopback-only port through an SSH tunnel: an existing
    /// one at once, otherwise a new one started on the background executor,
    /// since SSH may take seconds to connect. The page opens once the tunnel
    /// accepts; a window closed meanwhile drops, and so kills, the tunnel.
    pub(crate) fn open_tunneled_page(
        &mut self,
        endpoint: &str,
        workspace: &str,
        key: Key,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.tunnels.local(&key) {
            self.open_local_port(endpoint, workspace, local, window, cx);
            return;
        }
        let Some(preferred) = self.tunnels.begin(&key) else {
            self.show_flash(
                Flash::warning("A tunnel to that port is already opening"),
                cx,
            );
            return;
        };
        let opening = key.clone();
        let task = cx
            .background_executor()
            .spawn(async move { tunnel::open(&opening, preferred) });
        let (endpoint, workspace) = (endpoint.to_owned(), workspace.to_owned());
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(tunnel) => {
                    if let Some(local) = this.tunnels.finish(key, tunnel) {
                        this.open_local_port(&endpoint, &workspace, local, window, cx);
                    }
                }
                Err(error) => {
                    this.tunnels.fail(&key);
                    // The display boundary for a failed tunnel.
                    this.show_flash(
                        Flash::warning(format!("Could not open :{}: {error}", key.port)),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn open_local_port(
        &mut self,
        endpoint: &str,
        workspace: &str,
        local: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match WebUrl::try_from(format!("http://localhost:{local}/").as_str()) {
            Ok(url) => self.open_workspace_page(endpoint, workspace, url, window, cx),
            Err(error) => self.show_flash(Flash::warning(error.to_string()), cx),
        }
    }
}
