use super::*;
use herdr_board::DeviceStatus;
use std::{
    thread,
    time::{Duration, Instant},
};

#[test]
fn the_revision_grows_only_when_the_board_changes() {
    let hub = hub(&["local", "ssh:box"]);
    let first = hub.current();
    assert_eq!(first.revision, 7);
    assert_eq!(board(&first.json).revision, 7);
    assert_eq!(board(&first.json).devices.len(), 2);

    connect(&hub, 0, "working");
    assert_eq!(hub.current().revision, 8);
    // The daemon repeats a snapshot whenever a terminal redraws.
    connect(&hub, 0, "working");
    assert_eq!(hub.current().revision, 8);

    connect(&hub, 0, "blocked");
    let latest = hub.current();
    assert_eq!(latest.revision, 9);
    let latest = board(&latest.json);
    assert_eq!(latest.revision, 9);
    assert_eq!(latest.devices[0].spaces[0].tabs[0].state, "blocked");
    assert_eq!(latest.waiting.len(), 1);

    // An unknown device is ignored.
    connect(&hub, 5, "done");
    assert_eq!(hub.current().revision, 9);
}

#[test]
fn a_device_reports_its_connection() {
    let hub = hub(&["local"]);
    let status = |hub: &Hub| board(&hub.current().json).devices[0].clone();
    assert_eq!(status(&hub).status, DeviceStatus::Disconnected);

    hub.update(0, |link| link.connecting = true).unwrap();
    assert_eq!(status(&hub).status, DeviceStatus::Connecting);

    connect(&hub, 0, "idle");
    assert_eq!(status(&hub).status, DeviceStatus::Connected);
    assert_eq!(status(&hub).spaces.len(), 1);

    hub.update(0, |link| {
        *link = crate::hub::Link {
            detail: Some("the daemon went away".to_owned()),
            ..Default::default()
        };
    })
    .unwrap();
    let device = status(&hub);
    assert_eq!(device.status, DeviceStatus::Disconnected);
    assert_eq!(device.detail.as_deref(), Some("the daemon went away"));
    assert!(device.spaces.is_empty());
}

#[test]
fn a_wait_ends_on_the_next_board_or_its_time() {
    let hub = Arc::new(hub(&["local"]));
    assert_eq!(hub.newer_than(6, Duration::ZERO).unwrap().revision, 7);

    let started = Instant::now();
    assert!(hub.newer_than(7, Duration::from_millis(30)).is_none());
    assert!(started.elapsed() >= Duration::from_millis(30));

    let waiter = {
        let hub = hub.clone();
        thread::spawn(move || hub.newer_than(7, Duration::from_secs(30)))
    };
    connect(&hub, 0, "working");
    assert_eq!(waiter.join().unwrap().unwrap().revision, 8);
}
