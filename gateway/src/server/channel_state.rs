// publish: join / leave
// unsubscribe on last
use async_trait::async_trait;
use futures_util::StreamExt;
use redis::AsyncCommands;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::channel_registry::{DistributedChannelRegistry, LocalChannelRegistry};
use super::channels::{ChannelEvent, Channels, ClientInfo, LocallyPresent};

#[async_trait]
pub trait ChannelState: Clone + Send + Sync + 'static {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<()>;
    async fn leave(&self, info: ClientInfo) -> anyhow::Result<()>;
    async fn subscribe(&self) -> anyhow::Result<broadcast::Receiver<ChannelEvent>>;

    async fn locally_present(&self) -> anyhow::Result<Vec<ClientInfo>>;
}

/// Channel State implementation for Standalone Setup
///
/// Broadcasts channel events to all subscribed sessions.
#[derive(Clone)]
pub struct LocalChannelEvents {
    tx: broadcast::Sender<ChannelEvent>,
    locally_present: LocallyPresent<LocalChannelEvents, LocalChannelRegistry>,
}

impl LocalChannelEvents {
    pub fn new(
        channel_id: Uuid,
        tx: broadcast::Sender<ChannelEvent>,
        channels: Channels<Self, LocalChannelRegistry>,
    ) -> Self {
        let locally_present = LocallyPresent::new(channel_id, channels);
        Self {
            tx,
            locally_present,
        }
    }
}

#[async_trait]
impl ChannelState for LocalChannelEvents {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<()> {
        self.locally_present.join(info.clone()).await;
        let event = ChannelEvent::joined(info.client_id, info.name, info.source_id);
        self.tx.send(event)?;
        Ok(())
    }

    async fn leave(&self, info: ClientInfo) -> anyhow::Result<()> {
        self.locally_present.leave(info.client_id).await;
        let event = ChannelEvent::left(info.client_id, info.name, info.source_id);
        self.tx.send(event)?;
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<broadcast::Receiver<ChannelEvent>> {
        Ok(self.tx.subscribe())
    }

    async fn locally_present(&self) -> anyhow::Result<Vec<ClientInfo>> {
        Ok(self.locally_present.list().await)
    }
}

/// Channel event implementation broadcasting over Redis PubSub
#[derive(Clone)]
pub struct RedisChannelEvents {
    channel_id: Uuid,
    broadcast_tx: broadcast::Sender<ChannelEvent>,
    pool: deadpool_redis::Pool,
    locally_present: LocallyPresent<RedisChannelEvents, DistributedChannelRegistry>,
}

#[async_trait]
impl ChannelState for RedisChannelEvents {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<()> {
        self.locally_present.join(info.clone()).await;
        // let info_json = serde_json::to_string(&info)?;
        let event = ChannelEvent::joined(info.client_id, info.name, info.source_id);
        let mut conn = self.pool.get().await?;
        self.send(event, &mut conn).await?;
        // conn.hset::<_, _, _, ()>(
        //     &self.chan_members_key(),
        //     info.client_id.to_string(),
        //     info_json,
        // )
        // .await?;
        Ok(())
    }

    async fn leave(&self, info: ClientInfo) -> anyhow::Result<()> {
        self.locally_present.leave(info.client_id).await;
        let event = ChannelEvent::left(info.client_id, info.name, info.source_id);
        let mut conn = self.pool.get().await?;
        self.send(event, &mut conn).await?;
        // conn.hdel::<_, _, ()>(&self.chan_members_key(), info.client_id.to_string())
        //     .await?;
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<broadcast::Receiver<ChannelEvent>> {
        Ok(self.broadcast_tx.subscribe())
    }

    async fn locally_present(&self) -> anyhow::Result<Vec<ClientInfo>> {
        // let mut conn = self.pool.get().await?;
        // let resp: Vec<String> = conn.hvals(self.chan_members_key()).await?;
        // resp.iter()
        //     .map(|s| serde_json::from_str(s))
        //     .collect::<Result<Vec<_>, _>>()
        //     .map_err(anyhow::Error::from)
        Ok(self.locally_present.list().await)
    }
}

impl RedisChannelEvents {
    pub async fn new(
        channel_id: Uuid,
        pool: deadpool_redis::Pool,
        channels: Channels<Self, DistributedChannelRegistry>,
    ) -> anyhow::Result<Self> {
        let (tx, _) = broadcast::channel(100);
        let src = RedisEventSource::new(channel_id, pool.clone(), tx.clone()).await?;
        tokio::spawn(src.run());
        let locally_present = LocallyPresent::new(channel_id, channels);

        Ok(Self {
            channel_id,
            broadcast_tx: tx,
            pool,
            locally_present,
        })
    }

    // fn chan_members_key(&self) -> String {
    //     format!("{}:members", self.channel_id)
    // }

    async fn send(
        &self,
        event: ChannelEvent,
        conn: &mut deadpool_redis::Connection,
    ) -> anyhow::Result<()> {
        let evt = serde_json::to_string(&event)?;
        let received = conn
            .publish::<_, _, i32>(&self.channel_id.to_string(), &evt)
            .await?;
        tracing::debug!("published event to {} consumers", received);
        Ok(())
    }
}

struct RedisEventSource {
    channel_id: Uuid,
    pool: deadpool_redis::Pool,
    tx: broadcast::Sender<ChannelEvent>,
}

impl RedisEventSource {
    async fn new(
        channel_id: Uuid,
        pool: deadpool_redis::Pool,
        tx: broadcast::Sender<ChannelEvent>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            channel_id,
            pool,
            tx,
        })
    }

    async fn connect(pool: &deadpool_redis::Pool) -> Result<redis::aio::Connection, anyhow::Error> {
        let conn = pool.get().await?;
        let max_size = pool.status().max_size;
        let conn = deadpool_redis::Connection::take(conn);
        pool.resize(max_size);
        Ok(conn)
    }

    #[tracing::instrument(skip_all, err)]
    pub async fn run(self) -> anyhow::Result<()> {
        let conn = Self::connect(&self.pool).await?;
        let mut pubsub = conn.into_pubsub();
        pubsub.subscribe(&self.channel_id.to_string()).await?;
        let mut messages = pubsub.on_message();
        // FIXME: reconnect
        while let Some(msg) = messages.next().await {
            let msg = msg.get_payload::<String>()?;
            let evt = serde_json::from_str(&msg)?;
            if self.tx.send(evt).is_err() {
                tracing::debug!("event source done, no more receivers alive");
                break;
            }
        }
        drop(messages);
        let _ = pubsub.unsubscribe(&self.channel_id.to_string()).await;
        Ok(())
    }
}
