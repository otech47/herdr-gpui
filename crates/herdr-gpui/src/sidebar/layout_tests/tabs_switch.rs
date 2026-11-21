use super::tabs_attention::five_tabs;
use super::tabs_views::draw;
use super::*;
use crate::preferences::{TabsSort, TabsView};

#[gpui::test]
fn switched_sessions_share_a_title_until_one_tab_remains(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::Device;
        view.sidebar_width = Some(314.);
        view.sidebar_tabs_sort = TabsSort::Manual;
        let snap = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        for (tab, (id, number)) in
            snap.tabs
                .iter_mut()
                .zip([("t16", 38), ("t1", 1), ("t2", 2), ("t3", 3), ("t1M", 52)])
        {
            tab.tab_id = format!("w0:{id}");
            tab.number = number;
        }
        snap.workspaces[0].tokens = [
            ("switch_t16", "claude previous"),
            ("switch_t1M", "codex ready"),
        ]
        .map(|(key, value)| {
            (
                key.into(),
                format!("42c371c7-6937-4c41-b54a-d361b5ce5332 {value}"),
            )
        })
        .into();
        assert_eq!(view.listed_tab(2).unwrap().1, "w0:t1M");
    });
    draw(cx);
    let source = cx.debug_bounds("row-tab-w0:t16").unwrap();
    let target = cx.debug_bounds("row-tab-w0:t1M").unwrap();
    let other = cx.debug_bounds("row-tab-w0:t1").unwrap();
    assert!(source.bottom() <= target.top() && target.bottom() <= other.top());
    assert!(cx.debug_bounds("tab-close-w0:t16").is_some());
    let close = cx.debug_bounds("tab-close-w0:t1M").unwrap();
    let state = cx.debug_bounds("name-tab-w0:t1M-tag").unwrap();
    assert!(state.right() <= close.left());
    cx.update(|_, cx| {
        let labels = &cx.global::<TextProbes>().0;
        for label in ["tab 1", "Claude", "Previous", "Codex", "Ready"] {
            assert!(labels.contains_key(label), "{label}");
        }
    });
    let point = target.center();
    cx.simulate_mouse_down(point, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, Modifiers::default());
    view.update(cx, |view, _| {
        assert_eq!(
            view.pending_navigation,
            Some(crate::NavigationTarget::Tab("w0:t1M".into()))
        );
        let snap = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snap.tabs.retain(|tab| tab.tab_id != "w0:t16");
        assert!(!view.numbers_listed_tabs());
    });
    draw(cx);
    cx.update(|_, cx| {
        let labels = &cx.global::<TextProbes>().0;
        assert!(labels.contains_key("tab 5"));
        assert!(!labels.contains_key("Previous"));
    });
}
