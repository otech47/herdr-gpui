use crate::window::kept_space::{Keep, KeptSpace};
use std::time::{Duration, Instant};

#[test]
fn a_window_keeps_its_space_unless_it_asked_or_the_space_is_gone() {
    let now = Instant::now();
    let exists = |_: &str| true;
    let mut kept = KeptSpace::default();
    assert_eq!(kept.decide(("boot", "w1"), exists, now), Keep::Accept);
    kept.accept("boot", "w0");
    assert_eq!(kept.decide(("boot", "w0"), exists, now), Keep::Accept);
    assert_eq!(
        kept.decide(("boot", "w1"), exists, now),
        Keep::Restore("w0".into())
    );
    assert_eq!(kept.decide(("boot", "w1"), |_| false, now), Keep::Accept);
    assert_eq!(kept.decide(("other", "w1"), exists, now), Keep::Accept);
    kept.asked();
    assert_eq!(kept.decide(("boot", "w1"), exists, now), Keep::Accept);
    assert_eq!(
        kept.decide(("boot", "w1"), exists, now + Duration::from_secs(6)),
        Keep::Restore("w0".into())
    );
}

#[test]
fn a_restore_is_awaited_once_and_a_landed_request_clears_the_grace() {
    let now = Instant::now();
    let exists = |_: &str| true;
    let mut kept = KeptSpace::default();
    kept.accept("boot", "w0");
    kept.asked();
    // accept lands the asked request, so its grace must not cover the next
    // foreign space.
    kept.accept("boot", "w2");
    assert_eq!(
        kept.decide(("boot", "w1"), exists, now),
        Keep::Restore("w2".into())
    );
    assert_eq!(kept.decide(("boot", "w2"), exists, now), Keep::Accept);
}
