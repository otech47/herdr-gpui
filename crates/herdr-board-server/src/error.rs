use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}\n\n{usage}", usage = crate::args::USAGE)]
    Usage(String),
    #[error("cannot listen on the loopback port: {0}")]
    Listen(#[source] io::Error),
    #[error("cannot read the saved devices: {0}")]
    Devices(#[from] herdr_client::Error),
    #[error(transparent)]
    Board(#[from] herdr_board::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
