use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::select;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Instant};
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;
use voices_ws_proto::*;

pub struct ControlStream {
    inc: mpsc::Receiver<ServerEvent>,
    // enum Out { WithExpectedResponse { msg: ServerEvent, resp_chan: oneshot::Sender }, Msg(ServerEvent) }
    // enables forwarding without blocking in most cases
    out: mpsc::Sender<ClientEvent>,
    stop_tx: mpsc::Sender<oneshot::Sender<()>>,
}

impl ControlStream {
    pub fn new<T>(ws: WebSocketStream<T>) -> Self
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let (forward_tx, forward_rx) = mpsc::channel(10);
        let (backward_tx, backward_rx) = mpsc::channel(10);
        let (stop_tx, stop_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let fut = ControlStreamPriv::new(ws, forward_tx, backward_rx, stop_rx).run();
            if let Err(e) = fut.await {
                tracing::error!("WS task errored out: {}", e);
            } else {
                tracing::info!("WS task closed normally");
            }
        });
        Self {
            inc: forward_rx,
            out: backward_tx,
            stop_tx,
        }
    }
    async fn send(&self, evt: ClientEvent) -> Result<(), ControlStreamError> {
        self.out
            .send(evt)
            .await
            .map_err(|_| ControlStreamError::DeadReceiver)
    }

    pub async fn init(&mut self, user_id: Uuid, name: String) -> Result<(), ControlStreamError> {
        self.send(ClientEvent::Init(Init { user_id, name })).await
    }

    pub async fn announce_udp(&self, socket: SocketAddr) -> Result<(), ControlStreamError> {
        self.send(ClientEvent::UdpAnnounce(Announce {
            ip: socket.ip(),
            port: socket.port(),
        }))
        .await
    }

    pub async fn join(&self, channel_id: Uuid) -> Result<(), ControlStreamError> {
        self.send(ClientEvent::Join(Join { channel_id })).await
    }
    pub async fn leave(&self) -> Result<(), ControlStreamError> {
        self.send(ClientEvent::Leave).await
    }

    pub async fn next_event(&mut self) -> Result<ServerEvent, ControlStreamError> {
        self.inc.recv().await.ok_or(ControlStreamError::End)
    }

    pub async fn await_voice_udp(&mut self) -> Result<Announce, ControlStreamError> {
        match self.next_event().await? {
            ServerEvent::UdpAnnounce(init) => Ok(init),
            evt => Err(ControlStreamError::UnexpectedMessage {
                expected: "Announce",
                got: evt,
            }),
        }
    }

    pub async fn await_ready(&mut self) -> Result<Ready, ControlStreamError> {
        match self.next_event().await? {
            ServerEvent::Ready(ready) => Ok(ready),
            evt => Err(ControlStreamError::UnexpectedMessage {
                expected: "Ready",
                got: evt,
            }),
        }
    }

    pub async fn stop(self) -> Result<(), ControlStreamError> {
        let (tx, rx) = oneshot::channel();
        timeout(Duration::from_millis(50), self.stop_tx.send(tx))
            .await
            .map_err(|_| ControlStreamError::ShutdownError)?
            .map_err(|_| ControlStreamError::ShutdownError)?;
        timeout(Duration::from_millis(50), rx)
            .await
            .map_err(|_| ControlStreamError::ShutdownError)?
            .map_err(|_| ControlStreamError::ShutdownError)?;
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ControlStreamError {
    #[error("outgoing WS channel receiver died")]
    DeadReceiver,
    #[error("incoming WS channel senders are gone")]
    End,
    #[error("expected {expected}, received {got:?}")]
    UnexpectedMessage {
        expected: &'static str,
        got: ServerEvent,
    },
    #[error("graceful shutdown failed")]
    ShutdownError,
}

struct ControlStreamPriv<T> {
    ws: WebSocketStream<T>,
    forward: mpsc::Sender<ServerEvent>,
    back: mpsc::Receiver<ClientEvent>,
    send_timeout: Duration,
    keepalive: Duration,
    stop_rx: mpsc::Receiver<oneshot::Sender<()>>,
}

impl<T> ControlStreamPriv<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(
        ws: WebSocketStream<T>,
        forward: mpsc::Sender<ServerEvent>,
        back: mpsc::Receiver<ClientEvent>,
        stop_rx: mpsc::Receiver<oneshot::Sender<()>>,
    ) -> Self {
        Self {
            ws,
            send_timeout: Duration::from_millis(500),
            forward,
            back,
            keepalive: Duration::from_millis(2000),
            stop_rx,
        }
    }

    #[tracing::instrument(skip_all)]
    async fn run(mut self) -> Result<(), PrivControlStreamError> {
        let mut last = Instant::now();
        loop {
            let deadline = tokio::time::sleep_until(last + self.keepalive);
            let ws = self.ws.next();
            let back = self.back.recv();
            let stop = self.stop_rx.recv();
            select! {
                ws_msg = ws => {
                    let msg = ws_msg.ok_or_else(|| {
                        self.back.close();
                        PrivControlStreamError::StreamClosed
                    })??;
                    if msg.is_close() {
                        timeout(self.send_timeout, self.ws.close(None))
                            .await
                            .map_err(|_| PrivControlStreamError::SendTimeout)??;
                        tracing::info!("server closed WS");
                        break;
                    }
                    let evt = msg
                        .json()
                        .map_err(|e| PrivControlStreamError::deser(msg, e))?;
                    if let ServerEvent::Keepalive(ka) = evt {
                        tracing::trace!("WS Pong: {}", ka.diff().as_secs_f32());
                    } else {
                        self.forward.send(evt).await?;
                    }
                    continue;
                },
                back_msg = back => {
                    match back_msg {
                        Some(msg) => {
                            tracing::debug!("Sending message {:?}", msg);
                            self.send_json(&msg).await?;
                        },
                        None => {
                            tracing::debug!("Internal WS sender stopped");
                            self.close().await?;
                            break;
                        }
                    }
                }
                stop = stop => {
                    self.close().await?;
                    if let Some(rep) = stop {
                        let _ = rep.send(());
                    }
                    return Ok(())
                }
                _ = deadline => {
                    tracing::trace!("Sending keepalive");
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap();
                    let ka = ClientEvent::Keepalive(Keepalive{sent_at: now.as_millis() as _});
                    self.send_json(&ka).await?;
                }
            }
            last = Instant::now();
        }
        Ok(())
    }

    async fn close(mut self) -> Result<(), PrivControlStreamError> {
        timeout(self.send_timeout, self.ws.close(None))
            .await
            .map_err(|_| PrivControlStreamError::SendTimeout)??;
        Ok(())
    }

    async fn send_json<V>(&mut self, payload: &V) -> Result<(), PrivControlStreamError>
    where
        V: Serialize,
    {
        let msg = tungstenite::Message::Text(serde_json::to_string(payload).unwrap());

        timeout(self.send_timeout, self.ws.send(msg))
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
    NoReceiver(#[from] mpsc::error::SendError<ServerEvent>),
}

impl PrivControlStreamError {
    fn deser(msg: tungstenite::Message, e: serde_json::Error) -> Self {
        PrivControlStreamError::DeserError { msg, src: e }
    }
}
