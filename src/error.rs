use std::io;

use thiserror::Error;

/// All the possible errors returned by the library
#[derive(Debug, Error)]
pub enum TrainError {
    /// URL parse Error
    #[error("URL could not be parsed: {0}")]
    UrlParseError(#[from] url::ParseError),

    /// Output filename from URL parse Error
    #[error("filename from the URL could not be parsed")]
    FilenameParseError,

    /// File creation Error
    #[error("file creation failed: {0}")]
    FileCreateError(io::Error),

    /// File seek Error
    #[error("file seeking failed: {0}")]
    FileSeekError(io::Error),

    /// File write Error
    #[error("file write failed: {0}")]
    FileWriteError(io::Error),

    /// Buffer write Error
    #[error("buffer write failed: {0}")]
    BufferWriteError(io::Error),

    /// File sync with filesystem Error
    #[error("file could not be synced with the filesystem")]
    FileSyncError(io::Error),

    /// Request Error
    #[error("failed to execute the request: {0}")]
    RequestError(#[from] ureq::Error),
}

pub type Result<T> = core::result::Result<T, TrainError>;
