use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(PartialEq, Eq, Debug)]
pub struct Server {
    pub id: Uuid,
    pub name: String,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Channel {
    pub info: ChannelInfo,
    pub server_id: Uuid,
    pub assigned_to: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct ServerWithChannels {
    pub id: Uuid,
    pub name: String,
    pub channels: Vec<ChannelInfo>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct ChannelInfo {
    pub id: Uuid,
    pub name: String,
}

#[derive(PartialEq, Eq, Debug)]
pub struct VoiceServer {
    pub id: Uuid,
    pub host_url: String,
    pub last_seen: DateTime<Utc>,
}
