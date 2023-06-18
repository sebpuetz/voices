use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use tonic::{async_trait, Status};

use voices_voice::{
    ChannelNotFound, ConnectionData, ConnectionState, EstablishSession, EstablishSessionError,
    LeaveError, OpenConnection, OpenConnectionError, Peer, PeerNotFound, SessionData, StatusError,
    VoiceServerImpl,
};

#[path = "./voice_server.v1.rs"]
pub mod proto;
pub use tonic;

use self::proto::voice_server_server::VoiceServerServer;

pub struct VoiceServerGrpc {
    inner: VoiceServerImpl,
}

impl VoiceServerGrpc {
    pub fn new(inner: VoiceServerImpl) -> VoiceServerServer<Self> {
        VoiceServerServer::new(Self { inner })
    }
}

#[async_trait]
impl proto::voice_server_server::VoiceServer for VoiceServerGrpc {
    async fn assign_channel(
        &self,
        request: tonic::Request<proto::AssignChannelRequest>,
    ) -> Result<tonic::Response<proto::AssignChannelResponse>, tonic::Status> {
        let req = request.into_inner();
        let channel_id = parse(&req.channel_id, "channel_id")?;
        tracing::info!("assigning channel {}", channel_id);
        self.inner.assign_channel(channel_id).await.map_err(|e| {
            tracing::warn!("failed to create channel {}", e);
            tonic::Status::internal("failed to create channel")
        })?;
        tracing::info!("assigned channel {}", channel_id);
        Ok(tonic::Response::new(proto::AssignChannelResponse {}))
    }

    async fn open_connection(
        &self,
        request: tonic::Request<proto::OpenConnectionRequest>,
    ) -> Result<tonic::Response<proto::OpenConnectionResponse>, tonic::Status> {
        let req = request.into_inner().try_into()?;
        let connection_data = self
            .inner
            .open_connection(req)
            .await
            .map_err(|e| {
                tracing::info!(error=?e, "failed to open connection");
                e
            })
            .map_err(map_open_connection_error)?;

        Ok(tonic::Response::new(connection_data.into()))
    }

    async fn establish_session(
        &self,
        request: tonic::Request<proto::EstablishSessionRequest>,
    ) -> Result<tonic::Response<proto::EstablishSessionResponse>, tonic::Status> {
        let req = request.into_inner().try_into()?;

        let response = self
            .inner
            .establish_session(req)
            .await
            .map_err(|e| {
                tracing::info!(error=?e, "failed to establish session");
                e
            })
            .map_err(map_establish_session_error)?;
        Ok(tonic::Response::new(response.into()))
    }

    async fn leave(
        &self,
        request: tonic::Request<proto::LeaveRequest>,
    ) -> Result<tonic::Response<proto::LeaveResponse>, tonic::Status> {
        let req = request.into_inner();
        let client_id = parse(&req.user_id, "user_id")?;
        let channel_id = parse(&req.channel_id, "channel_id")?;
        self.inner
            .leave(channel_id, client_id)
            .await
            .map_err(map_leave_error)?;
        Ok(tonic::Response::new(proto::LeaveResponse {}))
    }

    // FIXME: move logic to VoiceServerImpl
    async fn user_status(
        &self,
        request: tonic::Request<proto::UserStatusRequest>,
    ) -> Result<tonic::Response<proto::UserStatusResponse>, tonic::Status> {
        let req = request.into_inner();
        let client_id = parse(&req.user_id, "user_id")?;
        let channel_id = parse(&req.channel_id, "channel_id")?;
        let status = self
            .inner
            .user_status(channel_id, client_id)
            .await
            .map_err(map_status_error)?;
        Ok(tonic::Response::new(proto::UserStatusResponse {
            status: Some(status.into()),
        }))
    }

    async fn status(
        &self,
        request: tonic::Request<proto::StatusRequest>,
    ) -> Result<tonic::Response<proto::StatusResponse>, tonic::Status> {
        let req = request.into_inner();
        let channel_id = parse(&req.channel_id, "channel_id")?;
        let peers = self
            .inner
            .status(channel_id)
            .await
            .map_err(map_status_error)?;
        Ok(tonic::Response::new(proto::StatusResponse {
            info: peers
                .into_iter()
                .map(
                    |Peer {
                         id: client_id,
                         source_id: src_id,
                         name,
                     }| proto::ClientInfo {
                        client_id: client_id.to_string(),
                        src_id,
                        name,
                    },
                )
                .collect(),
        }))
    }
}

fn map_open_connection_error(value: OpenConnectionError) -> tonic::Status {
    match value {
        OpenConnectionError::NoOpenPorts => tonic::Status::unavailable("server full"),
        OpenConnectionError::Other(_) => tonic::Status::internal("internal error"),
        OpenConnectionError::ChannelNotFound(ch) => map_channel_not_found(ch),
    }
}

impl From<SocketAddr> for proto::SockAddr {
    fn from(value: SocketAddr) -> Self {
        proto::SockAddr {
            ip: value.ip().to_string(),
            port: value.port() as u32,
        }
    }
}

impl From<ConnectionData> for proto::OpenConnectionResponse {
    fn from(ConnectionData { sock, source_id }: ConnectionData) -> Self {
        proto::OpenConnectionResponse {
            udp_sock: Some(sock.into()),
            src_id: source_id,
        }
    }
}

impl TryFrom<proto::OpenConnectionRequest> for OpenConnection {
    type Error = tonic::Status;

    fn try_from(req: proto::OpenConnectionRequest) -> Result<Self, Self::Error> {
        let client_id = parse(&req.user_id, "user_id")?;
        let channel_id = parse(&req.channel_id, "channel_id")?;
        Ok(OpenConnection {
            user_name: req.user_name,
            channel_id,
            client_id,
        })
    }
}

fn map_establish_session_error(value: EstablishSessionError) -> tonic::Status {
    match value {
        EstablishSessionError::PeerNotFound(inner) => map_peer_not_found(inner),
        EstablishSessionError::Other(_) => tonic::Status::internal("internal error"),
        EstablishSessionError::ChannelNotFound(ch) => map_channel_not_found(ch),
    }
}

fn map_leave_error(value: LeaveError) -> tonic::Status {
    match value {
        LeaveError::PeerNotFound(inner) => map_peer_not_found(inner),
        LeaveError::ChannelNotFound(ch) => map_channel_not_found(ch),
    }
}
fn map_status_error(value: StatusError) -> tonic::Status {
    match value {
        StatusError::PeerNotFound(inner) => map_peer_not_found(inner),
        StatusError::ChannelNotFound(ch) => map_channel_not_found(ch),
        StatusError::Other(_) => tonic::Status::internal("internal error"),
    }
}

fn map_peer_not_found(value: PeerNotFound) -> tonic::Status {
    match value {
        PeerNotFound => tonic::Status::not_found("client not present"),
    }
}

fn map_channel_not_found(_: ChannelNotFound) -> tonic::Status {
    tonic::Status::not_found("channel not hosted here")
}

impl TryFrom<proto::EstablishSessionRequest> for EstablishSession {
    type Error = tonic::Status;

    fn try_from(req: proto::EstablishSessionRequest) -> Result<Self, Self::Error> {
        let client_id = parse(&req.user_id, "user_id")?;
        let channel_id = parse(&req.channel_id, "channel_id")?;
        let client_sock_addr = try_opt(req.client_sock, "client_sock")?;
        let client_ip: IpAddr = parse(&client_sock_addr.ip, "client_sock.ip")?;
        let client_port: u16 = client_sock_addr.port.try_into().map_err(|e| {
            tracing::warn!("port ({}) out of range: {}", client_sock_addr.port, e);
            Status::invalid_argument("port out of range")
        })?;

        Ok(EstablishSession {
            channel_id,
            client_id,
            client_addr: SocketAddr::from((client_ip, client_port)),
        })
    }
}

impl From<SessionData> for proto::EstablishSessionResponse {
    fn from(SessionData { crypt_key }: SessionData) -> Self {
        proto::EstablishSessionResponse { crypt_key }
    }
}

impl From<ConnectionState> for proto::user_status_response::Status {
    fn from(value: ConnectionState) -> Self {
        match value {
            ConnectionState::Waiting => proto::user_status_response::Status::Waiting(()),
            ConnectionState::Peered => proto::user_status_response::Status::Peered(()),
            ConnectionState::Stopped => proto::user_status_response::Status::Error(()),
        }
    }
}

fn parse<S, E>(value: &str, field_name: &'static str) -> Result<S, Status>
where
    S: FromStr<Err = E>,
    E: std::fmt::Debug,
{
    value.parse().map_err(|e| {
        tracing::warn!(error=?e, "bad {}", field_name);
        Status::invalid_argument(format!("bad {}", field_name))
    })
}

fn try_opt<T>(value: Option<T>, field_name: &'static str) -> Result<T, Status> {
    value.ok_or_else(|| {
        tracing::warn!("missing {}", field_name);
        Status::invalid_argument(format!("missing {}", field_name))
    })
}
