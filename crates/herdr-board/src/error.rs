pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the board could not be encoded as JSON")]
    Encode(#[from] serde_json::Error),
}
