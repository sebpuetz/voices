#[cfg(feature = "distributed")]
pub mod distributed;
#[cfg(feature = "standalone")]
pub mod local;

use async_trait::async_trait;

use super::{ChatRoomJoined, ClientInfo};

#[async_trait]
pub trait ChannelState: Send + Sync + 'static {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<ChatRoomJoined>;
    async fn leave(&self, info: ClientInfo) -> anyhow::Result<()>;

    async fn list_members(&self) -> anyhow::Result<Vec<ClientInfo>>;
}
