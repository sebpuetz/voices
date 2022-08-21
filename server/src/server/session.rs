use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use futures_util::FutureExt;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::broadcast;
use uuid::Uuid;
use ws_proto::{ClientEvent, Join};

use crate::ports::Ports;
use crate::server::ws::ControlStream;
use crate::util::TimeoutExt;

use super::channel::{ChannelEvent, ChannelEventKind, Channels, Chatroom};
use super::voice::{ControlChannel, VoiceConnection};
use super::ws::ControlStreamError;

pub struct ServerSession {
    ctl: ControlStream,
    client_id: Uuid,
    udp_ports: Ports,
    channels: Channels,
    voice: Option<VoiceHandle>,
}

impl ServerSession {
    #[tracing::instrument(skip(ctl, channels))]
    pub async fn init(
        ctl: TcpStream,
        udp_ports: Ports,
        channels: Channels,
    ) -> anyhow::Result<Self> {
        let ctl = Duration::from_millis(500)
            .timeout(tokio_tungstenite::accept_async(ctl))
            .await??;
        let mut ctl = ControlStream::new(ctl);
        tracing::debug!("waiting for handshake");
        let client_id = ctl.await_handshake().await?.user_id;
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
            let evt = self.ctl.next_event();
            let chan_notify = self
                .voice
                .as_mut()
                .map(|v| v.updates.recv().boxed())
                .unwrap_or_else(|| std::future::pending().boxed());
            select! {
                evt = evt => {
                    self.handle_event(evt).await?;
                },
                Ok(chan_event) = chan_notify => {
                    self.handle_chan_event(chan_event).await?;
                },
            }
        }
    }

    async fn handle_chan_event(&mut self, chan_event: ChannelEvent) -> anyhow::Result<()> {
        if self.client_id == chan_event.source() {
            return Ok(());
        }
        match chan_event.kind() {
            ChannelEventKind::Joined(joined) => {
                self.ctl.joined(*joined).await?;
            }
            ChannelEventKind::Left(left) => {
                self.ctl.left(*left).await?;
            }
        }
        Ok(())
    }

    async fn handle_event(
        &mut self,
        evt: Result<ClientEvent, ControlStreamError>,
    ) -> anyhow::Result<()> {
        match evt {
            Ok(ClientEvent::Init(_)) => {
                tracing::warn!("unexpected init message");
                self.stop_voice().await;
                return Ok(());
            }
            Ok(ClientEvent::Disconnect) => {
                self.stop_voice().await;
                return Ok(());
            }
            Ok(ClientEvent::Join(Join { channel_id })) => {
                tracing::info!("joining {}", channel_id);
                self.stop_voice().await;
                let room = self.channels.get_or_create(channel_id);
                let room_id = room.id();
                let voice = self.initialize_voice_connection(room).await?;
                self.voice = Some(voice);
                self.ctl.voice_ready(room_id).await?;
            }
            Ok(ClientEvent::Leave) => {
                tracing::info!("leave");
                self.stop_voice().await;
            }
            Ok(ClientEvent::UdpAnnounce(_)) => {
                tracing::warn!("unexpected udp announce message");
                self.stop_voice().await;
            }
            Ok(ClientEvent::Keepalive(_)) => {
                unreachable!("Keepalive is handled by WS task");
            }
            Err(e) => {
                tracing::error!("Control stream error: {}", e);
                return Err(e.into());
            }
        }
        Ok(())
    }

    async fn stop_voice(&mut self) {
        if let Some(voice) = self.voice.take() {
            let _ = voice.ctl_tx.stop().await;
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
        let room2 = room.clone();
        let (voice, ctl_tx) = VoiceConnection::new(self.client_id, room, port).await?;
        let udp_addr = voice.addr()?;
        tracing::debug!("udp running on {}", udp_addr);
        let voice = tokio::spawn(voice.run());
        self.ctl.announce_udp(udp_addr).await?;
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
            updates: room2.updates(),
        })
    }
}

#[allow(unused)]
pub struct VoiceHandle {
    room_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
    ctl_tx: ControlChannel,
    updates: broadcast::Receiver<ChannelEvent>,
}
// pub struct ServerControlStream {
//     inner: ControlStream,
// }

// impl ServerControlStream {
//     pub async fn new(inner: TcpStream, timeout: Duration) -> anyhow::Result<Self> {
//         Ok(Self {
//             inner: ControlStream::new(inner, timeout).await?,
//         })
//     }

//     pub async fn await_handshake(&mut self) -> Result<Init, anyhow::Error> {
//         self.inner.recv_json::<Init>().await.map_err(Into::into)
//     }

//     pub async fn announce_voice(&mut self, udp_addr: SocketAddr) -> anyhow::Result<()> {
//         self.inner
//             .send_json(&Announce {
//                 ip: udp_addr.ip(),
//                 port: udp_addr.port(),
//             })
//             .await?;
//         Ok(())
//     }

//     pub async fn await_client_udp(&mut self) -> anyhow::Result<Announce> {
//         self.inner.recv_json().await.map_err(Into::into)
//     }

//     pub async fn ready(&mut self, id: Uuid) -> anyhow::Result<()> {
//         self.inner.send_json(&Ready { id }).await?;
//         Ok(())
//     }

//     pub async fn next_event(&mut self) -> Result<Event, ControlStreamError> {
//         self.inner.recv_json().await
//     }

//     pub async fn respond_keepalive(&mut self, sent_at: u64) -> Result<(), ControlStreamError> {
//         self.inner.send_json(&Event::Keepalive { sent_at }).await?;
//         Ok(())
//     }

//     pub async fn close(self) {
//         let _ = self.inner.close();
//     }
// }

// #[derive(Deserialize, Serialize)]
// pub enum Event {
//     Keepalive { sent_at: u64 },
//     Join { channel_id: Uuid },
//     Leave,
// }

// #[derive(Serialize, Deserialize)]
// pub struct Init {
//     id: String,
// }

// #[derive(Serialize, Deserialize)]
// pub struct Announce {
//     ip: IpAddr,
//     port: u16,
// }

// #[derive(Serialize, Deserialize)]
// pub struct Ready {
//     id: Uuid,
// }

// pub struct ControlStream {
//     inner: WebSocketStream<TcpStream>,
//     timeout: Duration,
// }

// impl ControlStream {
//     pub async fn new(inner: TcpStream, timeout: Duration) -> anyhow::Result<Self> {
//         let inner = timeout
//             .timeout(tokio_tungstenite::accept_async(inner))
//             .await??;
//         Ok(Self { inner, timeout })
//     }
// }

// impl ControlStream {
//     async fn recv_json<V: DeserializeOwned>(&mut self) -> Result<V, ControlStreamError> {
//         let msg = self
//             .timeout
//             .timeout(self.inner.next())
//             .await
//             .map_err(|_| ControlStreamError::RecvTimeout)?
//             .ok_or(ControlStreamError::StreamClosed)?
//             .map_err(ControlStreamError::StreamError)?;
//         if msg.is_close() {
//             return Err(ControlStreamError::StreamClosed);
//         }
//         Ok(serde_json::from_slice(msg.as_bytes()).unwrap())
//     }

//     async fn send_json<V: Serialize>(&mut self, payload: &V) -> Result<(), ControlStreamError> {
//         let msg = tungstenite::Message::Text(serde_json::to_string(payload).unwrap());
//         self.timeout
//             .timeout(self.inner.send(msg))
//             .await
//             .map_err(|_| ControlStreamError::SendTimeout)?
//             .map_err(ControlStreamError::StreamError)?;
//         Ok(())
//     }

//     async fn close(mut self) -> anyhow::Result<()> {
//         self.inner.close(None).await?;
//         Ok(())
//     }
// }

// #[derive(Debug, thiserror::Error)]
// pub enum ControlStreamError {
//     #[error("timeout waiting for message")]
//     RecvTimeout,
//     #[error("timeout sending message")]
//     SendTimeout,
//     #[error("stream closed")]
//     StreamClosed,
//     #[error("stream error: {0}")]
//     StreamError(tungstenite::Error),
//     #[error("invalid message")]
//     InvalidMessage {
//         msg: tungstenite::Message,
//         src: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
//     },
//     #[error("deser error")]
//     DeserError {
//         msg: tungstenite::Message,
//         src: Box<dyn std::error::Error + Send + Sync + 'static>,
//     },
// }

// impl ControlStreamError {
//     pub fn is_timeout(&self) -> bool {
//         use ControlStreamError::*;
//         matches!(self, RecvTimeout | SendTimeout)
//     }

//     pub fn is_closed(&self) -> bool {
//         use ControlStreamError::*;
//         matches!(self, StreamClosed)
//     }
// }
