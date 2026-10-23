/// Errors from the decisions repository.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("OPENROUTER_API_KEY is not set")]
    MissingApiKey,
    #[error("decisions request failed: {0}")]
    Transport(String),
    #[error("decisions API returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("could not decode the decisions response: {0}")]
    Decode(String),
    #[error("decisions response is missing a valid `noul` answer for `{0}`")]
    MissingAnswer(String),
    #[error("decisions response has an invalid probability for `{key}`: {value}")]
    InvalidProbability { key: String, value: f64 },
}
