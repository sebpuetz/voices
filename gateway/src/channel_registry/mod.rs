#[cfg(feature = "distributed")]
pub mod distributed;
#[cfg(feature = "distributed")]
#[path = "./voice_channels.v1.rs"]
pub mod voice_channels_proto;

#[cfg(feature = "standalone")]
pub mod integrated;

use async_trait::async_trait;
use uuid::Uuid;

use crate::voice_instance::VoiceHost;

#[cfg_attr(test, mockall::automock(type Voice=crate::voice_instance::MockVoiceHost;))]
#[async_trait]
pub trait GetVoiceHost: Send + Sync + 'static {
    type Voice: VoiceHost;
    async fn get_voice_host_for(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>>;
}

#[async_trait]
pub trait ChannelRegistry {
    async fn create_server(&self, name: String) -> anyhow::Result<Uuid>;
    async fn get_server(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::ServerWithChannels>>;
    // FIXME: should be some proper error enum instead of result<option<>>
    async fn create_channel(&self, server_id: Uuid, name: String) -> anyhow::Result<Option<Uuid>>;
    async fn get_channel(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Option<voices_channels_models::Channel>>;
    async fn get_servers(
        &self,
        page: Option<i64>,
        per_page: Option<i64>,
    ) -> anyhow::Result<(Vec<voices_channels_models::Server>, i64)>;
    async fn cleanup_stale_voice_servers(&self) -> anyhow::Result<Vec<Uuid>>;
}

#[cfg(test)]
pub fn happy_mocked_get_voice_host<F>(voice_host: F) -> MockGetVoiceHost
where
    F: Send + Sync + Fn() -> crate::voice_instance::MockVoiceHost + 'static,
{
    let mut mock_registry = MockGetVoiceHost::new();
    mock_registry
        .expect_get_voice_host_for()
        .returning(move |_id, _reassign| Ok(Some((voice_host)())));
    mock_registry
}
