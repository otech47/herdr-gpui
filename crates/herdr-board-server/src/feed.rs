//! One thread per device, keeping the hub current from its daemon's events.

use crate::{args::Endpoint, hub::Hub};
use herdr_client::{ClientEvent, ConnectOptions, connect_with_surface_active};
use std::{sync::Arc, thread, time::Duration};

const FIRST_RETRY: Duration = Duration::from_secs(1);
/// The Herdr app's own ceiling between attempts.
const LAST_RETRY: Duration = Duration::from_secs(30);

pub(crate) fn follow(hub: &Arc<Hub>, endpoints: &[Endpoint]) {
    for (index, endpoint) in endpoints.iter().enumerate() {
        if !endpoint.enabled {
            continue;
        }
        let (hub, endpoint) = (hub.clone(), endpoint.clone());
        thread::spawn(move || {
            let mut retry = FIRST_RETRY;
            loop {
                if attach(&hub, index, &endpoint) {
                    retry = FIRST_RETRY;
                }
                thread::sleep(retry);
                retry = (retry * 2).min(LAST_RETRY);
            }
        });
    }
}

/// Follow one connection until it ends. Whether it ever connected.
fn attach(hub: &Hub, index: usize, endpoint: &Endpoint) -> bool {
    let report = |change: &dyn Fn(&mut crate::hub::Link)| {
        if let Err(error) = hub.update(index, change) {
            eprintln!("herdr-board-server: {}: {error}", endpoint.id);
        }
    };
    report(&|link| link.connecting = !link.connected);
    let mut connected = false;
    let mut detail = None;
    // An inactive surface: the daemon sends snapshots but never sizes a tab
    // to this client, never makes it the foreground client, and refuses its
    // input. This server must stay invisible to whoever is at the terminal.
    match connect_with_surface_active(endpoint.target.clone(), ConnectOptions::default(), false) {
        Ok(client) => {
            for event in &client.events {
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
            }
        }
        Err(error) => detail = Some(error.to_string()),
    }
    report(&|link| {
        *link = crate::hub::Link {
            detail: detail.clone(),
            ..Default::default()
        };
    });
    connected
}
