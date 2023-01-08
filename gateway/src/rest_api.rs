use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use voices_channels::grpc::proto::channels_server::Channels;

use crate::server::channel_registry::ChannelRegistry;
use crate::server::channel_state::ChannelState;
use crate::AppState;

#[tracing::instrument("get servers", skip(state))]
pub async fn servers<S, R>(
    State(state): State<AppState<S, R>>,
) -> Result<Json<GetServersResponse>, ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry().registry_handle();
    // FIXME: pagination params via API
    let message = voices_channels::grpc::proto::GetServersRequest {
        page: None,
        per_page: None,
    };
    let request = tonic::Request::new(message);
    let resp = registry.get_servers(request).await?.into_inner();
    Ok(Json(GetServersResponse {
        info: resp
            .info
            .into_iter()
            .map(|v| ServerInfo {
                server_id: v.server_id,
                name: v.name,
            })
            .collect(),
        pages: resp.pages,
    }))
}

#[derive(Serialize)]
pub struct ServerInfo {
    server_id: String,
    name: String,
}

#[derive(Serialize)]
pub struct GetServersResponse {
    info: Vec<ServerInfo>,
    pages: i64,
}

#[tracing::instrument("new server", skip(state))]
pub async fn new_server<S, R>(
    State(state): State<AppState<S, R>>,
    Json(req): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<CreateServerResponse>), ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry().registry_handle();
    let message = voices_channels::grpc::proto::CreateServerRequest { name: req.name };
    let request = tonic::Request::new(message);
    let resp = registry.create_server(request).await?.into_inner();
    Ok((
        StatusCode::CREATED,
        Json(CreateServerResponse {
            server_id: resp.server_id,
        }),
    ))
}

#[derive(Deserialize, Debug)]
pub struct CreateServerRequest {
    name: String,
}

#[derive(Serialize)]
pub struct CreateServerResponse {
    server_id: String,
}

#[tracing::instrument("new channel", skip(state))]
pub async fn get_server<S, R>(
    Path(id): Path<Uuid>,
    State(state): State<AppState<S, R>>,
) -> Result<Json<GetServerResponse>, ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry().registry_handle();
    let message = voices_channels::grpc::proto::GetServerRequest {
        server_id: id.to_string(),
    };
    let request = tonic::Request::new(message);
    let response = registry.get_server(request).await?.into_inner();
    Ok(Json(GetServerResponse {
        server_id: response.server_id,
        name: response.name,
        channels: response
            .channels
            .into_iter()
            .map(|v| GetChannelResponse {
                channel_id: v.channel_id,
                name: v.name,
            })
            .collect(),
    }))
}

#[derive(Serialize)]
pub struct GetServerResponse {
    server_id: String,
    name: String,
    channels: Vec<GetChannelResponse>,
}

#[tracing::instrument("new channel", skip(state))]
pub async fn new_channel<S, R>(
    State(state): State<AppState<S, R>>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<CreateChannelResponse>), ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry().registry_handle();
    let message = voices_channels::grpc::proto::CreateChannelRequest {
        server_id: req.server_id.to_string(),
        name: req.name,
    };
    let request = tonic::Request::new(message);
    let resp = registry.create_channel(request).await?.into_inner();
    Ok((
        StatusCode::CREATED,
        Json(CreateChannelResponse {
            channel_id: resp.channel_id,
        }),
    ))
}

#[derive(Deserialize, Debug)]
pub struct CreateChannelRequest {
    server_id: Uuid,
    name: String,
}

#[derive(Serialize)]
pub struct CreateChannelResponse {
    channel_id: String,
}

#[tracing::instrument("new channel", skip(state))]
pub async fn get_channel<S, R>(
    Path(id): Path<Uuid>,
    State(state): State<AppState<S, R>>,
) -> Result<Json<GetChannelResponse>, ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry().registry_handle();
    let message = voices_channels::grpc::proto::GetChannelRequest {
        channel_id: id.to_string(),
    };
    let request = tonic::Request::new(message);
    let response = registry.get_channel(request).await?.into_inner();
    // FIXME: return who's present here, tricky right now because `get_voice` also initializes a voice server.
    // should change `channels` to provide a listing interface
    Ok(Json(GetChannelResponse {
        channel_id: response.channel_id,
        name: response.name,
    }))
}

#[derive(Serialize)]
pub struct GetChannelResponse {
    channel_id: String,
    name: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error(transparent)]
    InternalServerError(anyhow::Error),
    #[error("resource not found")]
    NotFound,
    #[error("{source}")]
    WithBody {
        message: serde_json::Value,
        #[source]
        source: Box<Self>,
    },
}

impl From<tonic::Status> for ApiError {
    fn from(value: tonic::Status) -> Self {
        match value.code() {
            tonic::Code::NotFound => ApiError::NotFound,
            tonic::Code::Ok
            | tonic::Code::Cancelled
            | tonic::Code::Unknown
            | tonic::Code::InvalidArgument
            | tonic::Code::DeadlineExceeded
            | tonic::Code::AlreadyExists
            | tonic::Code::PermissionDenied
            | tonic::Code::ResourceExhausted
            | tonic::Code::FailedPrecondition
            | tonic::Code::Aborted
            | tonic::Code::OutOfRange
            | tonic::Code::Unimplemented
            | tonic::Code::Internal
            | tonic::Code::Unavailable
            | tonic::Code::DataLoss
            | tonic::Code::Unauthenticated => {
                ApiError::InternalServerError(anyhow::Error::from(value))
            }
        }
    }
}

impl ApiError {
    pub fn with_body<S: Serialize>(self, s: S) -> Result<Self, Self> {
        Ok(Self::WithBody {
            message: serde_json::to_value(s).map_err(|e| Self::InternalServerError(e.into()))?,
            source: Box::new(self),
        })
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::WithBody { message: _, source } => source.status_code(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        tracing::info!("error response: {:#}", self);
        if let ApiError::WithBody { message, source } = self {
            let mut resp = Json(message).into_response();
            *resp.status_mut() = source.status_code();
            resp
        } else {
            self.status_code().into_response()
        }
    }
}
