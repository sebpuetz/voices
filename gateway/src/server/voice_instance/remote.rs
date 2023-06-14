use std::net::IpAddr;
use std::str::FromStr;

use async_trait::async_trait;
use tonic::transport::Channel;
use tonic::Status;
use uuid::Uuid;
use voice_server::channel::connection::ConnectionState;
use voice_server::Peer;

use crate::server::voice_instance::voice_server_proto::{
    self as proto, voice_server_client::VoiceServerClient,
};

#[derive(Clone)]
pub struct RemoteVoiceHost {
    inner: VoiceServerClient<Channel>,
}

impl RemoteVoiceHost {
    pub fn new(uri: tonic::transport::Uri) -> Self {
        RemoteVoiceHost {
            inner: VoiceServerClient::new(Channel::builder(uri).connect_lazy()),
        }
    }

    pub async fn assign_channel(&self, channel_id: Uuid) -> Result<(), tonic::Status> {
        self.inner
            .clone()
            .assign_channel(tonic::Request::new(proto::AssignChannelRequest {
                channel_id: channel_id.to_string(),
            }))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl super::VoiceHost for RemoteVoiceHost {
    async fn open_connection(
        &self,
        request: voice_server::OpenConnection,
    ) -> Result<voice_server::ConnectionData, Status> {
        self.inner
            .clone()
            .open_connection(tonic::Request::new(request.into()))
            .await?
            .into_inner()
            .try_into()
    }

    async fn establish_session(
        &self,
        request: voice_server::EstablishSession,
    ) -> Result<voice_server::SessionData, Status> {
        Ok(self
            .inner
            .clone()
            .establish_session(tonic::Request::new(request.into()))
            .await?
            .into_inner()
            .into())
    }

    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), Status> {
        self.inner
            .clone()
            .leave(tonic::Request::new(proto::LeaveRequest {
                user_id: client_id.to_string(),
                channel_id: channel_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, Status> {
        self.inner
            .clone()
            .user_status(tonic::Request::new(proto::UserStatusRequest {
                channel_id: channel_id.to_string(),
                user_id: client_id.to_string(),
            }))
            .await?
            .into_inner()
            .try_into()
    }

    async fn status(&self, channel_id: Uuid) -> Result<Vec<voice_server::Peer>, Status> {
        self.inner
            .clone()
            .status(tonic::Request::new(proto::StatusRequest {
                channel_id: channel_id.to_string(),
            }))
            .await?
            .into_inner()
            .try_into()
    }
}

impl From<voice_server::OpenConnection> for proto::OpenConnectionRequest {
    fn from(value: voice_server::OpenConnection) -> Self {
        proto::OpenConnectionRequest {
            user_name: value.user_name,
            user_id: value.client_id.to_string(),
            channel_id: value.channel_id.to_string(),
        }
    }
}

impl TryFrom<proto::OpenConnectionResponse> for voice_server::ConnectionData {
    type Error = Status;
    fn try_from(
        value: proto::OpenConnectionResponse,
    ) -> Result<voice_server::ConnectionData, Status> {
        let proto::SockAddr { ip, port } = try_opt(value.udp_sock, "udp_sock")?;
        let ip: IpAddr = parse(&ip, "ip")?;
        let port = port.try_into().map_err(|_| {
            tracing::warn!("port out of range: {}", port);
            tonic::Status::unknown("port out of range")
        })?;
        Ok(voice_server::ConnectionData {
            sock: (ip, port).into(),
            source_id: value.src_id,
        })
    }
}

impl From<voice_server::EstablishSession> for proto::EstablishSessionRequest {
    fn from(value: voice_server::EstablishSession) -> Self {
        proto::EstablishSessionRequest {
            user_id: value.client_id.to_string(),
            channel_id: value.channel_id.to_string(),
            client_sock: Some(proto::SockAddr {
                ip: value.client_addr.ip().to_string(),
                port: value.client_addr.port() as _,
            }),
        }
    }
}

impl From<proto::EstablishSessionResponse> for voice_server::SessionData {
    fn from(value: proto::EstablishSessionResponse) -> voice_server::SessionData {
        voice_server::SessionData {
            crypt_key: value.crypt_key,
        }
    }
}

impl TryFrom<proto::StatusResponse> for Vec<Peer> {
    type Error = Status;

    fn try_from(value: proto::StatusResponse) -> Result<Self, Status> {
        value
            .info
            .into_iter()
            .map(|v| {
                let client_id = parse(&v.client_id, "client_id")?;
                Ok(Peer {
                    id: client_id,
                    source_id: v.src_id,
                    name: v.name,
                })
            })
            .collect()
    }
}

impl TryFrom<proto::UserStatusResponse> for ConnectionState {
    type Error = Status;

    fn try_from(value: proto::UserStatusResponse) -> Result<Self, Self::Error> {
        match value.status {
            Some(proto::user_status_response::Status::Error(())) => Ok(ConnectionState::Stopped),
            Some(proto::user_status_response::Status::Peered(())) => Ok(ConnectionState::Peered),
            Some(proto::user_status_response::Status::Waiting(())) => Ok(ConnectionState::Waiting),
            None => {
                tracing::warn!("missing connection status field");
                Err(Status::unknown("didn't receive connection status"))
            }
        }
    }
}

fn try_opt<T>(value: Option<T>, field_name: &'static str) -> Result<T, Status> {
    value.ok_or_else(|| {
        tracing::warn!("missing {}", field_name);
        Status::unknown(format!("missing {}", field_name))
    })
}

fn parse<S, E>(value: &str, field_name: &'static str) -> Result<S, Status>
where
    S: FromStr<Err = E>,
    E: std::fmt::Debug,
{
    value.parse().map_err(|e| {
        tracing::warn!(error=?e, "bad {}", field_name);
        Status::unknown(format!("bad {}", field_name))
    })
}
