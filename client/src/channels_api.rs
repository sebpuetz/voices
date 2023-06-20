use anyhow::ensure;
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct Client {
    inner: reqwest::blocking::Client,
    base_url: Url,
}

impl Client {
    const SERVERS: &'static str = "servers";

    pub fn new(base_url: Url) -> anyhow::Result<Self> {
        ensure!(
            !base_url.cannot_be_a_base(),
            "base url needs to be usable as base"
        );
        Ok(Client {
            inner: reqwest::blocking::Client::new(),
            base_url,
        })
    }

    pub fn get_servers(&self) -> Result<GetServersResponse, RequestError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            // invariant is checked in the constructor
            .expect("base_url is a valid base")
            .push(Self::SERVERS);
        let response = self
            .inner
            .get(url)
            .send()
            .map_err(|e| RequestError::Retriable(e.into()))?;
        Self::handle_response(response)
    }

    pub fn get_server(&self, id: Uuid) -> Result<GetServerResponse, RequestError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            // invariant is checked in the constructor
            .expect("base_url is a valid base")
            .push(Self::SERVERS)
            .push(&id.to_string());
        let response = self
            .inner
            .get(url)
            .send()
            .map_err(|e| RequestError::Retriable(e.into()))?;
        Self::handle_response(response)
    }

    fn handle_response<V: DeserializeOwned>(
        response: reqwest::blocking::Response,
    ) -> Result<V, RequestError> {
        let status = response.status();
        let body = response.bytes().map_err(|e| {
            tracing::warn!("failed to read response body: {}", e);
            RequestError::Retriable(anyhow::Error::from(e))
        })?;
        if status.is_success() {
            return serde_json::from_slice(&body).map_err(|e| {
                let body = std::str::from_utf8(&body).unwrap_or("NOT_VALID_UTF8");
                tracing::warn!("failed to parse response '{}': {}", body, e);
                RequestError::Fatal(anyhow::anyhow!("failed to parse response body"))
            });
        }
        let body = std::str::from_utf8(&body).unwrap_or("NOT_VALID_UTF8");

        if status.is_client_error() {
            Err(RequestError::Fatal(anyhow::anyhow!(
                "client error response: {}",
                body
            )))
        } else {
            Err(RequestError::Retriable(anyhow::anyhow!(
                "client error response: {}",
                body
            )))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub server_id: Uuid,
    pub name: String,
}

#[derive(Deserialize)]
pub struct GetServersResponse {
    pub info: Vec<ServerInfo>,
    pub pages: i64,
}

#[derive(Deserialize)]
pub struct GetServerResponse {
    pub server_id: Uuid,
    pub name: String,
    pub channels: Vec<GetChannelResponse>,
}

#[derive(Debug, Deserialize)]
pub struct GetChannelResponse {
    pub channel_id: Uuid,
    pub name: String,
    pub present: Vec<ClientInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub client_id: String,
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("retriable error making request: {0}")]
    Retriable(anyhow::Error),
    #[error("fatal error making request: {0}")]
    Fatal(anyhow::Error),
}
