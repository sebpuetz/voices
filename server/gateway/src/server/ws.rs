use std::net::SocketAddr;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::Instant;
use uuid::Uuid;
use voices_ws_proto::{
    ClientAnnounce, ClientEvent, Disconnected, Init, Initialized, JoinError, Left, Present, Ready,
    ServerAnnounce, ServerEvent,
};

use crate::util::TimeoutExt;

pub struct ControlStream {
    inc: mpsc::Receiver<ClientEvent>,
    // enum Out { WithExpectedResponse { msg: ServerEvent, resp_chan: oneshot::Sender }, Msg(ServerEvent) }
    // enables forwarding without blocking in most cases
    out: mpsc::Sender<ServerEvent>,
}

impl ControlStream {
    pub fn new(inc: mpsc::Receiver<ClientEvent>, out: mpsc::Sender<ServerEvent>) -> Self {
        Self { inc, out }
    }

    pub fn from_websocket(ws: axum::extract::ws::WebSocket) -> Self {
        let (forward_tx, forward_rx) = mpsc::channel(10);
        let (backward_tx, backward_rx) = mpsc::channel(10);
        tokio::spawn(async move {
            let fut = ControlStreamPriv::new(ws, forward_tx, backward_rx).run();
            match fut.await {
                Ok(_) => {
                    tracing::info!("WS task closed normally");
                }
                Err(e) => {
                    tracing::error!("WS task errored out: {}", e);
                }
            }
        });
        Self::new(forward_rx, backward_tx)
    }

    async fn send(&self, evt: ServerEvent) -> Result<(), ControlStreamError> {
        self.out
            .send(evt)
            .await
            .map_err(|_| ControlStreamError::DeadReceiver)
    }

    pub async fn initialized(&self, session_id: Uuid) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::Init(Initialized { session_id }))
            .await
    }

    pub async fn announce_udp(
        &self,
        socket: SocketAddr,
        source_id: u32,
    ) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::UdpAnnounce(ServerAnnounce {
            ip: socket.ip(),
            port: socket.port(),
            source_id,
        }))
        .await
    }

    pub async fn joined(&self, user: String, source_id: u32) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::Joined(Present { user, source_id }))
            .await
    }

    pub async fn disconnected(&self) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::Disconnected(Disconnected {})).await
    }

    pub async fn join_error(&self, room_id: Uuid) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::JoinError(dbg!(JoinError { room_id })))
            .await
    }

    pub async fn left(&self, user: String, source_id: u32) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::Left(Left { user, source_id })).await
    }

    pub async fn voice_ready(&self, ready: Ready) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::Ready(ready)).await
    }

    pub async fn next_event(&mut self) -> Option<ClientEvent> {
        self.inc.recv().await
    }

    pub async fn await_handshake(&mut self) -> Result<Init, ControlStreamError> {
        match self.next_event().await.ok_or(ControlStreamError::End)? {
            ClientEvent::Init(init) => Ok(init),
            evt => Err(ControlStreamError::UnexpectedMessage {
                expected: "Init",
                got: evt.into(),
            }),
        }
    }

    pub async fn await_client_udp(&mut self) -> Result<ClientAnnounce, ControlStreamError> {
        match self.next_event().await.ok_or(ControlStreamError::End)? {
            ClientEvent::UdpAnnounce(announce) => Ok(announce),
            evt => Err(ControlStreamError::UnexpectedMessage {
                expected: "UdpAnnounce",
                got: evt.into(),
            }),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ControlStreamError {
    #[error("outgoing WS channel receiver died")]
    DeadReceiver,
    #[error("incoming WS channel senders are gone")]
    End,
    #[error("expected {expected}, received {got}")]
    UnexpectedMessage {
        expected: &'static str,
        got: &'static str,
    },
}

struct ControlStreamPriv {
    ws: axum::extract::ws::WebSocket,
    forward: mpsc::Sender<ClientEvent>,
    back: mpsc::Receiver<ServerEvent>,
    send_timeout: Duration,
    keepalive: Duration,
}

impl ControlStreamPriv {
    pub fn new(
        ws: axum::extract::ws::WebSocket,
        forward: mpsc::Sender<ClientEvent>,
        back: mpsc::Receiver<ServerEvent>,
    ) -> Self {
        Self {
            ws,
            send_timeout: Duration::from_millis(500),
            forward,
            back,
            keepalive: Duration::from_millis(2500),
        }
    }

    async fn run(mut self) -> Result<(), PrivControlStreamError> {
        let mut last = Instant::now();
        loop {
            let deadline = tokio::time::sleep_until(last + self.keepalive);
            let ws = self.ws.next();
            let back = self.back.recv();
            select! {
                ws_msg = ws => {
                    let msg = ws_msg.ok_or_else(|| {
                        self.back.close();
                        PrivControlStreamError::StreamClosed
                    })??;
                    match self.handle_incoming_message(msg).await? {
                        ControlFlow::Continue => {
                            last = Instant::now();
                            continue
                        },
                        ControlFlow::Stop => {
                            let _ = self
                            .send_timeout
                            .timeout(self.ws.close()).await;
                        return Ok(())
                    }
                    }
                }
                back_msg = back => {
                    if let Some(back_msg) = back_msg {
                        self.send_json(&back_msg).await?;
                    }
                }
                _ = deadline => {
                    tracing::info!("ws deadline for client elapsed, closing");
                    self.back.close();
                    let _ = self.send_timeout.timeout(self.ws.close()).await;
                    return Ok(())
                }
            }
        }
    }

    async fn handle_incoming_message(
        &mut self,
        message: axum::extract::ws::Message,
    ) -> Result<ControlFlow, PrivControlStreamError> {
        if matches!(message, axum::extract::ws::Message::Close(_)) {
            tracing::debug!("received close");
            return Ok(ControlFlow::Stop);
        }
        let evt =
            Self::decode_json(&message).map_err(|e| PrivControlStreamError::deser(message, e))?;
        if let ClientEvent::Keepalive(ka) = evt {
            self.send_json(&ServerEvent::Keepalive(ka)).await?;
        } else {
            self.forward.send(evt).await?;
        }
        Ok(ControlFlow::Continue)
    }

    fn decode_json(message: &axum::extract::ws::Message) -> Result<ClientEvent, serde_json::Error> {
        let bytes = match message {
            axum::extract::ws::Message::Text(string) => string.as_bytes(),
            axum::extract::ws::Message::Binary(data)
            | axum::extract::ws::Message::Ping(data)
            | axum::extract::ws::Message::Pong(data) => data,
            axum::extract::ws::Message::Close(None) => &[],
            axum::extract::ws::Message::Close(Some(frame)) => frame.reason.as_bytes(),
        };
        serde_json::from_slice(bytes)
    }

    async fn send_json<V: Serialize>(&mut self, payload: &V) -> Result<(), PrivControlStreamError> {
        let msg = axum::extract::ws::Message::Text(serde_json::to_string(payload).unwrap());
        self.send_timeout
            .timeout(self.ws.send(msg))
            .await
            .map_err(|_| PrivControlStreamError::SendTimeout)?
            .map_err(PrivControlStreamError::AxumError)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum PrivControlStreamError {
    #[error("timeout sending message")]
    SendTimeout,
    #[error("stream closed")]
    StreamClosed,
    #[error("stream error: {0}")]
    AxumError(#[from] axum::Error),
    #[error("deser error: {msg:?}, {src:?}")]
    DeserError {
        msg: axum::extract::ws::Message,
        #[source]
        src: serde_json::Error,
    },
    #[error("internal receiver went away")]
    NoReceiver(#[from] mpsc::error::SendError<ClientEvent>),
}

impl PrivControlStreamError {
    fn deser(msg: axum::extract::ws::Message, e: serde_json::Error) -> Self {
        PrivControlStreamError::DeserError { msg, src: e }
    }
}

enum ControlFlow {
    Continue,
    Stop,
}
