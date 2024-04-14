use std::net::SocketAddr;

use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct OpenConnection {
    pub user_name: String,
    pub channel_id: Uuid,
    pub client_id: Uuid,
}

#[derive(Debug)]
pub struct ConnectionData {
    pub sock: SocketAddr,
    pub source_id: u32,
}

#[derive(Debug)]
pub struct EstablishSession {
    pub channel_id: Uuid,
    pub client_id: Uuid,
    pub client_addr: SocketAddr,
}

pub struct SessionData {
    pub crypt_key: Vec<u8>,
}

impl std::fmt::Debug for SessionData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionData")
            .field("crypt_key", &"CENSORED")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: Uuid,
    pub source_id: u32,
    pub name: String,
}

#[derive(Debug)]
pub enum ConnectionState {
    Waiting,
    Peered,
    Stopped,
}
