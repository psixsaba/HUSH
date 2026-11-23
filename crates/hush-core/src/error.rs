/// Recoverable failure from the audio core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Wav(#[from] hound::Error),
    #[error(transparent)]
    Device(#[from] cpal::BuildStreamError),
    #[error(transparent)]
    DeviceConfig(#[from] cpal::DefaultStreamConfigError),
    #[error(transparent)]
    Play(#[from] cpal::PlayStreamError),
    #[error(transparent)]
    Supported(#[from] cpal::SupportedStreamConfigsError),
}

impl Error {
    pub fn msg(msg: impl Into<String>) -> Self {
        Error::Message(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

