use async_trait::async_trait;

use super::proto;

#[async_trait]
impl proto::channels_server::Channels
    for proto::channels_client::ChannelsClient<tonic::transport::Channel>
{
    async fn cleanup_stale_voice_servers(
        &self,
        request: tonic::Request<proto::CleanupStaleVoiceServersRequest>,
    ) -> Result<tonic::Response<proto::CleanupStaleVoiceServersResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::cleanup_stale_voice_servers(&mut slf, request).await
    }

    async fn register_voice_server(
        &self,
        request: tonic::Request<proto::RegisterVoiceServerRequest>,
    ) -> Result<tonic::Response<proto::RegisterVoiceServerResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::register_voice_server(&mut slf, request).await
    }

    async fn get_servers(
        &self,
        request: tonic::Request<proto::GetServersRequest>,
    ) -> Result<tonic::Response<proto::GetServersResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::get_servers(&mut slf, request).await
    }

    async fn create_server(
        &self,
        request: tonic::Request<proto::CreateServerRequest>,
    ) -> Result<tonic::Response<proto::CreateServerResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::create_server(&mut slf, request).await
    }

    async fn get_server(
        &self,
        request: tonic::Request<proto::GetServerRequest>,
    ) -> Result<tonic::Response<proto::GetServerResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::get_server(&mut slf, request).await
    }

    async fn create_channel(
        &self,
        request: tonic::Request<proto::CreateChannelRequest>,
    ) -> Result<tonic::Response<proto::CreateChannelResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::create_channel(&mut slf, request).await
    }

    async fn get_channel(
        &self,
        request: tonic::Request<proto::GetChannelRequest>,
    ) -> Result<tonic::Response<proto::GetChannelResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::get_channel(&mut slf, request).await
    }

    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::assign_channel(&mut slf, request).await
    }
    async fn unassign_channel(
        &self,
        request: tonic::Request<proto::UnassignChannelRequest>,
    ) -> Result<tonic::Response<proto::UnassignChannelResponse>, tonic::Status> {
        let mut slf = self.clone();
        proto::channels_client::ChannelsClient::unassign_channel(&mut slf, request).await
    }
}
