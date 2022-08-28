use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use anyhow::Context;
use futures_util::{Future, FutureExt};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::broadcast;
use tracing::Instrument;
use uuid::Uuid;
use ws_proto::{ClientEvent, Init, Join, Present};

use crate::ports::Ports;
use crate::server::ws::ControlStream;
use crate::util::TimeoutExt;

use super::channel::{ChannelEvent, ChannelEventKind, Channels, Chatroom};
use super::voice::{ControlChannel, VoiceConnection};

pub struct ServerSession {
    ctl: ControlStream,
    client_id: Uuid,
    client_name: String,
    udp_ports: Ports,
    channels: Channels,
    voice: Option<VoiceHandle>,
    host_info: UdpHostInfo,
}

// FIXME: remove, hard-coded to host
#[derive(Debug, Clone)]
pub enum UdpHostInfo {
    StaticIp(IpAddr),
    Host(String),
}

impl UdpHostInfo {
    pub async fn resolve(&self, port: u16) -> Result<Vec<SocketAddr>, anyhow::Error> {
        match self {
            UdpHostInfo::StaticIp(ip) => Ok(vec![(SocketAddr::from((ip.clone(), port)))]),
            UdpHostInfo::Host(host) => Ok(tokio::net::lookup_host((host.as_str(), port))
                .await?
                .collect()),
        }
    }
}

impl ServerSession {
    #[tracing::instrument(skip_all)]
    pub async fn init(
        ctl: TcpStream,
        udp_ports: Ports,
        channels: Channels,
        host_info: UdpHostInfo,
    ) -> anyhow::Result<Self> {
        let ctl = Duration::from_millis(500)
            .timeout(tokio_tungstenite::accept_async(ctl))
            .await??;
        let mut ctl = ControlStream::new(ctl);
        tracing::debug!("waiting for handshake");
        let Init { user_id, name } = ctl.await_handshake().await?;
        tracing::debug!("handshake succesful: {}", user_id);
        Ok(Self {
            ctl,
            client_id: user_id,
            client_name: name,
            udp_ports,
            channels,
            voice: None,
            host_info,
        })
    }

    #[tracing::instrument(skip(self), fields(client_id=%self.client_id, name=&self.client_name))]
    pub async fn run_session(mut self) -> anyhow::Result<()> {
        tracing::info!("session running");
        loop {
            let evt = self.ctl.next_event();
            let (handle, chan_notify) = match self.voice.as_mut() {
                Some(v) => ((&mut v.handle).boxed(), v.updates.recv().boxed()),
                None => (
                    std::future::pending().boxed(),
                    std::future::pending().boxed(),
                ),
            };
            select! {
                handle = handle => {
                    if let Err(e) = handle {
                        tracing::error!("Voice errored: {}", e);
                    }
                    self.voice = None;
                }
                evt = evt => {
                    if let Some(evt) = evt {
                        self.handle_event(evt).await?;
                    } else {
                        return Ok(());
                    }
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
        let source_id = chan_event.source_id();
        match chan_event.kind() {
            ChannelEventKind::Joined(joined) => {
                self.ctl.joined(joined.into(), source_id).await?;
            }
            ChannelEventKind::Left(left) => {
                self.ctl.left(left.into(), source_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, evt: ClientEvent) -> anyhow::Result<()> {
        match evt {
            ClientEvent::Init(_) => {
                tracing::warn!("unexpected init message");
                self.stop_voice().await;
                return Ok(());
            }
            ClientEvent::Disconnect => {
                self.stop_voice().await;
                return Ok(());
            }
            ClientEvent::Join(Join { channel_id }) => {
                tracing::info!("joining {}", channel_id);
                self.stop_voice().await;
                let room = self.channels.get_or_create(channel_id);
                let room_id = room.id();
                let room_ = room.clone();
                if let Some(voice) = self.initialize_voice_connection(room).await? {
                    let src_id = voice.source_id;
                    self.voice = Some(voice);
                    let present = room_
                        .list()
                        .into_iter()
                        .filter_map(|v| {
                            (v.source_id != src_id).then(|| Present {
                                user: v.name,
                                source_id: v.source_id,
                            })
                        })
                        .collect();
                    self.ctl.voice_ready(room_id, src_id, present).await?;
                } else {
                    self.ctl.join_error(room_id).await?;
                }
            }
            ClientEvent::Leave => {
                tracing::info!("leave");
                self.stop_voice().await;
            }
            ClientEvent::UdpAnnounce(_) => {
                tracing::warn!("unexpected udp announce message");
                self.stop_voice().await;
            }
            ClientEvent::Keepalive(_) => {
                unreachable!("Keepalive is handled by WS task");
            }
        }
        Ok(())
    }

    async fn stop_voice(&mut self) {
        if let Some(voice) = self.voice.take() {
            let _ = voice.ctl_tx.stop().await;
        }
    }

    pub async fn initialize_voice_connection(
        &mut self,
        room: Chatroom,
    ) -> anyhow::Result<Option<VoiceHandle>> {
        tracing::debug!("announcing udp");
        let port = match self.udp_ports.get() {
            Some(port) => port,
            None => {
                tracing::warn!("no free udp port");
                return Ok(None);
            }
        };
        let room_id = room.id();
        let room2 = room.clone();
        let port_ = port.port;
        let (voice, ctl_tx) =
            VoiceConnection::new(self.client_id, self.client_name.clone(), room, port).await?;
        let source_id = voice.source_id();

        let udp_addr = self
            .host_info
            .resolve(port_)
            .await
            .context("Failed to resolve")?
            .pop()
            .context("didn't get ip")?;
        tracing::debug!("udp running on {:?}", udp_addr);
        let voice = tokio::spawn(voice.run().instrument(tracing::Span::current()));
        self.ctl.announce_udp(udp_addr).await?;
        tracing::debug!("announced udp");

        tracing::debug!("waiting for client udp addr");

        let client_udp = self.ctl.await_client_udp().await?;
        let client_udp = SocketAddr::from((client_udp.ip, client_udp.port));
        tracing::debug!("received client udp addr {client_udp}");
        ctl_tx.init(client_udp).await?;
        tracing::debug!("successfully notified udp task");
        Ok(Some(VoiceHandle {
            room_id,
            handle: voice,
            ctl_tx,
            updates: room2.updates(),
            source_id,
        }))
    }
}

#[allow(unused)]
pub struct VoiceHandle {
    room_id: Uuid,
    handle: tokio::task::JoinHandle<()>,
    ctl_tx: ControlChannel,
    updates: broadcast::Receiver<ChannelEvent>,
    source_id: u32,
}

impl Future for VoiceHandle {
    type Output = anyhow::Result<()>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.handle.poll_unpin(cx).map_err(Into::into)
    }
}
