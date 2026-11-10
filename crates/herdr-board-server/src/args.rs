//! The command line: the port to serve on and the devices to follow.

use crate::{Error, Result};
use herdr_client::{ConnectTarget, SavedHost};

pub(crate) const USAGE: &str = "\
usage: herdr-board-server --port PORT [--label NAME] [--saved-devices] [--ssh LABEL=TARGET]...

  --port PORT         loopback port to serve on; 0 picks a free one
  --label NAME        what to call this machine's own daemon (default: Local)
  --saved-devices     also follow the ssh devices saved in the Herdr app
  --ssh LABEL=TARGET  also follow the daemon on an ssh target; repeatable";

/// One daemon to follow.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Endpoint {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) target: ConnectTarget,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Args {
    pub(crate) port: u16,
    pub(crate) label: String,
    pub(crate) saved: bool,
    pub(crate) ssh: Vec<(String, String)>,
}

impl Args {
    pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut port = None;
        let mut label = "Local".to_owned();
        let mut saved = false;
        let mut ssh = Vec::new();
        while let Some(flag) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| Error::Usage(format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--port" => {
                    let text = value()?;
                    port = Some(
                        text.parse()
                            .map_err(|_| Error::Usage(format!("{text} is not a port")))?,
                    );
                }
                "--label" => label = value()?,
                "--saved-devices" => saved = true,
                "--ssh" => {
                    let text = value()?;
                    match text.split_once('=') {
                        Some((label, target)) if !label.is_empty() && !target.is_empty() => {
                            ssh.push((label.to_owned(), target.to_owned()));
                        }
                        _ => return Err(Error::Usage(format!("{text} is not LABEL=TARGET"))),
                    }
                }
                _ => return Err(Error::Usage(format!("unknown argument {flag}"))),
            }
        }
        Ok(Self {
            port: port.ok_or_else(|| Error::Usage("--port is required".to_owned()))?,
            label,
            saved,
            ssh,
        })
    }

    /// The devices in board order: this machine first, then the saved ones,
    /// then those named on the command line. Ids match the Herdr app's.
    pub(crate) fn endpoints(&self, saved: Vec<SavedHost>) -> Vec<Endpoint> {
        let mut endpoints = vec![Endpoint {
            id: "local".to_owned(),
            label: self.label.clone(),
            enabled: true,
            target: ConnectTarget::Local,
        }];
        endpoints.extend(saved.into_iter().map(|host| Endpoint {
            id: format!("ssh:{}", host.id),
            label: host.label,
            enabled: host.enabled,
            target: ConnectTarget::Ssh {
                target: host.target,
                session: host.session,
            },
        }));
        for (label, target) in &self.ssh {
            let id = format!("ssh:{target}");
            if endpoints.iter().all(|endpoint| endpoint.id != id) {
                endpoints.push(Endpoint {
                    id,
                    label: label.clone(),
                    enabled: true,
                    target: ConnectTarget::Ssh {
                        target: target.clone(),
                        session: "default".to_owned(),
                    },
                });
            }
        }
        endpoints
    }
}
