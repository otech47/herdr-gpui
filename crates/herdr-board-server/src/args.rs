//! The command line: the port to serve on and the devices named on it.

use crate::{Error, Result};

pub(crate) const USAGE: &str = "\
usage: herdr-board-server --port PORT [--label NAME] [--ssh LABEL=TARGET]...

  --port PORT         loopback port to serve on; 0 picks a free one
  --label NAME        what to call this machine's own daemon (default: Local)
  --ssh LABEL=TARGET  also follow the daemon on an ssh target; repeatable";

#[derive(Debug, PartialEq)]
pub(crate) struct Args {
    pub(crate) port: u16,
    pub(crate) label: String,
    pub(crate) ssh: Vec<(String, String)>,
}

impl Args {
    pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut args = args.into_iter();
        let mut port = None;
        let mut label = "Local".to_owned();
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
            ssh,
        })
    }
}
