use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, strum::IntoStaticStr)]
#[serde(tag = "type")]
pub enum ClientEvent {
    Init(Init),
    Keepalive(Keepalive),
    Join(Join),
    UdpAnnounce(Announce),
    Leave,
    Disconnect,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Init {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Announce {
    pub ip: IpAddr,
    pub port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Keepalive {
    pub sent_at: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Join {
    pub channel_id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    Keepalive(Keepalive),
    Ready(Ready),
    Joined(Joined),
    Left(Left),
    UdpAnnounce(Announce),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ready {
    pub id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Joined {
    pub user: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Left {
    pub user: Uuid,
}

pub trait MessageExt {
    fn as_bytes(&self) -> &[u8];
    fn json<'a, T>(&'a self) -> Result<T, serde_json::Error>
    where
        T: Deserialize<'a> + 'a,
    {
        serde_json::from_slice(self.as_bytes())
    }
}

impl MessageExt for tungstenite::Message {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(string) => string.as_bytes(),
            Self::Binary(data) | Self::Ping(data) | Self::Pong(data) => data,
            Self::Close(None) => &[],
            Self::Close(Some(frame)) => frame.reason.as_bytes(),
            Self::Frame(frame) => frame.payload(),
        }
    }
}
