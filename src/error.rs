use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("missing second lens video: {0}")]
    MissingLens(String),

    #[error("mismatched recording: {0}")]
    MismatchedRecording(String),

    #[error("missing IMU telemetry: {0}")]
    MissingImu(String),

    #[error("unsupported telemetry format: {0}")]
    UnsupportedTelemetry(String),

    #[error("stream synchronization failure: {0}")]
    Sync(String),

    #[error("video decode failure: {0}")]
    Decode(String),

    #[error("seek failure at {timestamp_us}us: {msg}")]
    Seek { timestamp_us: i64, msg: String },

    #[error("output write failure: {0}")]
    Output(String),

    #[error("telemetry parse error: {0}")]
    Telemetry(String),

    #[error("ffmpeg error: {0}")]
    Ffmpeg(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl From<ffmpeg_next::Error> for AppError {
    fn from(e: ffmpeg_next::Error) -> Self {
        AppError::Ffmpeg(e.to_string())
    }
}
