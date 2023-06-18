use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::channel_registry::ChannelRegistry;
use crate::server::channels::state::ChannelState;
use crate::AppState;

#[tracing::instrument("get servers", skip(state))]
pub async fn servers<S, R>(
    State(state): State<AppState<S, R>>,
) -> Result<Json<GetServersResponse>, ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry();

    // FIXME: pagination params via API
    let (servers, pages) = registry.get_servers(None, None).await?;
    Ok(Json(GetServersResponse {
        info: servers
            .into_iter()
            .map(|v| ServerInfo {
                server_id: v.id.to_string(),
                name: v.name,
            })
            .collect(),
        pages,
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
    let registry = state.channels.registry();
    let server_id = registry.create_server(req.name).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateServerResponse { server_id }),
    ))
}

#[derive(Deserialize, Debug)]
pub struct CreateServerRequest {
    name: String,
}

#[derive(Serialize)]
pub struct CreateServerResponse {
    server_id: Uuid,
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
    let registry = state.channels.registry();
    let response = registry
        .get_server(id)
        .await?
        .ok_or_else(|| ApiError::NotFound)?;
    Ok(Json(GetServerResponse {
        server_id: response.id,
        name: response.name,
        channels: response
            .channels
            .into_iter()
            .map(|v| GetChannelResponse {
                channel_id: v.id,
                name: v.name,
            })
            .collect(),
    }))
}

#[derive(Serialize)]
pub struct GetServerResponse {
    server_id: Uuid,
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
    let registry = state.channels.registry();
    let channel_id = registry
        .create_channel(req.server_id, req.name)
        .await?
        .ok_or_else(|| ApiError::NotFound)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateChannelResponse { channel_id }),
    ))
}

#[derive(Deserialize, Debug)]
pub struct CreateChannelRequest {
    server_id: Uuid,
    name: String,
}

#[derive(Serialize)]
pub struct CreateChannelResponse {
    channel_id: Uuid,
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
    let registry = state.channels.registry();
    let response = registry
        .get_channel(id)
        .await?
        .ok_or_else(|| ApiError::NotFound)?;
    // FIXME: return who's present here, tricky right now because `get_voice` also initializes a voice server.
    // should change `channels` to provide a listing interface
    Ok(Json(GetChannelResponse {
        channel_id: response.info.id,
        name: response.info.name,
    }))
}

#[derive(Serialize)]
pub struct GetChannelResponse {
    channel_id: Uuid,
    name: String,
}

pub async fn cleanup_stale_voice_servers<S, R>(
    State(state): State<AppState<S, R>>,
) -> Result<Json<CleanupStaleVoiceServersResponse>, ApiError>
where
    S: ChannelState,
    R: ChannelRegistry,
{
    let registry = state.channels.registry();
    let deleted = registry.cleanup_stale_voice_servers().await?;
    Ok(Json(CleanupStaleVoiceServersResponse { deleted }))
}

#[derive(Serialize)]
pub struct CleanupStaleVoiceServersResponse {
    deleted: Vec<Uuid>,
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

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        ApiError::InternalServerError(value)
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
