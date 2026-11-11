use super::*;
use herdr_board::DeviceStatus;
use std::{
    thread,
    time::{Duration, Instant},
};

#[test]
fn the_revision_grows_only_when_the_board_changes() {
    let (hub, devices) = hub(&["local", "ssh:box"]);
    let first = hub.current();
    assert_eq!(first.revision, 7);
    assert_eq!(board(&first.json).revision, 7);
    assert_eq!(board(&first.json).devices.len(), 2);

    connect(&hub, &devices[0], "working");
    assert_eq!(hub.current().revision, 8);
    // The daemon repeats a snapshot whenever a terminal redraws.
    connect(&hub, &devices[0], "working");
    assert_eq!(hub.current().revision, 8);

    connect(&hub, &devices[0], "blocked");
    let latest = hub.current();
    assert_eq!(latest.revision, 9);
    let latest = board(&latest.json);
    assert_eq!(latest.revision, 9);
    assert_eq!(latest.devices[0].spaces[0].tabs[0].state, "blocked");
    assert_eq!(latest.waiting.len(), 1);
}

#[test]
fn a_device_reports_its_connection() {
    let (hub, devices) = hub(&["local"]);
    let status = |hub: &Hub| board(&hub.current().json).devices[0].clone();
    assert_eq!(status(&hub).status, DeviceStatus::Disconnected);

    hub.update(&devices[0], |link| link.connecting = true)
        .unwrap();
    assert_eq!(status(&hub).status, DeviceStatus::Connecting);

    connect(&hub, &devices[0], "idle");
    assert_eq!(status(&hub).status, DeviceStatus::Connected);
    assert_eq!(status(&hub).spaces.len(), 1);

    hub.update(&devices[0], |link| {
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
    let (hub, devices) = hub(&["local"]);
    let hub = Arc::new(hub);
    assert_eq!(hub.newer_than(6, Duration::ZERO).unwrap().revision, 7);

    let started = Instant::now();
    assert!(hub.newer_than(7, Duration::from_millis(30)).is_none());
    assert!(started.elapsed() >= Duration::from_millis(30));

    let waiter = {
        let hub = hub.clone();
        thread::spawn(move || hub.newer_than(7, Duration::from_secs(30)))
    };
    connect(&hub, &devices[0], "working");
    assert_eq!(waiter.join().unwrap().unwrap().revision, 8);
}

#[test]
fn the_followed_devices_can_change() {
    let (hub, first) = hub(&["local", "ssh:box"]);
    connect(&hub, &first[0], "working");
    connect(&hub, &first[1], "blocked");
    let ids = |hub: &Hub| -> Vec<String> {
        let devices = board(&hub.current().json).devices;
        devices.into_iter().map(|device| device.id).collect()
    };

    // The kept device needs no new connection and keeps what it reported.
    let added = hub
        .set_devices(vec![endpoint("ssh:new"), endpoint("local")])
        .unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].endpoint.id, "ssh:new");
    assert_eq!(ids(&hub), ["ssh:new", "local"]);
    let latest = board(&hub.current().json);
    assert_eq!(latest.devices[1].status, DeviceStatus::Connected);
    assert!(latest.waiting.is_empty());
    assert!(hub.follows(&first[0]));
    assert!(!hub.follows(&first[1]));

    // A connection left over from before the device was dropped says nothing
    // about the device followed under the same id now.
    let again = hub
        .set_devices(vec![endpoint("local"), endpoint("ssh:box")])
        .unwrap();
    let revision = hub.current().revision;
    connect(&hub, &first[1], "blocked");
    assert_eq!(hub.current().revision, revision);
    connect(&hub, &again[0], "blocked");
    assert_eq!(board(&hub.current().json).waiting.len(), 1);
}

#[test]
fn a_device_carries_its_route() {
    let hub = Hub::starting_at(6).unwrap();
    hub.set_devices(vec![
        endpoint("local"),
        Endpoint {
            route: Route::Through {
                gateway: "laptop".to_owned(),
                target: "box".to_owned(),
                session: "default".to_owned(),
            },
            ..endpoint("ssh:box")
        },
    ])
    .unwrap();
    let devices = board(&hub.current().json).devices;
    assert!(devices[0].route.is_empty());
    assert_eq!(devices[1].route, ["laptop", "box"]);
}
