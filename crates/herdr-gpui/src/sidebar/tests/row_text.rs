use super::*;

#[test]
fn section_headings_use_the_configured_sidebar_font_size() {
    use gpui::{Styled, px};
    for size in [12., 16., 20.] {
        let font = FontConfig {
            family: "Menlo".into(),
            size,
            fallbacks: None,
        };
        for label in ["spaces", "agents"] {
            let mut heading = header(
                label,
                &font,
                &Theme::default(),
                super::super::layout::for_mode(Default::default()),
            );
            assert_eq!(heading.text_style().font_size, Some(px(size).into()));
        }
    }
}

#[test]
fn rows_weight_and_dim_their_text_like_upstream() {
    use super::super::row::{RowKind, row_text};
    use gpui::FontWeight;
    let theme = Theme::default();
    // Agents stay bold whether or not they are the current row; a workspace
    // earns bold only while focused, and hands its branch the accent then.
    for (kind, focused, weight, name, detail) in [
        (
            RowKind::Agent(crate::icons::AgentIcon::Generic),
            false,
            FontWeight::BOLD,
            theme.subtext(),
            theme.muted,
        ),
        (
            RowKind::Agent(crate::icons::AgentIcon::Generic),
            true,
            FontWeight::BOLD,
            theme.foreground,
            theme.muted,
        ),
        (
            RowKind::Workspace,
            false,
            FontWeight::NORMAL,
            theme.subtext(),
            theme.muted,
        ),
        (
            RowKind::Workspace,
            true,
            FontWeight::BOLD,
            theme.foreground,
            theme.primary(),
        ),
    ] {
        assert_eq!(row_text(kind, focused, &theme), (name, weight, detail));
    }
    // Subtext sits between the muted detail and the focused name.
    let brightness = |color: u32| (color >> 16) + ((color >> 8) & 255) + (color & 255);
    assert!(brightness(theme.muted) < brightness(theme.subtext()));
    assert!(brightness(theme.subtext()) < brightness(theme.foreground));
}

#[test]
fn cells_hand_their_state_and_data_to_the_layout() {
    use super::super::{
        cell::{AgentRow, Cell, RowContext, RowData, RowLayout, RowState, WorkspaceRow},
        layout::for_mode,
        row::{RowIcon, RowTree},
    };
    use gpui::{Div, div};
    use std::cell::RefCell;

    /// Records what each call was given instead of drawing it.
    #[derive(Default)]
    struct Recorder(RefCell<Vec<(String, RowState)>>);

    impl RowLayout for Recorder {
        fn workspace(&self, row: WorkspaceRow<'_>, state: RowState, _: &RowContext<'_>) -> Div {
            self.0.borrow_mut().push((row.label.to_owned(), state));
            div()
        }
        fn agent(&self, row: AgentRow<'_>, state: RowState, _: &RowContext<'_>) -> Div {
            self.0.borrow_mut().push((row.key, state));
            div()
        }
    }

    let snapshot = layout_tests::snapshot(1);
    let (font, theme) = (crate::config::Config::default().sidebar, Theme::default());
    let cx = RowContext {
        indicators: Indicators::new(None, false, &theme),
        font: &font,
        theme: &theme,
        look: for_mode(Default::default()),
        width: 232.,
        host: None,
    };
    let recorder = Recorder::default();
    let workspace = || {
        RowData::Workspace(WorkspaceRow {
            workspace: &snapshot.workspaces[0],
            label: "herdr",
            tree: RowTree::None,
            icon: RowIcon::None,
            fold: None,
            grouped: false,
            badge: None,
            removing: false,
            status: snapshot.workspaces[0].agent_status,
            lines: vec![],
        })
    };
    let _ = Cell::new(&recorder, workspace(), &cx).row();
    let _ = Cell::new(&recorder, workspace(), &cx).selected(true).row();
    let _ = Cell::new(
        &recorder,
        RowData::Agent(AgentRow {
            key: "agent-p0".into(),
            name: "Claude Code",
            icon: crate::icons::AgentIcon::Generic,
            status: AgentStatus::Working.into(),
            place: None,
            status_text: None,
            lines: vec![],
            icon_line: None,
        }),
        &cx,
    )
    .highlighted(true)
    .row();
    let state = |selected, highlighted| RowState {
        selected,
        highlighted,
        ..RowState::default()
    };
    assert_eq!(
        recorder.0.into_inner(),
        vec![
            ("herdr".into(), state(false, false)),
            ("herdr".into(), state(true, false)),
            ("agent-p0".into(), state(false, true)),
        ]
    );
}

#[test]
fn upstream_counts_show_only_drift_and_size_to_what_they_print() {
    use super::super::row::Upstream;
    // In sync, no upstream, or a daemon config without `git_status`.
    assert_eq!(Upstream::new(None), None);
    assert_eq!(Upstream::new(Some((0, 0))), None);
    // `↓18`, `↑2`, and `↑2 ↓18` at one unit per glyph.
    assert_eq!(Upstream::new(Some((0, 18))).unwrap().width(1.), 3.);
    assert_eq!(Upstream::new(Some((2, 0))).unwrap().width(1.), 2.);
    assert_eq!(Upstream::new(Some((2, 18))).unwrap().width(1.), 6.);
    assert_eq!(Upstream::new(Some((2, 18))).unwrap().width(7.5), 45.);
}

#[test]
fn teleported_names_fade_but_stay_legible() {
    use crate::config::Theme;
    use crate::contrast::{Contrast, ratio};
    for name in Theme::BUILTIN_NAMES {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = Theme::builtin(name).unwrap().with_contrast(contrast);
            for color in [theme.foreground, theme.subtext()] {
                let faded = super::super::row::left_behind(color, &theme);
                let context = format!("{name} {contrast:?} {color:06x} -> {faded:06x}");
                assert!(
                    ratio(faded, theme.background) < ratio(color, theme.background),
                    "{context}: not faded"
                );
                assert!(
                    ratio(faded, theme.background) >= Contrast::Standard.mark_ratio() - 0.01,
                    "{context}: unreadable"
                );
            }
        }
    }
}
