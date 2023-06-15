#[path = "./voice_channels.v1.rs"]
pub mod voice_channels_proto;

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use super::channel_registry::ChannelRegistry;
use super::channel_state::ChannelState;
use super::voice_instance::VoiceHost;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClientInfo {
    pub client_id: Uuid,
    pub source_id: u32,
    pub name: String,
}

#[async_trait]
pub trait ChannelInit<S, V>: Send + Sync + 'static {
    async fn init(&self, uuid: Uuid, channels: Weak<ChannelMap<S, V>>) -> anyhow::Result<S>;
}

#[async_trait]
impl<F, Fut, S, V> ChannelInit<S, V> for F
where
    F: 'static + Send + Sync + Fn(Uuid, Weak<ChannelMap<S, V>>) -> Fut,
    Fut: Send + std::future::Future<Output = anyhow::Result<S>>,
    S: Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    async fn init(&self, uuid: Uuid, channels: Weak<ChannelMap<S, V>>) -> anyhow::Result<S> {
        (self)(uuid, channels).await
    }
}

/// In-memory representation of channels
#[derive(Clone)]
pub struct Channels<S, R, V> {
    registry: R,
    // channels with sessions on this instance
    #[allow(clippy::type_complexity)]
    channel_map: Arc<ChannelMap<S, V>>,
    channel_init: Arc<dyn ChannelInit<S, V>>,
}

pub struct ChannelMap<S, V> {
    inner: RwLock<HashMap<Uuid, Channel<S, V>>>,
}

impl<S, V> ChannelMap<S, V> {
    pub async fn close(&self, channel: Uuid) {
        // FIXME: there should be some check that there's no pending joins on this channel / no live handles
        let _ = self.inner.write().await.remove(&channel);
    }
}

impl<S, R> Channels<S, R, R::Voice>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    pub fn new<F>(room_init: F, registry: R) -> Self
    where
        F: ChannelInit<S, R::Voice>,
    {
        Self {
            channel_map: Arc::new(ChannelMap {
                inner: RwLock::new(HashMap::new()),
            }),
            channel_init: Arc::new(room_init),
            registry,
        }
    }

    /// Retrieve the locally running instance of
    pub async fn get_or_init(&self, id: Uuid) -> anyhow::Result<Channel<S, R::Voice>>
    where
        R: 'static,
    {
        if let Some(room) = self.channel_map.inner.read().await.get(&id) {
            return Ok(room.clone());
        }
        let voice = self
            .registry
            .get_voice_host(id, false)
            .await?
            .context("no voice server available")?;
        let weak = Arc::downgrade(&self.channel_map);
        let state = self.channel_init.init(id, weak).await?;
        let room = Channel::new(id, state, voice).await?;
        let r = self
            .channel_map
            .inner
            .write()
            .await
            .entry(id)
            .or_insert(room)
            .clone();
        Ok(r)
    }

    // FIXME: send reassign event
    pub async fn reassign_voice(&self, channel_id: Uuid) -> anyhow::Result<Channel<S, R::Voice>> {
        let voice = self
            .registry
            .get_voice_host(channel_id, true)
            .await?
            .context("no voice server available")?;
        match self.channel_map.inner.write().await.entry(channel_id) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let chan = o.get_mut();
                chan.voice = voice;
                Ok(chan.clone())
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                let weak = Arc::downgrade(&self.channel_map);
                let state = self.channel_init.init(channel_id, weak).await?;
                let room = Channel::new(channel_id, state, voice).await?;
                Ok(v.insert(room).to_owned())
            }
        }
    }

    pub async fn close(&self, channel_id: Uuid) -> Option<Uuid> {
        self.channel_map
            .inner
            .write()
            .await
            .remove(&channel_id)
            .map(|_| channel_id)
    }

    pub fn registry(&self) -> &R {
        &self.registry
    }
}

/// Channel representation
///
/// Offers a broadcast sender for channel events, a list of present users by ID and a voice sender
#[derive(Clone)]
pub struct Channel<S, V> {
    room_id: Uuid,
    state: S,
    voice: V,
}

impl<S, V> Channel<S, V>
where
    S: ChannelState,
    V: VoiceHost + Clone,
{
    pub async fn new(id: Uuid, state: S, voice: V) -> anyhow::Result<Self> {
        let slf = Self {
            room_id: id,
            state,
            voice,
        };
        Ok(slf)
    }

    pub async fn updates(&self) -> anyhow::Result<broadcast::Receiver<ChannelEvent>> {
        self.state.subscribe().await
    }

    pub async fn join(&self, info: ClientInfo) -> anyhow::Result<ChatRoomJoined<S>> {
        let updates = self.updates().await?;
        let _ = self.state.join(info.clone()).await;
        Ok(ChatRoomJoined::new(self.state.clone(), updates, info))
    }

    pub fn id(&self) -> Uuid {
        self.room_id
    }

    pub fn voice(&self) -> &V {
        &self.voice
    }
}

#[derive(Clone)]
pub struct LocallyPresent<S, V> {
    id: Uuid,
    present: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    channels: Weak<ChannelMap<S, V>>,
}

impl<S, V> LocallyPresent<S, V> {
    pub fn new(id: Uuid, channels: Weak<ChannelMap<S, V>>) -> Self {
        Self {
            id,
            present: Arc::default(),
            channels,
        }
    }

    pub async fn join(&self, info: ClientInfo) {
        self.present.write().await.insert(info.client_id, info);
    }

    pub async fn leave(&self, client_id: Uuid) {
        let mut guard = self.present.write().await;
        guard.remove(&client_id);
        // FIXME: there's probably a race between joiners waiting on the write lock and removing the channel
        // from the local channel registry
        if guard.is_empty() {
            if let Some(channels) = self.channels.upgrade() {
                tracing::info!(channel_id=?self.id, "last client left, closing");
                channels.close(self.id).await;
            }
        }
    }

    pub async fn list(&self) -> Vec<ClientInfo> {
        self.present.read().await.values().cloned().collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChannelEventKind {
    Joined(String),
    Left(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelEvent {
    source: Uuid,
    source_id: u32,
    kind: ChannelEventKind,
}

impl ChannelEvent {
    pub fn joined(source: Uuid, name: String, source_id: u32) -> Self {
        Self {
            source,
            source_id,
            kind: ChannelEventKind::Joined(name),
        }
    }
    pub fn left(source: Uuid, name: String, source_id: u32) -> Self {
        Self {
            source,
            source_id,
            kind: ChannelEventKind::Left(name),
        }
    }

    pub fn source(&self) -> Uuid {
        self.source
    }

    pub fn kind(&self) -> &ChannelEventKind {
        &self.kind
    }

    pub fn source_id(&self) -> u32 {
        self.source_id
    }
}

pub struct ChatRoomJoined<S>
where
    S: ChannelState,
{
    leave_tx: Option<S>,
    updates: broadcast::Receiver<ChannelEvent>,
    info: ClientInfo,
}

impl<S> ChatRoomJoined<S>
where
    S: ChannelState,
{
    fn new(leave_tx: S, updates: broadcast::Receiver<ChannelEvent>, info: ClientInfo) -> Self {
        Self {
            info,
            updates,
            leave_tx: Some(leave_tx),
        }
    }

    pub async fn recv(&mut self) -> Result<ChannelEvent, broadcast::error::RecvError> {
        self.updates.recv().await
    }

    pub fn source_id(&self) -> u32 {
        self.info.source_id
    }

    pub fn id(&self) -> Uuid {
        self.info.client_id
    }
}

impl<S> Drop for ChatRoomJoined<S>
where
    S: ChannelState,
{
    fn drop(&mut self) {
        if let Some(tx) = self.leave_tx.take() {
            let info = self.info.clone();
            tokio::spawn(async move {
                let _ = tx.leave(info).await;
            });
        }
    }
}
