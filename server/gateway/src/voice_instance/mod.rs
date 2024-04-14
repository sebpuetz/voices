#[cfg(feature = "standalone")]
pub use integrated::IntegratedVoiceHost;
#[cfg(feature = "distributed")]
pub use remote::RemoteVoiceHost;

pub use voices_voice_models::{
    ConnectionData, ConnectionState, EstablishSession, OpenConnection, Peer, SessionData,
};

#[cfg(feature = "standalone")]
mod integrated;
#[cfg(feature = "distributed")]
mod remote;
#[cfg(feature = "distributed")]
#[path = "voice_server.v1.rs"]
mod voice_server_proto;
use async_trait::async_trait;
use uuid::Uuid;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait VoiceHost: Send + Sync + 'static {
    async fn open_connection(
        &self,
        request: OpenConnection,
    ) -> Result<ConnectionData, VoiceHostError>;
    async fn establish_session(
        &self,
        request: EstablishSession,
    ) -> Result<SessionData, VoiceHostError>;
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), VoiceHostError>;
    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, VoiceHostError>;
    async fn status(&self, channel_id: Uuid) -> Result<Vec<Peer>, VoiceHostError>;
}

#[derive(thiserror::Error, Debug)]
pub enum VoiceHostError {
    // FIXME: the string is ugly, probably should be variants...
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
