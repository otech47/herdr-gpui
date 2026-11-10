//! Serves the session board of one or more Herdr daemons as JSON on a
//! loopback port. The contract is in the `herdr-board` crate's README.

mod args;
mod error;
mod feed;
mod http;
mod hub;
#[cfg(test)]
mod tests;

pub use error::{Error, Result};

use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("herdr-board-server: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = args::Args::parse(std::env::args().skip(1))?;
    let saved = if args.saved {
        herdr_client::load_saved_hosts(false)?
    } else {
        Vec::new()
    };
    let endpoints = args.endpoints(saved);
    let hub = Arc::new(hub::Hub::new(endpoints.clone())?);
    let listener = http::listen(args.port)?;
    if let Ok(address) = listener.local_addr() {
        eprintln!("herdr-board-server: serving http://{address}/board");
    }
    feed::follow(&hub, &endpoints);
    http::serve(&listener, &hub);
    Ok(())
}
