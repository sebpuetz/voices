use std::net::IpAddr;
use std::str::FromStr;

use anyhow::Context;
use async_trait::async_trait;
use tonic::transport::Channel;
use tonic::{Code, Status};
use uuid::Uuid;

use super::voice_server_proto::{self as proto, voice_server_client::VoiceServerClient};
use super::*;

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

    pub async fn assign_channel(&self, channel_id: Uuid) -> Result<(), Status> {
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
        request: OpenConnection,
    ) -> Result<ConnectionData, VoiceHostError> {
        self.inner
            .clone()
            .open_connection(tonic::Request::new(request.into()))
            .await?
            .into_inner()
            .try_into()
            .map_err(Into::into)
    }

    async fn establish_session(
        &self,
        request: EstablishSession,
    ) -> Result<SessionData, VoiceHostError> {
        Ok(self
            .inner
            .clone()
            .establish_session(tonic::Request::new(request.into()))
            .await?
            .into_inner()
            .into())
    }

    async fn leave(&self, channel_id: Uuid, client_id: Uuid) -> Result<(), VoiceHostError> {
        self.inner
            .clone()
            .leave(tonic::Request::new(proto::LeaveRequest {
                user_id: client_id.to_string(),
                channel_id: channel_id.to_string(),
            }))
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn user_status(
        &self,
        channel_id: Uuid,
        client_id: Uuid,
    ) -> Result<ConnectionState, VoiceHostError> {
        self.inner
            .clone()
            .user_status(tonic::Request::new(proto::UserStatusRequest {
                channel_id: channel_id.to_string(),
                user_id: client_id.to_string(),
            }))
            .await?
            .into_inner()
            .try_into()
            .map_err(Into::into)
    }

    async fn status(&self, channel_id: Uuid) -> Result<Vec<Peer>, VoiceHostError> {
        self.inner
            .clone()
            .status(tonic::Request::new(proto::StatusRequest {
                channel_id: channel_id.to_string(),
            }))
            .await?
            .into_inner()
            .try_into()
            .map_err(Into::into)
    }
}

impl From<Status> for VoiceHostError {
    fn from(value: Status) -> Self {
        match value.code() {
            Code::NotFound => {
                let msg = value.message();
                if msg.contains("peer") {
                    VoiceHostError::NotFound("client")
                } else if msg.contains("channel") {
                    VoiceHostError::NotFound("channel")
                } else {
                    VoiceHostError::NotFound("something")
                }
            }
            _ => VoiceHostError::Other(anyhow::Error::from(value).context("Voice host error")),
        }
    }
}

impl From<OpenConnection> for proto::OpenConnectionRequest {
    fn from(value: OpenConnection) -> Self {
        proto::OpenConnectionRequest {
            user_name: value.user_name,
            user_id: value.client_id.to_string(),
            channel_id: value.channel_id.to_string(),
        }
    }
}

impl TryFrom<proto::OpenConnectionResponse> for ConnectionData {
    type Error = anyhow::Error;
    fn try_from(value: proto::OpenConnectionResponse) -> Result<ConnectionData, anyhow::Error> {
        let proto::SockAddr { ip, port } = try_opt(value.udp_sock, "udp_sock")?;
        let ip: IpAddr = parse(&ip, "ip")?;
        let port = port
            .try_into()
            .with_context(|| format!("port out of range: {}", port))?;
        Ok(ConnectionData {
            sock: (ip, port).into(),
            source_id: value.src_id,
        })
    }
}

impl From<EstablishSession> for proto::EstablishSessionRequest {
    fn from(value: EstablishSession) -> Self {
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

impl From<proto::EstablishSessionResponse> for SessionData {
    fn from(value: proto::EstablishSessionResponse) -> SessionData {
        SessionData {
            crypt_key: value.crypt_key,
        }
    }
}

impl TryFrom<proto::StatusResponse> for Vec<Peer> {
    type Error = anyhow::Error;

    fn try_from(value: proto::StatusResponse) -> Result<Self, anyhow::Error> {
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
    type Error = anyhow::Error;

    fn try_from(value: proto::UserStatusResponse) -> Result<Self, Self::Error> {
        match try_opt(value.status, "status")? {
            proto::user_status_response::Status::Error(()) => Ok(ConnectionState::Stopped),
            proto::user_status_response::Status::Peered(()) => Ok(ConnectionState::Peered),
            proto::user_status_response::Status::Waiting(()) => Ok(ConnectionState::Waiting),
        }
    }
}

fn try_opt<T>(value: Option<T>, field_name: &'static str) -> Result<T, anyhow::Error> {
    value.with_context(|| format!("missing {}", field_name))
}

fn parse<S, E>(value: &str, field_name: &'static str) -> Result<S, anyhow::Error>
where
    S: FromStr<Err = E>,
    E: Send + Sync + std::error::Error + 'static,
{
    value
        .parse()
        .with_context(|| format!("bad {}: '{}'", field_name, value))
}
