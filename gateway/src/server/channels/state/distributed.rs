use std::sync::{Arc, Weak};

// publish: join / leave
// unsubscribe on last
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use redis::AsyncCommands;
use tokio::sync::{broadcast, RwLock};
use tracing::Instrument;
use uuid::Uuid;

use crate::server::channels::{
    ChannelEvent, ChannelInit, ChannelMap, ChatRoomJoined, ClientInfo, LocallyPresent,
};

use super::ChannelState;


// FIXME: Feature gates
/// Channel event implementation broadcasting over Redis PubSub
#[derive(Clone)]
pub struct RedisChannelEvents {
    channel_id: Uuid,
    broadcast_tx: broadcast::Sender<ChannelEvent>,
    pool: deadpool_redis::Pool,
    locally_present: Arc<RwLock<LocallyPresent>>,
    channels: Weak<ChannelMap>,
}

#[derive(Clone)]
pub struct RedisChannelInitializer {
    pool: deadpool_redis::Pool,
}

impl RedisChannelInitializer {
    pub fn new(pool: deadpool_redis::Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChannelInit for RedisChannelInitializer {
    type ChannelState = RedisChannelEvents;
    async fn init(
        &self,
        uuid: Uuid,
        channels: Weak<ChannelMap>,
    ) -> anyhow::Result<RedisChannelEvents> {
        RedisChannelEvents::new(uuid, self.pool.clone(), channels.clone()).await
    }
}

#[async_trait]
impl ChannelState for RedisChannelEvents {
    async fn join(&self, info: ClientInfo) -> anyhow::Result<ChatRoomJoined> {
        let mut guard: tokio::sync::RwLockWriteGuard<'_, LocallyPresent> =
            self.locally_present.write().await;
        if !guard.join(info.clone()) {
            anyhow::bail!("local channel was closed");
        }
        drop(guard);
        let client_id = info.client_id;
        let source_id = info.source_id;
        let name = info.name.clone();
        let info_json = serde_json::json!({
            "client_id": client_id,
            "name": info.name,
            "source_id": info.source_id,
            "joined_at": Utc::now(),
        });
        let (joined, mut drop_indicator) =
            ChatRoomJoined::new(self.broadcast_tx.subscribe(), info.clone());
        let slf = self.clone();
        tokio::spawn(async move {
            let chan_members_key = slf.chan_members_key();
            let refresh_field = slf.chan_members_last_seen_hash(client_id);
            loop {
                let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(5));
                tokio::select! {
                        _ = &mut drop_indicator => {
                            if let Err(e) = slf.leave(info).await {
                                tracing::error!("leaving failed: {}", e);
                            }
                            break
                        }
                        _ = refresh_interval.tick() => {
                            if let Err(e) = slf.refresh_member(chan_members_key.clone(), refresh_field.clone()).await {
                                tracing::error!("failed to refresh channel presence for {} -> {}: {}", chan_members_key, refresh_field, e)
                            } else {
                                tracing::debug!("refreshed {}", refresh_field)
                            }
                        }

                }
            }
        }.instrument(tracing::debug_span!("channel_member_update")));
        let mut conn = self.pool.get().await?;
        let event = ChannelEvent::joined(client_id, name, source_id);
        conn.hset_multiple::<_, _, _, ()>(
            &self.chan_members_key(),
            &[
                (
                    client_id.to_string(),
                    serde_json::to_string(&info_json).unwrap(),
                ),
                (
                    self.chan_members_last_seen_hash(client_id),
                    Utc::now().to_rfc3339(),
                ),
            ],
        )
        .await?;
        self.send(event, &mut conn).await?;
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
        drop(guard);

        let event = ChannelEvent::left(info.client_id, info.name, info.source_id);
        let mut conn = self.pool.get().await?;
        self.send(event, &mut conn).await?;
        // FIXME: there's no guarantueed cleanup
        conn.hdel::<_, _, ()>(&self.chan_members_key(), info.client_id.to_string())
            .await?;
        Ok(())
    }

    async fn list_members(&self) -> anyhow::Result<Vec<ClientInfo>> {
        let mut conn = self.pool.get().await?;
        let resp: Vec<String> = conn.hvals(self.chan_members_key()).await?;
        resp.iter()
            .map(|s| serde_json::from_str(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::from)
        // Ok(self.locally_present.read().await.list().await)
    }
}

impl RedisChannelEvents {
    pub async fn new(
        channel_id: Uuid,
        pool: deadpool_redis::Pool,
        channels: Weak<ChannelMap>,
    ) -> anyhow::Result<Self> {
        let (tx, _) = broadcast::channel(100);
        let src = RedisEventSource::new(channel_id, pool.clone(), tx.clone()).await?;
        tokio::spawn(src.run());

        Ok(Self {
            channel_id,
            broadcast_tx: tx,
            pool,
            locally_present: Arc::default(),
            channels,
        })
    }

    async fn refresh_member(&self, key: String, field: String) -> anyhow::Result<()> {
        let mut conn = self.pool.get().await?;
        conn.hset(key, field, Utc::now().to_rfc3339()).await?;
        Ok(())
    }

    fn chan_members_key(&self) -> String {
        format!("{}:members", self.channel_id)
    }

    fn chan_members_last_seen_hash(&self, client_id: Uuid) -> String {
        format!("last_seen:{}", client_id)
    }

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
