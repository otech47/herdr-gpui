use super::*;
use crate::devices::{Network, Saved, SavedByOwner, all, load, store};
use herdr_client::Destination;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, Ipv4Addr},
};

const HERE: IpAddr = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1));
const LAPTOP: IpAddr = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 2));
const STUDIO: IpAddr = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 3));

/// This machine's SSH configuration and name service.
#[derive(Default)]
struct Fake {
    destinations: BTreeMap<&'static str, Destination>,
    addresses: BTreeMap<&'static str, IpAddr>,
}

impl Network for Fake {
    fn destination(&self, target: &str) -> Option<Destination> {
        self.destinations.get(target).cloned()
    }

    fn addresses(&self, host: &str) -> BTreeSet<IpAddr> {
        let literal = host.parse().ok();
        let named = self.addresses.get(host).copied();
        literal.into_iter().chain(named).collect()
    }

    fn here(&self, address: IpAddr) -> bool {
        address == HERE
    }
}

fn destination(user: &str, host: &str) -> Destination {
    Destination {
        user: user.to_owned(),
        host: host.to_owned(),
        port: 22,
    }
}

/// A network where the laptop is named as it is on the command line.
fn network() -> Fake {
    Fake {
        destinations: BTreeMap::from([("laptop", destination("me", "laptop.net"))]),
        addresses: BTreeMap::from([("laptop.net", LAPTOP), ("mini.net", HERE)]),
    }
}

fn named() -> Vec<Endpoint> {
    let args = Args {
        port: 1,
        label: "Mini".to_owned(),
        ssh: vec![("Laptop".to_owned(), "laptop".to_owned())],
    };
    crate::devices::named(&args)
}

fn saved(id: &str, target: &str, user: &str, host: &str) -> Saved {
    Saved {
        id: id.to_owned(),
        label: id.to_uppercase(),
        target: target.to_owned(),
        session: "default".to_owned(),
        enabled: true,
        user: user.to_owned(),
        host: host.to_owned(),
        port: 22,
    }
}

fn by_owner(owner: &str, devices: Vec<Saved>) -> SavedByOwner {
    BTreeMap::from([(owner.to_owned(), devices)])
}

/// The id and route of each device after the named ones.
fn found(network: &Fake, saved: &SavedByOwner) -> Vec<(String, Route)> {
    let endpoints = all(network, &named(), saved);
    assert_eq!(endpoints[..2], named());
    endpoints[2..]
        .iter()
        .map(|endpoint| (endpoint.id.clone(), endpoint.route.clone()))
        .collect()
}

fn ssh(target: &str) -> Route {
    Route::Ssh {
        target: target.to_owned(),
        session: "default".to_owned(),
    }
}

fn through(target: &str) -> Route {
    Route::Through {
        gateway: "laptop".to_owned(),
        target: target.to_owned(),
        session: "default".to_owned(),
    }
}

#[test]
fn a_device_saved_here_is_reached_directly() {
    let saved = by_owner("local", vec![saved("studio", "studio", "me", "10.0.0.3")]);
    assert_eq!(
        found(&network(), &saved),
        [("ssh:studio".to_owned(), ssh("studio"))]
    );
}

#[test]
fn a_device_saved_elsewhere_is_reached_directly_only_under_the_same_name() {
    let mut network = network();
    network
        .destinations
        .insert("studio", destination("me", "studio.net"));
    network.addresses.insert("studio.net", STUDIO);
    // This machine's `box` is another machine, and it has no `cloud` at all.
    network
        .destinations
        .insert("box", destination("me", "10.0.0.9"));
    let saved = by_owner(
        "ssh:laptop",
        vec![
            saved("studio", "studio", "me", &STUDIO.to_string()),
            saved("box", "box", "me", "10.0.0.4"),
            saved("cloud", "cloud", "team", "203.0.113.5"),
        ],
    );
    assert_eq!(
        found(&network, &saved),
        [
            ("ssh:studio".to_owned(), ssh("studio")),
            ("ssh:box".to_owned(), through("box")),
            ("ssh:cloud".to_owned(), through("cloud")),
        ]
    );
}

#[test]
fn a_user_this_machine_would_not_log_in_as_goes_through_the_owner() {
    let mut network = network();
    network
        .destinations
        .insert("cloud", destination("me", "203.0.113.5"));
    let saved = by_owner(
        "ssh:laptop",
        vec![saved("cloud", "cloud", "team", "203.0.113.5")],
    );
    assert_eq!(
        found(&network, &saved),
        [("ssh:cloud".to_owned(), through("cloud"))]
    );
}

#[test]
fn a_daemon_already_followed_is_not_followed_again() {
    let mut saved = by_owner(
        "ssh:laptop",
        vec![
            saved("mini", "mini", "me", "mini.net"),
            saved("self", "localhost-alias", "me", &LAPTOP.to_string()),
            saved("studio", "studio", "me", "10.0.0.3"),
        ],
    );
    saved.insert(
        "local".to_owned(),
        vec![self::saved("studio", "studio", "me", "10.0.0.3")],
    );
    assert_eq!(
        found(&network(), &saved),
        [("ssh:studio".to_owned(), ssh("studio"))]
    );
}

#[test]
fn another_session_of_a_followed_machine_is_its_own_device() {
    let work = Saved {
        session: "work".to_owned(),
        ..saved("mini-work", "mini", "me", "mini.net")
    };
    let saved = by_owner("ssh:laptop", vec![work]);
    assert_eq!(
        found(&network(), &saved),
        [(
            "ssh:mini-work".to_owned(),
            Route::Through {
                gateway: "laptop".to_owned(),
                target: "mini".to_owned(),
                session: "work".to_owned(),
            }
        )]
    );
}

#[test]
fn the_saved_devices_outlast_a_run() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("state/saved-devices.json");
    assert!(load(&path).is_empty());

    let saved = by_owner(
        "ssh:laptop",
        vec![saved("cloud", "cloud", "team", "203.0.113.5")],
    );
    store(&path, &saved).unwrap();
    assert_eq!(load(&path), saved);

    std::fs::write(&path, r#"{"version":2,"saved":{"ssh:laptop":[]}}"#).unwrap();
    assert!(load(&path).is_empty());
    std::fs::write(&path, "{").unwrap();
    assert!(load(&path).is_empty());
}
