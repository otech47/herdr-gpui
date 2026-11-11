//! One thread per device, keeping the hub current from its daemon's events.

use crate::{
    devices::Endpoint,
    hub::{Followed, Hub, Link},
};
use herdr_client::ClientEvent;
use std::{sync::Arc, thread, time::Duration};

const FIRST_RETRY: Duration = Duration::from_secs(1);
/// The Herdr app's own ceiling between attempts.
const LAST_RETRY: Duration = Duration::from_secs(30);
/// How long a connection to a device that is no longer followed can stay open.
const DROPPED_CHECK: Duration = Duration::from_secs(5);

/// Follow `endpoints` and nothing else. A device already followed keeps its
/// connection.
pub(crate) fn follow(hub: &Arc<Hub>, endpoints: Vec<Endpoint>) {
    let added = match hub.set_devices(endpoints) {
        Ok(added) => added,
        Err(error) => {
            eprintln!("herdr-board-server: devices not followed: {error}");
            return;
        }
    };
    for device in added {
        if !device.endpoint.enabled {
            continue;
        }
        let hub = hub.clone();
        thread::spawn(move || {
            let mut retry = FIRST_RETRY;
            while hub.follows(&device) {
                if attach(&hub, &device) {
                    retry = FIRST_RETRY;
                }
                thread::sleep(retry);
                retry = (retry * 2).min(LAST_RETRY);
            }
        });
    }
}

/// Follow one connection until it ends or its device is dropped. Whether it
/// ever connected.
fn attach(hub: &Hub, device: &Followed) -> bool {
    let report = |change: &dyn Fn(&mut Link)| {
        if let Err(error) = hub.update(device, change) {
            eprintln!("herdr-board-server: {}: {error}", device.endpoint.id);
        }
    };
    report(&|link| link.connecting = !link.connected);
    let mut connected = false;
    let mut detail = None;
    match device.endpoint.route.connect() {
        Ok(client) => loop {
            let event = match client.events.recv_timeout(DROPPED_CHECK) {
                Ok(event) => event,
                Err(error) if error.is_timeout() && hub.follows(device) => continue,
                Err(_) => break,
            };
            match event {
                ClientEvent::Connected(_) => {
                    connected = true;
                    report(&|link| {
                        link.connected = true;
                        link.connecting = false;
                        link.detail = None;
                    });
                }
                ClientEvent::Snapshot(snapshot) => {
                    report(&|link| link.snapshot = Some(snapshot.clone()));
                }
                ClientEvent::Disconnected { reason, .. } => {
                    detail = Some(reason);
                    break;
                }
                _ => {}
            }
        },
        Err(error) => detail = Some(error.to_string()),
    }
    report(&|link| {
        *link = Link {
            detail: detail.clone(),
            ..Default::default()
        };
    });
    connected
}
