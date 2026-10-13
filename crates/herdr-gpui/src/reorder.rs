//! Reordering by dragging, shared by the sidebar's workspace rows and the
//! tab strips: when a press lifts, how the carried item finds its gap among
//! the others, and how the others slide aside to preview the drop. Positions
//! are along one axis, so the same rules serve a column and a row.

use gpui::{Pixels, Point, Task};
use std::{
    cell::RefCell,
    collections::HashMap,
    time::{Duration, Instant},
};

/// How long a press rests before what it pressed lifts.
pub(crate) const LIFT_DELAY: Duration = Duration::from_millis(250);
/// How far a press may travel before it lifts without waiting.
pub(crate) const LIFT_DISTANCE: f32 = 4.;
/// How long a dropped preview waits for the daemon's new order before the
/// list falls back to the order it last received.
pub(crate) const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long an item takes to slide to its previewed place. Tests settle at
/// once: they check where rows go, and a frame clock would only add waits.
pub(crate) const SLIDE: Duration = if cfg!(test) {
    Duration::ZERO
} else {
    Duration::from_millis(150)
};

/// A press on a sidebar row that may become a reorder: the row it holds,
/// how far the pointer has carried it, and the gap `T` it is over.
pub(crate) struct Drag<T> {
    pub(crate) boot: String,
    pub(crate) key: String,
    origin: Point<Pixels>,
    pointer: Point<Pixels>,
    pub(crate) lifted: bool,
    /// The gap under the pointer, `None` over the carried unit's own place.
    pub(crate) target: Option<T>,
    /// The order the list had when the drag was dropped with a move sent,
    /// kept so the preview lasts exactly until the daemon's answer.
    dropped: Option<Vec<String>>,
    /// Each shifted row's slide, by key. Render reads and advances them, so
    /// they live behind a cell; they end with the drag.
    slides: RefCell<HashMap<String, Slide>>,
    /// Lifts the row once the press has rested, then ends a dropped preview
    /// the daemon never answered. Dropping the drag cancels it.
    timer: Task<()>,
}

impl<T> Drag<T> {
    pub(crate) fn new(boot: String, key: String, origin: Point<Pixels>, timer: Task<()>) -> Self {
        Self {
            boot,
            key,
            origin,
            pointer: origin,
            lifted: false,
            target: None,
            dropped: None,
            slides: RefCell::default(),
            timer,
        }
    }

    /// How far the lifted row has followed the pointer.
    pub(crate) fn offset(&self) -> Pixels {
        self.pointer.y - self.origin.y
    }

    /// Where a row paints on its way to shift `to`, and whether it is still
    /// moving. A row starts from its resting place.
    pub(crate) fn slide(&self, key: &str, to: f32, now: Instant) -> (f32, bool) {
        let mut slides = self.slides.borrow_mut();
        let slide = slides.entry(key.to_owned()).or_insert(Slide {
            from: 0.,
            to: 0.,
            start: now,
        });
        *slide = slide.toward(to, now, SLIDE);
        (slide.at(now, SLIDE), slide.progress(now, SLIDE) < 1.)
    }

    /// Records a carried row at the pointer, so it slides from there once
    /// dropped rather than from where it was picked up.
    pub(crate) fn pin(&self, key: &str, at: f32, now: Instant) {
        self.slides.borrow_mut().insert(
            key.to_owned(),
            Slide {
                from: at,
                to: at,
                start: now,
            },
        );
    }

    /// Whether the lifted row still follows the pointer.
    pub(crate) fn floating(&self) -> bool {
        self.lifted && self.dropped.is_none()
    }

    pub(crate) fn is_dropped(&self) -> bool {
        self.dropped.is_some()
    }

    /// Whether the list should show the drop in place: while it floats, and
    /// after it is dropped until the daemon's order replaces the old one.
    pub(crate) fn previewing<'a>(&self, order: impl IntoIterator<Item = &'a str>) -> bool {
        self.lifted
            && self
                .dropped
                .as_ref()
                .is_none_or(|dropped| dropped.iter().map(String::as_str).eq(order))
    }

    /// Lifts the row. Returns whether it was resting.
    pub(crate) fn lift(&mut self) -> bool {
        !std::mem::replace(&mut self.lifted, true)
    }

    /// Follows the pointer, lifting the row once it has travelled far
    /// enough. Returns whether the row was lifted by this move.
    pub(crate) fn follow(&mut self, position: Point<Pixels>) -> bool {
        self.pointer = position;
        let moved = position - self.origin;
        !self.lifted && f32::from(moved.x.abs().max(moved.y.abs())) > LIFT_DISTANCE && self.lift()
    }

    /// Keeps the preview after a move was sent, until the daemon's order
    /// differs from `order` or `timer` ends it.
    pub(crate) fn drop(&mut self, order: Vec<String>, timer: Task<()>) {
        self.dropped = Some(order);
        self.timer = timer;
    }
}

/// Where a dropped item lands: beside another, since a drop past the last
/// item shown still has to land after it rather than at the very end of a
/// longer list the view shows only part of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Beside<T> {
    Before(T),
    After(T),
}

impl<T> Beside<T> {
    /// The same side of what `f` makes of the anchor, if it makes anything.
    pub(crate) fn and_then<U>(self, f: impl FnOnce(T) -> Option<U>) -> Option<Beside<U>> {
        Some(match self {
            Self::Before(anchor) => Beside::Before(f(anchor)?),
            Self::After(anchor) => Beside::After(f(anchor)?),
        })
    }
}

/// An item gliding between two shifts, eased out so it arrives gently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slide {
    pub(crate) from: f32,
    pub(crate) to: f32,
    pub(crate) start: Instant,
}

impl Slide {
    pub(crate) fn progress(&self, now: Instant, duration: Duration) -> f32 {
        if duration.is_zero() {
            return 1.;
        }
        (now.saturating_duration_since(self.start).as_secs_f32() / duration.as_secs_f32()).min(1.)
    }

    pub(crate) fn at(&self, now: Instant, duration: Duration) -> f32 {
        let eased = 1. - (1. - self.progress(now, duration)).powi(3);
        self.from + (self.to - self.from) * eased
    }

    /// Heads for `to` from wherever the row is now, so a gap that moves
    /// mid-slide turns the row around instead of making it jump.
    pub(crate) fn toward(self, to: f32, now: Instant, duration: Duration) -> Self {
        if (self.to - to).abs() <= f32::EPSILON {
            return self;
        }
        Self {
            from: self.at(now, duration),
            to,
            start: now,
        }
    }
}

/// The gap the carried card points to, from every unit's resting span and
/// the card's own. Moving up, the card passes a unit once its top edge is
/// above that unit's middle; moving down, once its bottom edge is below it.
/// Units without a span (hidden rows) never block the card.
pub(crate) fn slot_for(spans: &[Option<(f32, f32)>], dragged: usize, card: (f32, f32)) -> usize {
    let center = |unit: usize| spans[unit].map(|(top, bottom)| (top + bottom) / 2.);
    if let Some(unit) = (0..dragged).find(|&unit| center(unit).is_some_and(|c| card.0 < c)) {
        return unit;
    }
    (dragged + 1..spans.len())
        .rev()
        .find(|&unit| center(unit).is_some_and(|c| card.1 > c))
        .map_or(dragged, |unit| unit + 1)
}

/// How far each unit moves to preview dropping `dragged` into `slot`, given
/// every unit's height: the units it passes close its place, and it takes the
/// room they left. A gap that moves nothing moves no row.
pub(crate) fn preview(heights: &[f32], dragged: usize, slot: usize) -> Vec<f32> {
    let mut shifts = vec![0.; heights.len()];
    let hole = heights[dragged];
    if slot > dragged + 1 && slot <= heights.len() {
        shifts[dragged + 1..slot].fill(-hole);
        shifts[dragged] = heights[dragged + 1..slot].iter().sum();
    } else if slot < dragged {
        shifts[slot..dragged].fill(hole);
        shifts[dragged] = -heights[slot..dragged].iter().sum::<f32>();
    }
    shifts
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn the_card_passes_a_unit_once_its_edge_crosses_that_unit_s_middle() {
        // Four 20px units; the card is unit 1, resting at 20..40.
        let spans = [
            Some((0., 20.)),
            Some((20., 40.)),
            Some((40., 60.)),
            Some((60., 80.)),
        ];
        let at = |lift: f32| slot_for(&spans, 1, (20. + lift, 40. + lift));
        assert_eq!(at(0.), 1);
        // Up: the top edge must pass unit 0's middle at 10.
        assert_eq!(at(-9.), 1);
        assert_eq!(at(-11.), 0);
        // Down: the bottom edge must pass unit 2's middle at 50, then 3's at 70.
        assert_eq!(at(9.), 1);
        assert_eq!(at(11.), 3);
        assert_eq!(at(31.), 4);
        // A hidden unit is skipped rather than blocking.
        let hidden = [None, Some((20., 40.)), Some((40., 60.))];
        assert_eq!(slot_for(&hidden, 1, (0., 20.)), 1);
    }

    #[test]
    fn a_slide_eases_to_its_target_and_turns_around_from_where_it_is() {
        let start = Instant::now();
        let duration = Duration::from_millis(100);
        let at = |ms| start + Duration::from_millis(ms);
        let slide = Slide {
            from: 0.,
            to: 40.,
            start,
        };
        assert_eq!(slide.at(start, duration), 0.);
        // Eased out: past half the distance at half the time.
        assert_eq!(slide.at(at(50), duration), 35.);
        assert_eq!(slide.at(at(100), duration), 40.);
        assert_eq!(slide.at(at(500), duration), 40.);
        // The same target keeps the slide going.
        assert_eq!(slide.toward(40., at(50), duration), slide);
        // A new target starts from where the row is.
        let back = slide.toward(0., at(50), duration);
        assert_eq!((back.from, back.to, back.start), (35., 0., at(50)));
        assert_eq!(Slide { start, ..slide }.at(at(50), Duration::ZERO), 40.);
    }

    #[test]
    fn the_preview_closes_the_carried_place_and_opens_the_gap() {
        let heights = [10., 20., 30., 40.];
        // Over its own place, nothing moves.
        assert_eq!(preview(&heights, 1, 1), [0.; 4]);
        assert_eq!(preview(&heights, 1, 2), [0.; 4]);
        // Down past two units: they rise by its height, it drops by theirs.
        assert_eq!(preview(&heights, 1, 4), [0., 70., -20., -20.]);
        // Up past one: it rises by that unit's height, which moves down.
        assert_eq!(preview(&heights, 1, 0), [20., -10., 0., 0.]);
    }
}
