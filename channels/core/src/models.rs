use crate::db_models::channel::ChannelDb;
use crate::db_models::server::ServerDb;
use crate::db_models::voice_server::VoiceServerDb;

pub(crate) use voices_channels_models::*;

impl From<ServerDb> for Server {
    fn from(value: ServerDb) -> Self {
        Server {
            id: value.id,
            name: value.name,
        }
    }
}

impl From<ChannelDb> for ChannelInfo {
    fn from(value: ChannelDb) -> Self {
        ChannelInfo {
            id: value.id,
            name: value.name,
        }
    }
}
impl From<VoiceServerDb> for VoiceServer {
    fn from(value: VoiceServerDb) -> Self {
        let VoiceServerDb {
            id,
            host_url,
            last_seen,
        } = value;
        VoiceServer {
            id,
            host_url,
            last_seen,
        }
    }
}
