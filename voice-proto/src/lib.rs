#[allow(clippy::all)]
pub mod voice {
    include!(concat!(env!("OUT_DIR"), "/voice.rs"));
}
pub use voice::*;

#[cfg(feature = "server")]
impl ServerMessage {
    pub fn new(msg: server_message::Message) -> Self {
        ServerMessage { message: Some(msg) }
    }

    pub fn pong(pong: Pong) -> Self {
        ServerMessage::new(server_message::Message::Pong(pong))
    }

    pub fn voice(voice: Voice, src_id: u32) -> Self {
        ServerMessage::new(server_message::Message::Voice(ServerVoice {
            sequence: voice.sequence,
            source_id: src_id,
            payload: voice.payload,
        }))
    }
}

#[cfg(feature = "client")]
impl ClientMessage {
    pub fn new(msg: client_message::Payload) -> Self {
        ClientMessage { payload: Some(msg) }
    }

    pub fn ping(ping: Ping) -> Self {
        ClientMessage::new(client_message::Payload::Ping(ping))
    }

    pub fn voice(voice: Voice) -> Self {
        ClientMessage::new(client_message::Payload::Voice(voice))
    }
}
