use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;
use ws_proto::{
    Announce, ClientEvent, Disconnected, Init, JoinError, Left, MessageExt, Present, Ready,
    ServerEvent,
};

use crate::util::TimeoutExt;

pub struct ControlStream {
    inc: mpsc::Receiver<ClientEvent>,
    // enum Out { WithExpectedResponse { msg: ServerEvent, resp_chan: oneshot::Sender }, Msg(ServerEvent) }
    // enables forwarding without blocking in most cases
    out: mpsc::Sender<ServerEvent>,
}

impl ControlStream {
    pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
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
        Self {
            inc: forward_rx,
            out: backward_tx,
        }
    }

    async fn send(&self, evt: ServerEvent) -> Result<(), ControlStreamError> {
        self.out
            .send(evt)
            .await
            .map_err(|_| ControlStreamError::DeadReceiver)
    }

    pub async fn announce_udp(&self, socket: SocketAddr) -> Result<(), ControlStreamError> {
        self.send(ServerEvent::UdpAnnounce(Announce {
            ip: socket.ip(),
            port: socket.port(),
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

    pub async fn await_client_udp(&mut self) -> Result<Announce, ControlStreamError> {
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
    ws: WebSocketStream<TcpStream>,
    forward: mpsc::Sender<ClientEvent>,
    back: mpsc::Receiver<ServerEvent>,
    send_timeout: Duration,
    keepalive: Duration,
}

impl ControlStreamPriv {
    pub fn new(
        ws: WebSocketStream<TcpStream>,
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
                        ControlFlow::Stop => return Ok(()),
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
                    let _ = self.send_timeout.timeout(self.ws.close(None)).await;
                    return Ok(())
                }
            }
        }
    }

    async fn handle_incoming_message(
        &mut self,
        message: tungstenite::Message,
    ) -> Result<ControlFlow, PrivControlStreamError> {
        if message.is_close() {
            tracing::debug!("received close");
            match self
                .send_timeout
                .timeout(self.ws.close(None))
                .await
                .map_err(|_| PrivControlStreamError::SendTimeout)?
            {
                Ok(_) | Err(tungstenite::Error::ConnectionClosed) => {
                    return Ok(ControlFlow::Stop);
                }
                Err(e) => return Err(e.into()),
            }
        }
        let evt = message
            .json()
            .map_err(|e| PrivControlStreamError::deser(message, e))?;
        if let ClientEvent::Keepalive(ka) = evt {
            self.send_json(&ServerEvent::Keepalive(ka)).await?;
        } else {
            self.forward.send(evt).await?;
        }
        Ok(ControlFlow::Continue)
    }

    async fn send_json<V: Serialize>(&mut self, payload: &V) -> Result<(), PrivControlStreamError> {
        let msg = tungstenite::Message::Text(serde_json::to_string(payload).unwrap());
        self.send_timeout
            .timeout(self.ws.send(msg))
            .await
            .map_err(|_| PrivControlStreamError::SendTimeout)?
            .map_err(PrivControlStreamError::StreamError)?;
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
    StreamError(#[from] tungstenite::Error),
    #[error("deser error: {msg}, {src:?}")]
    DeserError {
        msg: tungstenite::Message,
        #[source]
        src: serde_json::Error,
    },
    #[error("internal receiver went away")]
    NoReceiver(#[from] mpsc::error::SendError<ClientEvent>),
}

impl PrivControlStreamError {
    fn deser(msg: tungstenite::Message, e: serde_json::Error) -> Self {
        PrivControlStreamError::DeserError { msg, src: e }
    }
}

enum ControlFlow {
    Continue,
    Stop,
}
