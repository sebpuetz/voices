#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IpDiscoveryRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub uuid: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IpDiscoveryResponse {
    #[prost(string, tag = "1")]
    pub ip: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub port: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Voice {
    #[prost(uint64, tag = "1")]
    pub sequence: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ServerVoice {
    #[prost(uint64, tag = "1")]
    pub sequence: u64,
    #[prost(uint32, tag = "2")]
    pub source_id: u32,
    #[prost(bytes = "vec", tag = "3")]
    pub payload: ::prost::alloc::vec::Vec<u8>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ping {
    #[prost(uint64, tag = "1")]
    pub seq: u64,
    #[prost(uint64, tag = "2")]
    pub recv: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Pong {
    #[prost(uint64, tag = "1")]
    pub seq: u64,
    #[prost(uint64, tag = "2")]
    pub sent: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientMessage {
    #[prost(oneof = "client_message::Payload", tags = "1, 2")]
    pub payload: ::core::option::Option<client_message::Payload>,
}
/// Nested message and enum types in `ClientMessage`.
pub mod client_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag = "1")]
        Ping(super::Ping),
        #[prost(message, tag = "2")]
        Voice(super::Voice),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ServerMessage {
    #[prost(oneof = "server_message::Message", tags = "1, 2")]
    pub message: ::core::option::Option<server_message::Message>,
}
/// Nested message and enum types in `ServerMessage`.
pub mod server_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Message {
        #[prost(message, tag = "1")]
        Pong(super::Pong),
        #[prost(message, tag = "2")]
        Voice(super::ServerVoice),
    }
}
