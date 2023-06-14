#[path = "voice_server.v1.rs"]
pub mod voice_server_proto;
use async_trait::async_trait;
use uuid::Uuid;
use voice_server::channel::connection::ConnectionState;

// pub struct VoiceHost {}

// impl VoiceHost {
//     pub async fn open_connection() {}
// }

#[async_trait]
pub trait VoiceHost: Send + Sync + 'static {
    async fn open_connection(
        &self,
        request: voice_server::OpenConnection,
    ) -> Result<voice_server::ConnectionData, tonic::Status>;
    async fn establish_session(
        &self,
        request: voice_server::EstablishSession,
    ) -> Result<voice_server::SessionData, tonic::Status>;
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), tonic::Status>;
    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, tonic::Status>;
    async fn status(&self, channel_id: Uuid) -> Result<Vec<voice_server::Peer>, tonic::Status>;
}

#[async_trait]
impl VoiceHost for voice_server::VoiceServerImpl {
    async fn open_connection(
        &self,
        request: voice_server::OpenConnection,
    ) -> Result<voice_server::ConnectionData, tonic::Status> {
        todo!()
    }

    async fn establish_session(
        &self,
        request: voice_server::EstablishSession,
    ) -> Result<voice_server::SessionData, tonic::Status> {
        todo!()
    }
    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), tonic::Status> {
        todo!()
    }
    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, tonic::Status> {
        todo!()
    }
    async fn status(&self, channel_id: Uuid) -> Result<Vec<voice_server::Peer>, tonic::Status> {
        todo!()
    }
}
