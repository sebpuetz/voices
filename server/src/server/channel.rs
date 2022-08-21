use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::voice::Voice;

#[derive(Default, Clone)]
pub struct Channels {
    inner: Arc<RwLock<HashMap<Uuid, Chatroom>>>,
}

impl Channels {
    pub fn new() -> Self {
        Self {
            inner: Arc::default(),
        }
    }

    pub fn get_or_create(&self, id: Uuid) -> Chatroom {
        if let Some(room) = self.inner.read().unwrap().get(&id) {
            return room.clone();
        }
        let room = Chatroom::new(id);
        self.inner.write().unwrap().insert(id, room.clone());
        room
    }
}

#[derive(Clone)]
pub struct Chatroom {
    room_id: Uuid,
    notify: broadcast::Sender<ChannelEvent>,
    voice_tx: broadcast::Sender<(Uuid, Voice)>,
}

impl Chatroom {
    pub fn new(id: Uuid) -> Self {
        Self {
            room_id: id,
            notify: broadcast::channel(100).0,
            voice_tx: broadcast::channel(100).0,
        }
    }

    pub fn updates(&self) -> broadcast::Receiver<ChannelEvent> {
        self.notify.subscribe()
    }

    pub fn join(&self, id: Uuid) -> ChatRoomVoiceHandle {
        let _ = self.notify.send(ChannelEvent::joined(id));
        ChatRoomVoiceHandle::new(self.voice_tx.clone(), self.notify.clone(), id)
    }

    pub fn id(&self) -> Uuid {
        self.room_id
    }
}

#[derive(Clone, Debug)]
pub enum ChannelEventKind {
    Joined(Uuid),
    Left(Uuid),
}

#[derive(Clone, Debug)]
pub struct ChannelEvent {
    source: Uuid,
    kind: ChannelEventKind,
}

impl ChannelEvent {
    pub fn joined(source: Uuid) -> Self {
        Self {
            source,
            kind: ChannelEventKind::Joined(source),
        }
    }
    pub fn left(source: Uuid) -> Self {
        Self {
            source,
            kind: ChannelEventKind::Left(source),
        }
    }

    pub fn source(&self) -> Uuid {
        self.source
    }

    pub fn kind(&self) -> &ChannelEventKind {
        &self.kind
    }
}

pub struct ChatRoomVoiceHandle {
    rx: broadcast::Receiver<(Uuid, Voice)>,
    tx: broadcast::Sender<(Uuid, Voice)>,
    leave_tx: broadcast::Sender<ChannelEvent>,
    id: Uuid,
}

impl ChatRoomVoiceHandle {
    fn new(
        tx: broadcast::Sender<(Uuid, Voice)>,
        leave_tx: broadcast::Sender<ChannelEvent>,
        id: Uuid,
    ) -> Self {
        Self {
            rx: tx.subscribe(),
            tx,
            id,
            leave_tx,
        }
    }

    pub fn send(&self, msg: Voice) -> Result<usize, broadcast::error::SendError<(Uuid, Voice)>> {
        self.tx.send((self.id, msg))
    }

    pub async fn recv(&mut self) -> Result<(Uuid, Voice), broadcast::error::RecvError> {
        self.rx.recv().await
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
}

impl Drop for ChatRoomVoiceHandle {
    fn drop(&mut self) {
        let _ = self.leave_tx.send(ChannelEvent::left(self.id));
    }
}
