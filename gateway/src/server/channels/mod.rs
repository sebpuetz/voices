pub mod state;

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::channel_registry::{ChannelRegistry, GetVoiceHost};
use crate::voice_instance::VoiceHost;

use state::ChannelState;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClientInfo {
    pub client_id: Uuid,
    pub source_id: u32,
    pub name: String,
}

#[async_trait]
pub trait ChannelInit: Send + Sync + 'static {
    type ChannelState: ChannelState;
    async fn init(
        &self,
        uuid: Uuid,
        channels: Weak<ChannelMap>,
    ) -> anyhow::Result<Self::ChannelState>;
}

/// In-memory representation of channels
#[derive(Clone)]
pub struct Channels<S, R> {
    registry: R,
    // channels with sessions on this instance
    #[allow(clippy::type_complexity)]
    channel_map: Arc<ChannelMap>,
    channel_init: Arc<dyn ChannelInit<ChannelState = S>>,
}

impl<S, R> Channels<S, R> {
    pub async fn get(&self, id: Uuid) -> Option<Channel> {
        self.channel_map.inner.read().await.get(&id).cloned()
    }
}

pub struct ChannelMap {
    inner: RwLock<HashMap<Uuid, Channel>>,
}

impl ChannelMap {
    pub async fn close(&self, channel: Uuid) {
        // FIXME: there should be some check that there's no pending joins on this channel / no live handles
        let _ = self.inner.write().await.remove(&channel);
    }
}
// State: List Members, Subscribe, Publish
impl<S, R> Channels<S, R>
where
    S: ChannelState,
    R: GetVoiceHost,
{
    pub fn new<F>(room_init: F, registry: R) -> Self
    where
        F: ChannelInit<ChannelState = S>,
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
    pub async fn get_or_init(&self, id: Uuid) -> anyhow::Result<Option<Channel>> {
        if let Some(room) = self.get(id).await {
            return Ok(Some(room.clone()));
        }
        let voice = self.registry.get_voice_host_for(id, false).await?;
        let voice = match voice {
            Some(voice) => voice,
            None => return Ok(None),
        };

        // FIXME: The channel instance gets a reference to the top level channel map so it can remove itself
        // after it's empty. This is rather hacky since there's a cycle between the channel and that map.
        let channels = Arc::downgrade(&self.channel_map);
        let state = self.channel_init.init(id, channels).await?;
        let room = Channel::new(id, state, voice).await?;
        let r = self
            .channel_map
            .inner
            .write()
            .await
            .entry(id)
            .or_insert(room)
            .clone();
        Ok(Some(r))
    }

    // FIXME: send reassign event
    pub async fn reassign_voice(&self, channel_id: Uuid) -> anyhow::Result<Channel> {
        let voice = self
            .registry
            .get_voice_host_for(channel_id, true)
            .await?
            .context("no voice server available")?;
        match self.channel_map.inner.write().await.entry(channel_id) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let chan = o.get_mut();
                chan.voice = Arc::new(voice);
                Ok(chan.clone())
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                let channels = Arc::downgrade(&self.channel_map);
                let state = self.channel_init.init(channel_id, channels).await?;
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
}
impl<S, R> Channels<S, R>
where
    R: ChannelRegistry,
{
    pub fn registry(&self) -> &R {
        &self.registry
    }
}

/// Channel representation
///
/// Offers a broadcast sender for channel events, a list of present users by ID and a voice sender
#[derive(Clone)]
pub struct Channel {
    room_id: Uuid,
    state: Arc<dyn ChannelState>,
    voice: Arc<dyn VoiceHost>,
}

impl Channel {
    pub async fn new<S, V>(id: Uuid, state: S, voice: V) -> anyhow::Result<Self>
    where
        S: ChannelState,
        V: VoiceHost,
    {
        let slf = Self {
            room_id: id,
            state: Arc::new(state),
            voice: Arc::new(voice),
        };
        Ok(slf)
    }

    pub async fn join(&self, info: ClientInfo) -> anyhow::Result<ChatRoomJoined> {
        self.state.join(info).await
    }

    pub fn id(&self) -> Uuid {
        self.room_id
    }

    pub fn voice(&self) -> Arc<dyn VoiceHost> {
        self.voice.clone()
    }

    pub async fn list_members(&self) -> anyhow::Result<Vec<ClientInfo>> {
        self.state.list_members().await
    }
}

#[derive(Clone)]
pub struct LocallyPresent {
    inner: HashMap<Uuid, ClientInfo>,
    live: bool,
}

impl Default for LocallyPresent {
    fn default() -> Self {
        Self::new()
    }
}

impl LocallyPresent {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            live: true,
        }
    }

    // FIXME: return enum
    pub fn join(&mut self, info: ClientInfo) -> bool {
        if !self.live {
            return false;
        }
        self.inner.insert(info.client_id, info);
        true
    }

    // FIXME: return enum
    pub fn leave(&mut self, client_id: Uuid) -> bool {
        self.inner.remove(&client_id);
        self.live = !self.inner.is_empty();
        !self.live
    }

    pub async fn list(&self) -> Vec<ClientInfo> {
        self.inner.values().cloned().collect()
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

pub struct ChatRoomJoined {
    updates: broadcast::Receiver<ChannelEvent>,
    info: ClientInfo,
    dropped: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ChatRoomJoined {
    // FIXME: second value in the returned tuple is a hack to indicate when keep alive in redis member list
    // should stop
    pub fn new(
        updates: broadcast::Receiver<ChannelEvent>,
        info: ClientInfo,
    ) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            Self {
                info,
                updates,
                dropped: Some(tx),
            },
            rx,
        )
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

impl Drop for ChatRoomJoined {
    fn drop(&mut self) {
        self.dropped.take();
    }
}
