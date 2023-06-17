#[cfg(feature = "distributed")]
pub mod distributed;
#[cfg(feature = "distributed")]
#[path = "./voice_channels.v1.rs"]
pub mod voice_channels_proto;

// #[cfg(feature = "integrated")]
pub mod integrated;

use async_trait::async_trait;
use uuid::Uuid;

use crate::voice_instance::VoiceHost;

#[async_trait]
pub trait ChannelRegistry: Clone + Send + Sync + 'static {
    type Voice: VoiceHost;
    async fn get_voice_host(
        &self,
        channel_id: Uuid,
        reassign: bool,
    ) -> anyhow::Result<Option<Self::Voice>>;
}
