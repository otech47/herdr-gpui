//! The devices to follow: this machine, those named on the command line, and
//! those saved in the Herdr app here and on each named device.

use crate::{args::Args, feed, hub::Hub};
use herdr_client::{Client, ConnectOptions, ConnectTarget, Destination, SavedHost};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    net::{IpAddr, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

const DEFAULT_SESSION: &str = "default";
/// How long a device saved in a Herdr app can take to be followed here.
const REFRESH: Duration = Duration::from_secs(60);
const KEPT_VERSION: u32 = 1;

/// One daemon to follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) route: Route,
}

/// How this machine reaches a daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Local,
    Ssh {
        target: String,
        session: String,
    },
    /// `gateway` runs the SSH client that reaches `target`.
    Through {
        gateway: String,
        target: String,
        session: String,
    },
}

impl Route {
    /// The SSH targets on the way, the first one from this machine.
    pub(crate) fn hops(&self) -> Vec<String> {
        match self {
            Self::Local => Vec::new(),
            Self::Ssh { target, .. } => vec![target.clone()],
            Self::Through {
                gateway, target, ..
            } => vec![gateway.clone(), target.clone()],
        }
    }

    /// Attach with an inactive surface: the daemon sends snapshots but never
    /// sizes a tab to this client, never makes it the foreground client, and
    /// refuses its input. This server must stay invisible to whoever is at
    /// the terminal.
    pub(crate) fn connect(&self) -> herdr_client::Result<Client> {
        let options = ConnectOptions::default();
        match self {
            Self::Local => {
                herdr_client::connect_with_surface_active(ConnectTarget::Local, options, false)
            }
            Self::Ssh { target, session } => herdr_client::connect_with_surface_active(
                ConnectTarget::Ssh {
                    target: target.clone(),
                    session: session.clone(),
                },
                options,
                false,
            ),
            Self::Through {
                gateway,
                target,
                session,
            } => herdr_client::connect_through(gateway, target, session, options, false),
        }
    }
}

/// The devices named on the command line, this machine first.
pub(crate) fn named(args: &Args) -> Vec<Endpoint> {
    let mut endpoints = vec![Endpoint {
        id: "local".to_owned(),
        label: args.label.clone(),
        enabled: true,
        route: Route::Local,
    }];
    for (label, target) in &args.ssh {
        let id = format!("ssh:{target}");
        if endpoints.iter().all(|endpoint| endpoint.id != id) {
            endpoints.push(Endpoint {
                id,
                label: label.clone(),
                enabled: true,
                route: Route::Ssh {
                    target: target.clone(),
                    session: DEFAULT_SESSION.to_owned(),
                },
            });
        }
    }
    endpoints
}

/// A device saved in a Herdr app, and where the SSH configuration of that
/// app's machine takes its target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) target: String,
    pub(crate) session: String,
    pub(crate) enabled: bool,
    pub(crate) user: String,
    pub(crate) host: String,
    pub(crate) port: u16,
}

/// The saved devices of each named device, keyed by its id.
pub(crate) type SavedByOwner = BTreeMap<String, Vec<Saved>>;

/// What this machine knows of the network.
pub(crate) trait Network {
    /// Where this machine's SSH configuration takes `target`.
    fn destination(&self, target: &str) -> Option<Destination>;
    /// The addresses `host` names here. Empty when it names none.
    fn addresses(&self, host: &str) -> BTreeSet<IpAddr>;
    /// Whether `address` is one of this machine's own.
    fn here(&self, address: IpAddr) -> bool;
}

struct System;

impl Network for System {
    fn destination(&self, target: &str) -> Option<Destination> {
        herdr_client::resolve_destination(target).ok()
    }

    fn addresses(&self, host: &str) -> BTreeSet<IpAddr> {
        (host, 0)
            .to_socket_addrs()
            .map(|found| found.map(|address| address.ip()).collect())
            .unwrap_or_default()
    }

    fn here(&self, address: IpAddr) -> bool {
        address.is_loopback() || UdpSocket::bind((address, 0)).is_ok()
    }
}

/// Where a daemon is, to tell two names for one daemon apart.
struct Place {
    host: String,
    addresses: BTreeSet<IpAddr>,
    port: u16,
    session: String,
}

impl Place {
    fn of(network: &impl Network, host: &str, port: u16, session: &str) -> Self {
        Self {
            host: host.to_owned(),
            addresses: network.addresses(host),
            port,
            session: session.to_owned(),
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.port == other.port
            && self.session == other.session
            && (self.host == other.host || !self.addresses.is_disjoint(&other.addresses))
    }
}

/// `named`, then every saved device that is none of them, each with the route
/// this machine has to it.
pub(crate) fn all(
    network: &impl Network,
    named: &[Endpoint],
    saved: &SavedByOwner,
) -> Vec<Endpoint> {
    let mut endpoints = named.to_vec();
    let mut places: Vec<Place> = named
        .iter()
        .filter_map(|endpoint| match &endpoint.route {
            Route::Ssh { target, session } => {
                let destination = network.destination(target)?;
                Some(Place::of(
                    network,
                    &destination.host,
                    destination.port,
                    session,
                ))
            }
            Route::Local | Route::Through { .. } => None,
        })
        .collect();
    for owner in named {
        for device in saved.get(&owner.id).into_iter().flatten() {
            let place = Place::of(network, &device.host, device.port, &device.session);
            let here = device.session == DEFAULT_SESSION
                && place.addresses.iter().any(|address| network.here(*address));
            let id = format!("ssh:{}", device.id);
            if here
                || places.iter().any(|known| known.same(&place))
                || endpoints.iter().any(|endpoint| endpoint.id == id)
            {
                continue;
            }
            endpoints.push(Endpoint {
                id,
                label: device.label.clone(),
                enabled: device.enabled,
                route: route(network, owner, device, &place),
            });
            places.push(place);
        }
    }
    endpoints
}

fn route(network: &impl Network, owner: &Endpoint, device: &Saved, place: &Place) -> Route {
    let (target, session) = (device.target.clone(), device.session.clone());
    let Route::Ssh {
        target: gateway, ..
    } = &owner.route
    else {
        return Route::Ssh { target, session };
    };
    // The owner's name for the device can mean another machine here, or none.
    let direct = network
        .destination(&device.target)
        .is_some_and(|destination| {
            destination.user == device.user
                && Place::of(
                    network,
                    &destination.host,
                    destination.port,
                    &device.session,
                )
                .same(place)
        });
    if direct {
        Route::Ssh { target, session }
    } else {
        Route::Through {
            gateway: gateway.clone(),
            target,
            session,
        }
    }
}

/// What the Herdr app on one device has saved.
#[derive(Clone, Default)]
struct Answer {
    hosts: Vec<SavedHost>,
    saved: Vec<Saved>,
}

/// Ask `owner` for its saved devices. A target is resolved again only when
/// the saved list differs from `last`, so an edit to the owner's SSH
/// configuration alone is not seen until then.
fn ask(owner: &Endpoint, last: Option<&Answer>) -> herdr_client::Result<Answer> {
    let gateway = match &owner.route {
        Route::Local => None,
        Route::Ssh { target, .. } => Some(target.as_str()),
        Route::Through { .. } => return Ok(Answer::default()),
    };
    let hosts = match gateway {
        Some(gateway) => herdr_client::remote_saved_hosts(gateway)?,
        None => herdr_client::load_saved_hosts(false)?,
    };
    if let Some(last) = last
        && last.hosts == hosts
    {
        return Ok(last.clone());
    }
    let mut saved = Vec::with_capacity(hosts.len());
    for host in &hosts {
        let destination = match gateway {
            Some(gateway) => herdr_client::resolve_destination_from(gateway, &host.target)?,
            None => herdr_client::resolve_destination(&host.target)?,
        };
        saved.push(Saved {
            id: host.id.clone(),
            label: host.label.clone(),
            target: host.target.clone(),
            session: host.session.clone(),
            enabled: host.enabled,
            user: destination.user,
            host: destination.host,
            port: destination.port,
        });
    }
    Ok(Answer { hosts, saved })
}

/// The file that keeps the saved devices between runs.
#[derive(Serialize, Deserialize)]
struct Kept {
    version: u32,
    saved: SavedByOwner,
}

fn kept_path() -> Option<PathBuf> {
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(env::var_os("HOME")?).join(".local/state")))?;
    Some(state.join("herdr/board-server/saved-devices.json"))
}

/// The saved devices an earlier run kept. Empty when it kept none.
pub(crate) fn load(path: &Path) -> SavedByOwner {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Kept>(&bytes).ok())
        .filter(|kept| kept.version == KEPT_VERSION)
        .map(|kept| kept.saved)
        .unwrap_or_default()
}

pub(crate) fn store(path: &Path, saved: &SavedByOwner) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let kept = Kept {
        version: KEPT_VERSION,
        saved: saved.clone(),
    };
    // A write cut short leaves the earlier file whole.
    let partial = path.with_extension("json.partial");
    fs::write(&partial, serde_json::to_vec(&kept)?)?;
    fs::rename(partial, path)
}

/// Follow the saved devices as well as `named`: first those an earlier run
/// kept, so an owner that cannot be asked still has its devices followed,
/// then whatever each owner says every [`REFRESH`].
pub(crate) fn watch(hub: Arc<Hub>, named: Vec<Endpoint>) {
    thread::spawn(move || {
        let path = kept_path();
        let mut saved = path.as_deref().map(load).unwrap_or_default();
        let mut answers: BTreeMap<String, Answer> = BTreeMap::new();
        let mut unread = BTreeSet::new();
        feed::follow(&hub, all(&System, &named, &saved));
        loop {
            let mut changed = false;
            for owner in &named {
                match ask(owner, answers.get(&owner.id)) {
                    Ok(answer) => {
                        if unread.remove(&owner.id) {
                            eprintln!("herdr-board-server: {}: saved devices read", owner.id);
                        }
                        changed |= saved.get(&owner.id) != Some(&answer.saved);
                        saved.insert(owner.id.clone(), answer.saved.clone());
                        answers.insert(owner.id.clone(), answer);
                    }
                    Err(error) => {
                        if unread.insert(owner.id.clone()) {
                            eprintln!(
                                "herdr-board-server: {}: saved devices not read, keeping the last ones: {error}",
                                owner.id
                            );
                        }
                    }
                }
            }
            if changed
                && let Some(path) = &path
                && let Err(error) = store(path, &saved)
            {
                eprintln!("herdr-board-server: saved devices not kept: {error}");
            }
            // Every time, because a name that did not resolve at boot can
            // resolve now and change which devices are this machine.
            feed::follow(&hub, all(&System, &named, &saved));
            thread::sleep(REFRESH);
        }
    });
}
