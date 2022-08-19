use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use anyhow::Context;
use futures_util::SinkExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

use crate::ports::Ports;
use crate::util::TimeoutExt;

use super::voice::{Channels, Chatroom, ControlChannel, VoiceConnection};

pub struct ServerSession {
    ctl: ServerControlStream,
    client_id: Uuid,
    udp_ports: Ports,
    channels: Channels,
    voice: Option<VoiceHandle>,
}

impl ServerSession {
    #[tracing::instrument(skip(ctl, channels))]
    pub async fn await_handshake(
        ctl: TcpStream,
        udp_ports: Ports,
        channels: Channels,
    ) -> anyhow::Result<Self> {
        let mut ctl = ServerControlStream::new(ctl, Duration::from_millis(500)).await?;
        tracing::debug!("waiting for handshake");
        let client_id = ctl.await_handshake().await?.id.parse()?;
        tracing::debug!("handshake succesful: {}", client_id);
        Ok(Self {
            ctl,
            client_id,
            udp_ports,
            channels,
            voice: None,
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn run_session(mut self) -> anyhow::Result<()> {
        tracing::info!("session running");
        loop {
            match self.ctl.next_event().await {
                Ok(Event::Keepalive { sent_at }) => {
                    tracing::trace!("Received Keepalive");
                    self.ctl.respond_keepalive(sent_at).await?
                }
                Ok(Event::Join { channel_id }) => {
                    tracing::info!("joining {}", channel_id);
                    if let Some(voice) = self.voice.take() {
                        let _ = voice.ctl_tx.stop().await;
                    }
                    let room = self.channels.get_or_create(channel_id);
                    let room_id = room.id();
                    let voice = self.initialize_voice_connection(room).await?;
                    self.voice = Some(voice);
                    self.ctl.ready(room_id).await?;
                }
                Ok(Event::Leave) => {
                    tracing::info!("leave");
                    if let Some(voice) = self.voice.take() {
                        let _ = voice.ctl_tx.stop().await;
                    }
                }
                Err(e) => {
                    tracing::error!("error: {}", e);
                    if e.is_timeout() {
                        tracing::info!("Session timed out");
                        self.ctl.close().await;
                        return Ok(());
                    }
                    if e.is_closed() {
                        tracing::info!("Client closed session");
                        return Ok(());
                    }
                    tracing::error!("Error waiting for event {}", e);
                    return Err(e.into());
                }
            }
        }
    }

    #[tracing::instrument(skip(self, room))]
    pub async fn initialize_voice_connection(
        &mut self,
        room: Chatroom,
    ) -> anyhow::Result<VoiceHandle> {
        tracing::debug!("announcing udp");
        let port = self.udp_ports.get().context("no open udp ports")?;
        let room_id = room.id();
        let (voice, ctl_tx) = VoiceConnection::new(self.client_id, room, port).await?;
        let udp_addr = voice.addr()?;
        tracing::debug!("udp running on {}", udp_addr);
        let voice = tokio::spawn(voice.run());
        self.ctl.announce_voice(udp_addr).await?;
        tracing::debug!("announced udp");

        tracing::debug!("waiting for client udp addr");

        let client_udp = self.ctl.await_client_udp().await?;
        let client_udp = SocketAddr::from((client_udp.ip, client_udp.port));
        tracing::debug!("received client udp addr {client_udp}");
        ctl_tx.init(client_udp).await?;
        tracing::debug!("successfully notified udp task");
        Ok(VoiceHandle {
            room_id,
            handle: voice,
            ctl_tx,
        })
    }
}

#[allow(unused)]
pub struct VoiceHandle {
    room_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
    ctl_tx: ControlChannel,
}
pub struct ServerControlStream {
    inner: ControlStream,
}

impl ServerControlStream {
    pub async fn new(inner: TcpStream, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            inner: ControlStream::new(inner, timeout).await?,
        })
    }

    pub async fn await_handshake(&mut self) -> Result<Init, anyhow::Error> {
        self.inner.recv_json::<Init>().await.map_err(Into::into)
    }

    pub async fn announce_voice(&mut self, udp_addr: SocketAddr) -> anyhow::Result<()> {
        self.inner
            .send_json(&Announce {
                ip: udp_addr.ip(),
                port: udp_addr.port(),
            })
            .await?;
        Ok(())
    }

    pub async fn await_client_udp(&mut self) -> anyhow::Result<Announce> {
        self.inner.recv_json().await.map_err(Into::into)
    }

    pub async fn ready(&mut self, id: Uuid) -> anyhow::Result<()> {
        self.inner.send_json(&Ready { id }).await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<Event, ControlStreamError> {
        self.inner.recv_json().await
    }

    pub async fn respond_keepalive(&mut self, sent_at: u64) -> Result<(), ControlStreamError> {
        self.inner.send_json(&Event::Keepalive { sent_at }).await?;
        Ok(())
    }

    pub async fn close(self) {
        let _ = self.inner.close();
    }
}

#[derive(Deserialize, Serialize)]
pub enum Event {
    Keepalive { sent_at: u64 },
    Join { channel_id: Uuid },
    Leave,
}

#[derive(Serialize, Deserialize)]
pub struct Init {
    id: String,
}

#[derive(Serialize, Deserialize)]
pub struct Announce {
    ip: IpAddr,
    port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct Ready {
    id: Uuid,
}

pub struct ControlStream {
    inner: WebSocketStream<TcpStream>,
    timeout: Duration,
}

impl ControlStream {
    pub async fn new(inner: TcpStream, timeout: Duration) -> anyhow::Result<Self> {
        let inner = timeout
            .timeout(tokio_tungstenite::accept_async(inner))
            .await??;
        Ok(Self { inner, timeout })
    }
}

impl ControlStream {
    async fn recv_json<V: DeserializeOwned>(&mut self) -> Result<V, ControlStreamError> {
        let msg = self
            .timeout
            .timeout(self.inner.next())
            .await
            .map_err(|_| ControlStreamError::RecvTimeout)?
            .ok_or(ControlStreamError::StreamClosed)?
            .map_err(ControlStreamError::StreamError)?;
        if msg.is_close() {
            return Err(ControlStreamError::StreamClosed);
        }
        Ok(serde_json::from_slice(msg.as_bytes()).unwrap())
    }

    async fn send_json<V: Serialize>(&mut self, payload: &V) -> Result<(), ControlStreamError> {
        let msg = tungstenite::Message::Text(serde_json::to_string(payload).unwrap());
        self.timeout
            .timeout(self.inner.send(msg))
            .await
            .map_err(|_| ControlStreamError::SendTimeout)?
            .map_err(ControlStreamError::StreamError)?;
        Ok(())
    }

    async fn close(mut self) -> anyhow::Result<()> {
        self.inner.close(None).await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ControlStreamError {
    #[error("timeout waiting for message")]
    RecvTimeout,
    #[error("timeout sending message")]
    SendTimeout,
    #[error("stream closed")]
    StreamClosed,
    #[error("stream error: {0}")]
    StreamError(tungstenite::Error),
    #[error("invalid message")]
    InvalidMessage {
        msg: tungstenite::Message,
        src: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
    #[error("deser error")]
    DeserError {
        msg: tungstenite::Message,
        src: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl ControlStreamError {
    pub fn is_timeout(&self) -> bool {
        use ControlStreamError::*;
        matches!(self, RecvTimeout | SendTimeout)
    }

    pub fn is_closed(&self) -> bool {
        use ControlStreamError::*;
        matches!(self, StreamClosed)
    }
}

trait MessageExt {
    fn as_bytes(&self) -> &[u8];
}

impl MessageExt for tungstenite::Message {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(string) => string.as_bytes(),
            Self::Binary(data) | Self::Ping(data) | Self::Pong(data) => data,
            Self::Close(None) => &[],
            Self::Close(Some(frame)) => frame.reason.as_bytes(),
            Self::Frame(frame) => frame.payload(),
        }
    }
}
