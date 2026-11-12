//! A sidebar fixture that stays open for looking at natively: two devices
//! sharing a space, the tabs list in one of its views. `--sidebar-test`
//! with `HERDR_TEST_SIDEBAR_HOLD` set to `all`, `by-device` or `device`,
//! and `HERDR_TEST_SIDEBAR_SORT` set to `attention` for the sorted list.

use super::*;
use crate::{
    browser,
    config::{Density, LayoutMode, Style},
    endpoint::Endpoint,
    monitoring::{Tab, Watched, Watchers},
    preferences::{TabsSort, TabsView},
    state::ConnectionStatus,
    usage::Host,
};

pub(super) fn held_view() -> Option<TabsView> {
    Some(
        match std::env::var("HERDR_TEST_SIDEBAR_HOLD").ok()?.as_str() {
            "all" => TabsView::All,
            "by-device" => TabsView::ByDevice,
            _ => TabsView::Device,
        },
    )
}

fn tabs(
    workspace: &str,
    prefix: &str,
    titles: &[(&str, Option<&str>, &str)],
) -> Result<(Vec<ClientShellTab>, Vec<ClientShellAgent>)> {
    let mut tabs = Vec::new();
    let mut agents = Vec::new();
    for (index, (title, agent, status)) in titles.iter().enumerate() {
        let tab_id = format!("{prefix}{index}");
        tabs.push(serde_json::from_value(serde_json::json!({
            "tab_id": tab_id, "workspace_id": workspace, "number": index + 1,
            "label": title, "custom_label": true, "zoomed": false,
            "focused": index == 0, "agent_status": status
        }))?);
        if let Some(agent) = agent {
            agents.push(serde_json::from_value(serde_json::json!({
                "pane_id": format!("{prefix}p{index}"), "workspace_id": workspace,
                "tab_id": tab_id, "name": title, "display_agent": agent, "agent": agent,
                "agent_status": status, "state_change_seq": 0, "state_labels": [],
                "tokens": [], "focused": false
            }))?);
        }
    }
    Ok((tabs, agents))
}

fn populate(this: &mut HerdrWindow, view: TabsView) -> Result<()> {
    this.config.layout.mode = LayoutMode::new(Density::Comfortable, Style::Rounded);
    this.config.show_sidebar_tabs = true;
    this.config.show_tab_strip = false;
    this.config.show_agents = false;
    this.sidebar_width = Some(314.);
    this.sidebar_tabs_view = view;
    if std::env::var("HERDR_TEST_SIDEBAR_SORT").is_ok_and(|sort| sort == "attention") {
        this.sidebar_tabs_sort = TabsSort::Attention;
    }

    let mut local = sidebar::layout_tests::snapshot(2);
    local.workspaces[0].label = "fedi".into();
    local.workspaces[1].label = "tools".into();
    local.focused_workspace_id = Some("w0".into());
    (local.tabs, local.agents) = tabs(
        "w0",
        "t",
        &[
            ("deployment-safety-net", Some("claude"), "working"),
            ("action-items", Some("claude"), "idle"),
            ("pr 12382 review", Some("codex"), "working"),
            ("manifold mg seats", Some("claude"), "blocked"),
            ("fedi", None, "idle"),
            ("nix develop", None, "idle"),
            ("cargo nextest fedimint-server", Some("claude"), "working"),
            ("worktree cleanup", Some("kimi"), "done"),
        ],
    )?;
    this.live.status = ConnectionStatus::Connected;
    this.live.snapshot = Some(Arc::new(local));
    this.endpoints[0].live = this.live.clone();
    this.monitoring
        .seed(Host::Local, watchers(&[("t1", "claude")]));

    let mut remote = Endpoint::new(
        "ssh:mini".into(),
        "Mini".into(),
        ConnectTarget::Ssh {
            target: "unused".into(),
            session: "default".into(),
        },
        true,
    );
    let mut snapshot = sidebar::layout_tests::snapshot(1);
    snapshot.workspaces[0].label = "fedi".into();
    (snapshot.tabs, snapshot.agents) = tabs(
        "w0",
        "r",
        &[
            ("overnight operator run", Some("claude"), "working"),
            ("e2e tests", None, "idle"),
            ("mini build", Some("codex"), "idle"),
        ],
    )?;
    remote.live.status = ConnectionStatus::Connected;
    remote.live.snapshot = Some(Arc::new(snapshot));
    this.endpoints.push(remote);
    this.monitoring
        .seed(Host::Ssh("unused".into()), watchers(&[("r2", "codex")]));
    Ok(())
}

/// Tabs with a shell running under the named agent, as a scan would find them.
fn watchers(entries: &[(&str, &str)]) -> Watchers {
    entries
        .iter()
        .map(|&(tab, under)| {
            (
                Tab {
                    id: tab.into(),
                    socket: String::new(),
                },
                Watched {
                    under: vec![under.into()],
                    disowned: false,
                    stopped: false,
                },
            )
        })
        .collect()
}

pub(super) fn hold_sidebar(handle: WindowHandle<HerdrWindow>, cx: &mut App, view: TabsView) {
    let result = handle.update(cx, |this, window, cx| {
        populate(this, view)?;
        window.resize(size(px(1000.), px(700.)));
        if std::env::var_os("HERDR_TEST_TABS_OPTIONS").is_some() {
            this.open_tabs_options(point(px(283.), px(170.)), window, cx);
        }
        if let Ok(mode) = std::env::var("HERDR_TEST_AGENT_SESSIONS") {
            this.preview_agent_sessions(&mode, window, cx);
            if std::env::var_os("HERDR_TEST_NARROW").is_some() {
                window.resize(size(px(520.), px(640.)));
            }
        }
        if let Ok(url) = std::env::var("HERDR_TEST_SESSION_PAGE") {
            let url = browser::WebUrl::try_from(url.as_str())?;
            let (scope, workspace) = this
                .browser_key()
                .ok_or_else(|| anyhow::anyhow!("fixture has no workspace"))?;
            let page = browser::Store::update(cx, |store| {
                let id = store.open(
                    scope.clone(),
                    &workspace,
                    Some(browser::Location::Web { url }),
                    None,
                )?;
                store.attach_to_session(id, browser::Pick::Herdr("t0".into()));
                store.visited(id, None, Some("navigation review"));
                let second = store.open(scope, &workspace, None, None)?;
                store.attach_to_session(second, browser::Pick::Herdr("t0".into()));
                store.visited(
                    second,
                    None,
                    Some("API response details with a long page title"),
                );
                Some(id)
            })
            .ok_or_else(|| anyhow::anyhow!("fixture page could not open"))?;
            this.show_browser_tab(page, window, cx);
            if std::env::var_os("HERDR_TEST_NARROW").is_some() {
                window.resize(size(px(720.), px(640.)));
            }
        }
        cx.notify();
        Ok::<(), anyhow::Error>(())
    });
    match result {
        Ok(Ok(())) => eprintln!("SIDEBAR holding {view:?}"),
        other => {
            eprintln!("SIDEBAR hold FAIL: {other:?}");
            std::process::exit(1);
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::var_os("HERDR_TEST_WEBVIEW_CAPTURE") {
        HerdrWindow::check_native_session_page(handle, path.into(), cx);
    }
    #[cfg(feature = "mockup")]
    if let Some(path) = std::env::var_os("HERDR_TEST_SIDEBAR_CAPTURE") {
        let delay = std::env::var("HERDR_TEST_CAPTURE_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(750)
            .clamp(750, 15000);
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let result = handle.update(cx, |_, window, _| window.render_to_image());
            match result {
                Ok(Ok(image)) => {
                    let result = cx
                        .background_executor()
                        .spawn(async move { image.save(std::path::PathBuf::from(path)) })
                        .await;
                    eprintln!("SIDEBAR capture: {result:?}");
                }
                other => eprintln!("SIDEBAR capture FAIL: {other:?}"),
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}
