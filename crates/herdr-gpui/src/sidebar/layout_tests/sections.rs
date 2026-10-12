use super::*;
use crate::{preferences::Section, sidebar::SidebarDrag};

#[gpui::test]
fn sections_fold_to_their_headers_and_their_handles_follow_the_pointer(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::{MouseButton, MouseDownEvent};

    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let spaces = cx.debug_bounds("spaces-scroll").unwrap();
    let tabs = cx.debug_bounds("tabs-scroll").unwrap();
    let agents = cx.debug_bounds("agents-scroll").unwrap();
    assert!(tabs.size.height > px(0.));
    assert!(spaces.bottom() <= tabs.top());
    assert!(tabs.bottom() <= agents.top());
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let position = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    };

    click(cx, "section-toggle-tabs");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.sidebar_collapsed.is(Section::Tabs));
        assert!(view.sidebar_sections_modified);
    });
    assert!(cx.debug_bounds("tabs-scroll").is_none());
    assert!(cx.debug_bounds("header-tabs").is_some());
    assert!(cx.debug_bounds("sidebar-split-resize").is_none());
    assert!(cx.debug_bounds("sidebar-spaces-resize").is_some());
    assert!(cx.debug_bounds("agents-scroll").unwrap().size.height > agents.size.height);
    assert_eq!(cx.debug_bounds("spaces-scroll").unwrap(), spaces);

    click(cx, "section-toggle-agents");
    draw(cx);
    assert!(cx.debug_bounds("agents-scroll").is_none());
    assert!(cx.debug_bounds("sidebar-spaces-resize").is_none());

    click(cx, "section-toggle-tabs");
    click(cx, "section-toggle-agents");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_collapsed, Default::default());
    });
    assert_eq!(cx.debug_bounds("tabs-scroll").unwrap(), tabs);
    assert_eq!(cx.debug_bounds("agents-scroll").unwrap(), agents);

    let handle = cx.debug_bounds("sidebar-spaces-resize").unwrap();
    let end = point(sidebar.right() + px(50.), handle.center().y + px(60.));
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_drag, Some(SidebarDrag::Spaces));
        let height = view.sidebar_spaces_height.unwrap();
        assert!(
            (height - f32::from(spaces.size.height) - 60.).abs() < 1.,
            "{height}"
        );
    });
    let dragged = cx.debug_bounds("spaces-scroll").unwrap();
    assert!((dragged.size.height - spaces.size.height - px(60.)).abs() <= px(1.));
    let handle = cx.debug_bounds("sidebar-spaces-resize").unwrap();
    assert!((handle.center().y - end.y).abs() <= px(1.), "{handle:?}");
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| assert!(view.sidebar_drag.is_none()));

    let handle = cx.debug_bounds("sidebar-split-resize").unwrap();
    let end = point(sidebar.right() + px(50.), handle.center().y - px(40.));
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    let handle = cx.debug_bounds("sidebar-split-resize").unwrap();
    assert!(
        (handle.center().y - end.y).abs() <= px(1.),
        "{handle:?} {end:?}"
    );
    assert_eq!(cx.debug_bounds("spaces-scroll").unwrap(), dragged);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());

    let position = cx.debug_bounds("sidebar-spaces-resize").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_spaces_height, None);
        assert!(view.sidebar_drag.is_none());
    });
    assert_eq!(cx.debug_bounds("spaces-scroll").unwrap(), spaces);
}
