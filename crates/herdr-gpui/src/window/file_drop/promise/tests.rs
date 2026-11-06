use super::*;

fn pending() -> Pending {
    Pending { drops: Vec::new() }
}

fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[test]
fn a_drop_waits_for_every_promised_file() {
    let window = WindowId::from(1);
    let mut drops = pending();
    drops.begin(7, window, at(10., 20.), 2);
    drops.arrived(7, Ok("/tmp/a.png".into()));
    assert!(drops.take(window).is_empty());
    drops.arrived(
        7,
        Err(Error::FilePromise {
            reason: "denied".into(),
        }),
    );
    let done = drops.take(window);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].position, at(10., 20.));
    assert_eq!(done[0].paths, [PathBuf::from("/tmp/a.png")]);
    assert!(matches!(done[0].errors[..], [Error::FilePromise { .. }]));
    assert!(drops.take(window).is_empty());
}

#[test]
fn each_window_takes_only_its_own_drops() {
    let (first, second) = (WindowId::from(1), WindowId::from(2));
    let mut drops = pending();
    drops.begin(1, first, at(1., 1.), 1);
    drops.begin(2, second, at(2., 2.), 1);
    drops.arrived(1, Ok("/tmp/one.png".into()));
    drops.arrived(2, Ok("/tmp/two.png".into()));
    let taken = drops.take(second);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].paths, [PathBuf::from("/tmp/two.png")]);
    assert_eq!(drops.take(first)[0].paths, [PathBuf::from("/tmp/one.png")]);
}

#[test]
fn a_promise_without_file_types_still_expects_one_file() {
    let window = WindowId::from(1);
    let mut drops = pending();
    drops.begin(1, window, at(0., 0.), 0);
    assert!(drops.take(window).is_empty());
    drops.arrived(1, Ok("/tmp/one.png".into()));
    assert_eq!(drops.take(window).len(), 1);
}

#[test]
fn unfinished_drops_are_bounded_and_late_files_are_ignored() {
    let window = WindowId::from(1);
    let mut drops = pending();
    for id in 0..=MAX_PENDING as u64 {
        drops.begin(id, window, at(0., 0.), 1);
    }
    assert_eq!(drops.drops.len(), MAX_PENDING);
    drops.arrived(0, Ok("/tmp/forgotten.png".into()));
    assert!(drops.take(window).is_empty());
    drops.arrived(MAX_PENDING as u64, Ok("/tmp/kept.png".into()));
    assert_eq!(
        drops.take(window)[0].paths,
        [PathBuf::from("/tmp/kept.png")]
    );
}
