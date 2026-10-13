//! A workspace's listening ports behind a globe at its row's trailing edge.
//! Resting on the globe opens a menu of the ports over it; leaving both
//! closes it again, so the row itself stays one line.

use super::trailing::Trailing;
use crate::{HerdrWindow, fonts::StyledFont, listening_ports::Listed};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

/// A rest this long on the globe opens the menu; a pass across it does not.
pub(super) const OPEN_DELAY: Duration = Duration::from_millis(120);
/// Off both the globe and the menu this long closes it; a slip over an edge
/// on the way to a line does not.
pub(super) const CLOSE_GRACE: Duration = Duration::from_millis(200);
/// A dismissed menu is forgotten once the pointer has moved this far from
/// where it chose.
const DISMISS_SLOP: f32 = 24.;

enum Phase {
    /// The pointer entered the globe then; the menu shows after the delay.
    Arming(Instant),
    /// Drawn over its globe; `left_at` runs the close grace.
    Shown { left_at: Option<Instant> },
    /// A line was chosen with the pointer here. The menu stays down until
    /// the pointer has left the globe, so it does not reopen under a rest.
    Dismissed(Point<Pixels>),
}

/// The ports menu a globe is opening or has open. The globe's and the menu's
/// hovers only report here; the frame poll decides, so their order within
/// one event does not matter.
pub(crate) struct PortsMenu {
    endpoint: String,
    workspace: String,
    over_globe: bool,
    inside: bool,
    phase: Phase,
}

impl PortsMenu {
    fn is_for(&self, endpoint: &str, workspace: &str) -> bool {
        self.endpoint == endpoint && self.workspace == workspace
    }

    fn shown(&self) -> bool {
        matches!(self.phase, Phase::Shown { .. })
    }
}

impl HerdrWindow {
    /// The globe for `workspace`'s row, placed by `trailing`, with the menu
    /// over it while open. A press stays here, so the row neither selects nor
    /// lifts under it.
    pub(super) fn ports_globe(
        &self,
        trailing: &Trailing,
        (endpoint, workspace): (&str, &str),
        listed: Listed<'_>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let open = self
            .ports_menu
            .as_ref()
            .is_some_and(|menu| menu.is_for(endpoint, workspace) && menu.shown());
        let (foreground, muted) = (theme.foreground, theme.muted);
        let wash = rgba((foreground << 8) | 0x24);
        let id = format!("ports-{endpoint}-{workspace}");
        let selector = id.clone();
        let hover = (endpoint.to_owned(), workspace.to_owned());
        let click = hover.clone();
        let menu = open.then(|| self.ports_menu_panel((endpoint, workspace), listed, cx));
        trailing.place(
            div()
                .id(SharedString::from(id))
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::config::corners::SMALL))
                .when(open, |globe| globe.bg(wash))
                .hover(move |globe| globe.bg(wash))
                .child(
                    svg()
                        .path("icons/globe.svg")
                        .size(px((trailing.size * 0.7).round()))
                        .text_color(rgb(if open { foreground } else { muted })),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.hover_ports_globe(&hover.0, &hover.1, *hovered, cx);
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_ports_menu(&click.0, &click.1, true, cx);
                }))
                .children(menu),
        )
    }

    /// The menu with its top right corner on the globe's, so the pointer that
    /// opened it starts inside. It occludes what it covers: the rows under it
    /// neither hover nor take a press meant for a line.
    fn ports_menu_panel(
        &self,
        (endpoint, workspace): (&str, &str),
        listed: Listed<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let font = &self.config.ui;
        let panel = crate::listening_ports::menu(listed, (endpoint, workspace), &self.theme, cx)
            .id("ports-menu")
            .debug_selector(|| "ports-menu".into())
            .occlude()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if let Some(menu) = &mut this.ports_menu {
                    menu.inside = *hovered;
                    cx.notify();
                }
            }));
        // The anchor point needs these insets: a flex child without them is
        // centered on the globe.
        div()
            .absolute()
            .top_0()
            .right_0()
            .size_0()
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopRight)
                        .snap_to_window_with_margin(Edges::all(px(12.)))
                        .child(panel),
                )
                .with_priority(2),
            )
            .into_any_element()
    }

    fn hover_ports_globe(
        &mut self,
        endpoint: &str,
        workspace: &str,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        if hovered {
            // Resting on the globe is not resting on the row.
            self.hover = None;
            self.open_ports_menu(endpoint, workspace, false, cx);
            return;
        }
        let Some(menu) = &mut self.ports_menu else {
            return;
        };
        if !menu.is_for(endpoint, workspace) {
            return;
        }
        menu.over_globe = false;
        if matches!(menu.phase, Phase::Dismissed(_)) {
            self.ports_menu = None;
        }
    }

    /// Arms the menu for a row from a hover, or shows it at once for a click.
    fn open_ports_menu(
        &mut self,
        endpoint: &str,
        workspace: &str,
        at_once: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = &mut self.ports_menu
            && menu.is_for(endpoint, workspace)
        {
            menu.over_globe = true;
            if at_once && !menu.shown() {
                menu.phase = Phase::Shown { left_at: None };
                cx.notify();
            }
            return;
        }
        self.ports_menu = Some(PortsMenu {
            endpoint: endpoint.to_owned(),
            workspace: workspace.to_owned(),
            over_globe: true,
            inside: false,
            phase: if at_once {
                Phase::Shown { left_at: None }
            } else {
                Phase::Arming(Instant::now())
            },
        });
        if at_once {
            cx.notify();
        }
    }

    /// Takes the menu down after a line was chosen.
    pub(crate) fn dismiss_ports_menu(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.ports_menu {
            menu.inside = false;
            menu.phase = Phase::Dismissed(window.mouse_position());
            cx.notify();
        }
    }

    /// Moves the menu through its phases by where the pointer has rested;
    /// the frame poll calls this.
    pub(crate) fn poll_ports_menu(
        &mut self,
        now: Instant,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = &mut self.ports_menu else {
            return;
        };
        if self.menu.page.is_some() {
            self.ports_menu = None;
            cx.notify();
            return;
        }
        match &mut menu.phase {
            Phase::Arming(since) => {
                if !menu.over_globe {
                    self.ports_menu = None;
                } else if now.saturating_duration_since(*since) >= OPEN_DELAY {
                    menu.phase = Phase::Shown { left_at: None };
                    cx.notify();
                }
            }
            Phase::Shown { left_at } => {
                if menu.over_globe || menu.inside {
                    *left_at = None;
                    return;
                }
                let since = *left_at.get_or_insert(now);
                if now.saturating_duration_since(since) >= CLOSE_GRACE {
                    self.ports_menu = None;
                    cx.notify();
                }
            }
            Phase::Dismissed(at) => {
                let drift = window.mouse_position() - *at;
                if !menu.over_globe
                    && (drift.x.abs() > px(DISMISS_SLOP) || drift.y.abs() > px(DISMISS_SLOP))
                {
                    self.ports_menu = None;
                }
            }
        }
    }
}
