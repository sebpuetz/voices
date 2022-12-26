use std::net::IpAddr;
use std::time::{Duration, SystemTime};

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
    pub name: String,
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

impl Keepalive {
    pub fn diff(&self) -> Duration {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time shouldn't be before unix epoch");
        now - Duration::from_millis(self.sent_at)
    }
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
    Joined(Present),
    Disconnected(Disconnected),
    Left(Left),
    UdpAnnounce(Announce),
    JoinError(JoinError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Disconnected {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ready {
    pub id: Uuid,
    pub src_id: u32,
    pub present: Vec<Present>,
    // b64 encoded xsalsa20poly1305 key
    pub crypt_key: secstr::SecUtf8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Present {
    pub user: String,
    pub source_id: u32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinError {
    pub room_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Left {
    pub user: String,
    pub source_id: u32,
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
