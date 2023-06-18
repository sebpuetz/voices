#[cfg(feature = "standalone")]
pub use integrated::IntegratedVoiceHost;
#[cfg(feature = "distributed")]
pub use remote::RemoteVoiceHost;
pub use voice_server::{
    channel::connection::ConnectionState, ConnectionData, EstablishSession, OpenConnection, Peer,
    SessionData,
};

#[cfg(feature = "standalone")]
mod integrated;
#[cfg(feature = "distributed")]
mod remote;
#[cfg(feature = "distributed")]
#[path = "voice_server.v1.rs"]
mod voice_server_proto;
use async_trait::async_trait;
use tonic::Status;
use uuid::Uuid;

// FIXME: these arguments / return types should be defined in this crate, Status as the error is a hack
#[async_trait]
pub trait VoiceHost: Send + Sync + 'static {
    async fn open_connection(
        &self,
        request: voice_server::OpenConnection,
    ) -> Result<voice_server::ConnectionData, Status>;
    async fn establish_session(
        &self,
        request: voice_server::EstablishSession,
    ) -> Result<voice_server::SessionData, Status>;
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), Status>;
    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, Status>;
    async fn status(&self, channel_id: Uuid) -> Result<Vec<voice_server::Peer>, Status>;
}
