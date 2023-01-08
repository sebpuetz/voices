use tonic::async_trait;
use tonic::transport::Channel;

use crate::grpc::proto;
use crate::grpc::proto::voice_server_client as client;

// FIXME: downsides to reusing server trait?
#[async_trait]
impl proto::voice_server_server::VoiceServer for client::VoiceServerClient<Channel> {
    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::assign_channel(&mut slf, request).await
    }

    async fn open_connection(
        &self,
        request: tonic::Request<proto::OpenConnectionRequest>,
    ) -> Result<tonic::Response<proto::OpenConnectionResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::open_connection(&mut slf, request).await
    }

    async fn establish_session(
        &self,
        request: tonic::Request<proto::EstablishSessionRequest>,
    ) -> Result<tonic::Response<proto::EstablishSessionResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::establish_session(&mut slf, request).await
    }

    async fn leave(
        &self,
        request: tonic::Request<proto::LeaveRequest>,
    ) -> Result<tonic::Response<proto::LeaveResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::leave(&mut slf, request).await
    }

    async fn user_status(
        &self,
        request: tonic::Request<proto::UserStatusRequest>,
    ) -> Result<tonic::Response<proto::UserStatusResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::user_status(&mut slf, request).await
    }

    async fn status(
        &self,
        request: tonic::Request<proto::StatusRequest>,
    ) -> Result<tonic::Response<proto::StatusResponse>, tonic::Status> {
        let mut slf = self.clone();
        client::VoiceServerClient::status(&mut slf, request).await
    }
}
