#[allow(clippy::all)]
pub mod voice {
    include!(concat!(env!("OUT_DIR"), "/voice.rs"));
}
pub use voice::*;
pub use Voice as ServerVoice;

#[cfg(feature = "server")]
impl ServerMessage {
    pub fn new(msg: server_message::Message) -> Self {
        ServerMessage { message: Some(msg) }
    }

    pub fn pong(pong: Pong) -> Self {
        ServerMessage::new(server_message::Message::Pong(pong))
    }

    pub fn voice(voice: Voice) -> Self {
        ServerMessage::new(server_message::Message::Voice(voice))
    }

    pub fn ip_disco(disco: IpDiscoveryResponse) -> Self {
        ServerMessage::new(server_message::Message::IpDisco(disco))
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

    pub fn ip_disco(disco: IpDiscoveryRequest) -> Self {
        ClientMessage::new(client_message::Payload::IpDisco(disco))
    }
}
