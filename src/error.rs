use thiserror::Error;

#[derive(Debug, Error)]
pub enum HelperError {
    #[error("native messaging frame is too large: {actual} bytes (limit {limit})")]
    FrameTooLarge { actual: usize, limit: usize },

    #[error(
        "native messaging frame is truncated (expected {expected} bytes, received {received})"
    )]
    TruncatedFrame { expected: usize, received: usize },

    #[error("native messaging frame contains invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("invalid JSON message: {0}")]
    Json(#[from] serde_json::Error),

    #[error("token JSON is invalid: {0}")]
    InvalidToken(String),

    #[error("a token is required before requesting playback")]
    MissingToken,

    #[error("network error: {0}")]
    Network(String),

    #[error("protobuf/gRPC error: {0}")]
    Protobuf(String),

    #[error("playback response error: {0}")]
    PlaybackResponse(String),

    #[error("QR login error: {0}")]
    Qr(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
