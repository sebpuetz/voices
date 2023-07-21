use std::sync::{Arc, Weak};

use async_trait::async_trait;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::server::channels::{
    ChannelEvent, ChannelInit, ChannelMap, ChatRoomJoined, ClientInfo, LocallyPresent,
};

use super::ChannelState;

/// Channel State implementation for Standalone Setup
///
/// Broadcasts channel events to all subscribed sessions.
#[derive(Clone, Debug)]
pub struct LocalChannelEvents {
    channel_id: Uuid,
    tx: broadcast::Sender<ChannelEvent>,
    locally_present: Arc<RwLock<LocallyPresent>>,
    channels: Weak<ChannelMap>,
}

#[derive(Clone)]
pub struct LocalChannelInitializer;

#[async_trait]
impl ChannelInit for LocalChannelInitializer {
    type ChannelState = LocalChannelEvents;
    async fn init(
        &self,
        channel_id: Uuid,
        channels: Weak<ChannelMap>,
    ) -> anyhow::Result<LocalChannelEvents> {
        Ok(LocalChannelEvents::new(
            channel_id,
            broadcast::channel(100).0,
            channels,
        ))
    }
}

impl LocalChannelEvents {
    pub fn new(
        channel_id: Uuid,
        tx: broadcast::Sender<ChannelEvent>,
        channels: Weak<ChannelMap>,
    ) -> Self {
        Self {
            channel_id,
            tx,
            locally_present: Arc::default(),
            channels,
        }
    }
}

// multiple standalone instances don't propagate channel events and clients don't hear eachother
#[async_trait]
impl ChannelState for LocalChannelEvents {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<ChatRoomJoined> {
        let mut guard = self.locally_present.write().await;
        if !guard.join(info.clone()) {
            anyhow::bail!("local channel was closed");
        }
        drop(guard);
        let (joined, stopped) = ChatRoomJoined::new(self.tx.subscribe(), info.clone());
        let event = ChannelEvent::joined(info.client_id, info.name.clone(), info.source_id);
        self.tx.send(event)?;
        let slf = self.clone();
        tokio::spawn(async move {
            let _ = stopped.await;
            let _ = slf.leave(info).await;
        });

        Ok(joined)
    }

    async fn leave(&self, info: ClientInfo) -> anyhow::Result<()> {
        let mut guard = self.locally_present.write().await;
        let empty = guard.leave(info.client_id);
        if empty {
            if let Some(map) = self.channels.upgrade() {
                map.close(self.channel_id).await;
            }
        }
        let event = ChannelEvent::left(info.client_id, info.name, info.source_id);
        self.tx.send(event)?;
        Ok(())
    }

    async fn list_members(&self) -> anyhow::Result<Vec<ClientInfo>> {
        Ok(self.locally_present.read().await.list().await)
    }
}
