pub mod server;
mod util;
pub mod voice;

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use futures_util::SinkExt;
use prost::Message as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use server::voice::{Chatroom, ControlMessage, InitChan, VoiceConnection};
use tokio::net::{TcpStream, UdpSocket};
use tokio::select;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_tungstenite::WebSocketStream;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;
use util::TimeoutExt;
use uuid::Uuid;

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let tcp_addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    rt.block_on(main_(tcp_addr)).map_err(|e| {
        tracing::error!("{}", e);
        e
    })?;
    Ok(())
}

async fn main_(tcp_addr: SocketAddr) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let ctl_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    let chatroom = Chatroom::new();
    loop {
        let (inc, addr) = ctl_listener.accept().await?;
        tracing::debug!("accepted tcp connection from {}", addr);
        let room = chatroom.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_session(inc, addr, room).await {
                tracing::warn!("{}", e);
            }
        });
    }
}

async fn handle_session(inc: TcpStream, addr: SocketAddr, room: Chatroom) -> anyhow::Result<()> {
    println!("{:?}", addr);
    let sess = ServerSession::new(inc)
        .await?
        .await_handshake()
        .await
        .map_err(|e| e.state)?
        .announce_udp(room)
        .await
        .map_err(|e| e.error)?
        .await_client_udp()
        .await
        .map_err(|e| e.error)?
        .run_session()
        .await
        .map_err(|e| e.error);
    Ok(())
}

pub struct ServerSession<State> {
    ctl: ServerControlStream,
    state: State,
}

impl ServerSession<Uninit> {
    pub async fn new(ctl: TcpStream) -> anyhow::Result<Self> {
        let ctl = ServerControlStream::new(ctl, Duration::from_millis(500)).await?;
        Ok(Self { ctl, state: Uninit })
    }

    #[tracing::instrument(skip(self))]
    pub async fn await_handshake(
        self,
    ) -> Result<ServerSession<ClientHandshake>, ServerSession<anyhow::Error>> {
        tracing::debug!("waiting for handshake");
        let Self { mut ctl, state: _ } = self;
        let state = match ctl.await_handshake().await {
            Ok(Init { id }) => ClientHandshake { user_id: id },
            Err(_) => todo!(),
        };
        tracing::debug!("handshake succesful: {}", state.user_id);
        Ok(ServerSession { state, ctl })
    }
}

impl ServerSession<ClientHandshake> {
    #[tracing::instrument(skip(self, room))]
    pub async fn announce_udp(
        self,
        room: Chatroom,
    ) -> Result<ServerSession<WaitClientUdp>, SessionError> {
        tracing::debug!("announcing udp");
        let Self {
            mut ctl,
            state: ClientHandshake { user_id },
        } = self;
        let (voice, tx) = match VoiceConnection::new(user_id.parse().unwrap(), room).await {
            Ok(ok) => ok,
            Err(e) => return Err(SessionError::new(ctl, e)),
        };
        let udp_addr = match voice.addr() {
            Ok(ok) => ok,
            Err(e) => return Err(SessionError::new(ctl, e)),
        };
        tracing::debug!("udp running on {}", udp_addr);
        tokio::spawn(voice.run());
        if let Err(e) = ctl.announce_voice(udp_addr).await {
            return Err(SessionError::new(ctl, e));
        }
        tracing::debug!("announced udp");
        Ok(ServerSession {
            ctl,
            state: WaitClientUdp { user_id, tx },
        })
    }
}

impl ServerSession<WaitClientUdp> {
    #[tracing::instrument(skip(self))]
    pub async fn await_client_udp(self) -> Result<ServerSession<Initialized>, SessionError> {
        tracing::debug!("waiting for client udp addr");
        let mut ctl = self.ctl;
        let user_id = self.state.user_id;
        let client_udp = match ctl.await_client_udp().await {
            Ok(ok) => ok,
            Err(e) => {
                return Err(SessionError::new(ctl, e));
            }
        };
        let client_udp = SocketAddr::from((client_udp.ip, client_udp.port));
        tracing::debug!("received client udp addr {client_udp}");
        let voice_ctl_tx = match self.state.tx.send(client_udp) {
            Ok(ctl_chan) => ctl_chan,
            Err(e) => {
                tracing::error!("{}", e);
                return Err(SessionError::new(ctl, e));
            }
        };
        tracing::debug!("successfully notified udp task");
        let id = Uuid::new_v4();
        if let Err(e) = ctl.ready(id).await {
            return Err(SessionError::new(ctl, e));
        }
        Ok(ServerSession {
            ctl,
            state: Initialized {
                id,
                user_id,
                voice_ctl_tx,
            },
        })
    }
}

impl ServerSession<Initialized> {
    #[tracing::instrument(skip(self))]
    pub async fn run_session(mut self) -> Result<(), SessionError> {
        loop {
            match self.ctl.next_event().await {
                Ok(Event::Keepalive { sent_at }) => {
                    tracing::trace!("Received Keepalive");
                    if let Err(e) = self.ctl.respond_keepalive(sent_at).await {
                        return Err(SessionError::new(self.ctl, e));
                    }
                }
                Err(e) => {
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
                    return Err(SessionError::new(self.ctl, e));
                }
            }
        }
    }
}

pub struct Initialized {
    id: Uuid,
    user_id: String,
    voice_ctl_tx: mpsc::Sender<ControlMessage>,
}

pub struct SessionError {
    ctl: ServerControlStream,
    udp: Option<UdpSocket>,
    error: anyhow::Error,
}

impl SessionError {
    pub fn new(ctl: ServerControlStream, error: impl Into<anyhow::Error>) -> Self {
        Self {
            ctl,
            udp: None,
            error: error.into(),
        }
    }
    pub fn with_udp(mut self, udp: UdpSocket) -> Self {
        self.udp = Some(udp);
        self
    }
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
}

pub struct Uninit;
pub struct Error;
pub struct ClientHandshake {
    user_id: String,
}

pub struct WaitClientUdp {
    user_id: String,
    tx: InitChan,
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
