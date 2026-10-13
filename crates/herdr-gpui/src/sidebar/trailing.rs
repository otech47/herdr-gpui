//! A control at a row's trailing edge: the room it takes from the row's label
//! budget, and the spot in that room, on the row's first line, it is placed at.

use super::{cell::RowContext, line_height};
use gpui::{Div, Stateful, Styled, px};

pub(super) struct Trailing {
    pub(super) size: f32,
    right: f32,
    top: f32,
    reserve: f32,
}

impl Trailing {
    pub(super) fn new(cx: &RowContext<'_>) -> Self {
        let look = cx.look;
        let line = line_height(cx.font);
        let size = line.min(18.);
        Self {
            size,
            right: look.content_x(),
            top: look.row_padding() + look.spacing() / 2. + (line - size) / 2.,
            reserve: size + look.density.gap(),
        }
    }

    /// The row's context with the control's room taken off the width its
    /// label budget is measured from, so the row's own columns end before it.
    pub(super) fn context<'a>(&self, cx: &RowContext<'a>) -> RowContext<'a> {
        RowContext {
            width: cx.width - self.reserve,
            ..*cx
        }
    }

    /// `control` in the reserved room. The row is positioned, so its own
    /// edge is the reference.
    pub(super) fn place(&self, control: Stateful<Div>) -> Stateful<Div> {
        control
            .absolute()
            .right(px(self.right))
            .top(px(self.top))
            .size(px(self.size))
    }
}
