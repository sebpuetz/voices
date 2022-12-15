use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use uuid::Uuid;

use voice_proto::Voice;

use crate::ClientInfo;


/// In-memory representation of channels
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

/// Chatroom representation
/// 
/// Offers a broadcast sender for channel events, a list of present users by ID and a voice sender
// TODO: separate Chatroom representation and voice package delivery
#[derive(Clone)]
pub struct Chatroom {
    room_id: Uuid,
    notify: broadcast::Sender<ChannelEvent>,
    present: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    voice_tx: broadcast::Sender<(u32, Voice)>,
}

impl Chatroom {
    pub fn new(id: Uuid) -> Self {
        let slf = Self {
            room_id: id,
            notify: broadcast::channel(100).0,
            present: Arc::default(),
            voice_tx: broadcast::channel(100).0,
        };
        let present = slf.present.clone();
        let mut updates = slf.updates();
        tokio::spawn(async move {
            while let Ok(ChannelEvent {
                source,
                source_id,
                kind,
            }) = updates.recv().await
            {
                match kind {
                    ChannelEventKind::Joined(name) => {
                        present.write().unwrap().insert(
                            source,
                            ClientInfo {
                                client_id: source,
                                source_id,
                                name,
                            },
                        );
                    }
                    ChannelEventKind::Left(_) => {
                        present.write().unwrap().remove(&source);
                    }
                }
            }
        });
        slf
    }

    pub fn updates(&self) -> broadcast::Receiver<ChannelEvent> {
        self.notify.subscribe()
    }

    pub fn list(&self) -> Vec<ClientInfo> {
        self.present.read().unwrap().values().cloned().collect()
    }

    pub fn join(&self, info: ClientInfo) -> ChatRoomVoiceHandle {
        let ClientInfo {
            client_id,
            source_id,
            name,
        } = info;
        let _ = self
            .notify
            .send(ChannelEvent::joined(client_id, name.clone(), source_id));
        ChatRoomVoiceHandle::new(
            self.voice_tx.clone(),
            self.notify.clone(),
            client_id,
            source_id,
            name,
        )
    }

    pub fn id(&self) -> Uuid {
        self.room_id
    }
}

#[derive(Clone, Debug)]
pub enum ChannelEventKind {
    Joined(String),
    Left(String),
}

#[derive(Clone, Debug)]
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

pub struct ChatRoomVoiceHandle {
    rx: broadcast::Receiver<(u32, Voice)>,
    tx: broadcast::Sender<(u32, Voice)>,
    leave_tx: broadcast::Sender<ChannelEvent>,
    source_id: u32,
    client_id: Uuid,
    name: String,
}

impl ChatRoomVoiceHandle {
    fn new(
        tx: broadcast::Sender<(u32, Voice)>,
        leave_tx: broadcast::Sender<ChannelEvent>,
        client_id: Uuid,
        source_id: u32,
        name: String,
    ) -> Self {
        Self {
            rx: tx.subscribe(),
            tx,
            source_id,
            client_id,
            leave_tx,
            name,
        }
    }

    pub fn send(&self, msg: Voice) -> Result<usize, broadcast::error::SendError<(u32, Voice)>> {
        self.tx.send((self.source_id, msg))
    }

    pub async fn recv(&mut self) -> Result<(u32, Voice), broadcast::error::RecvError> {
        self.rx.recv().await
    }

    pub fn source_id(&self) -> u32 {
        self.source_id
    }

    pub fn id(&self) -> Uuid {
        self.client_id
    }
}

impl Drop for ChatRoomVoiceHandle {
    fn drop(&mut self) {
        let _ = self.leave_tx.send(ChannelEvent::left(
            self.client_id,
            self.name.clone(),
            self.source_id,
        ));
    }
}
