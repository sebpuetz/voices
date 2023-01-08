use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use rand::thread_rng;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::Instrument;
use udp_proto::UdpWithBuf;
use uuid::Uuid;
use voice_proto::IpDiscoveryResponse;
use voices_crypto::VoiceCrypto;
use xsalsa20poly1305::{Key, KeyInit, XSalsa20Poly1305};

use crate::ports::PortRef;

#[derive(Clone, Debug)]
pub struct VoiceControl {
    source_id: u32,
    user_name: String,
    tx: mpsc::Sender<ControlRequest>,
}

impl VoiceControl {
    pub async fn init(&self, client_addr: SocketAddr) -> anyhow::Result<InitResponse> {
        let (responder, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(
                InitRequest {
                    client_addr,
                    responder,
                }
                .into(),
            )
            .await?;
        let resp = rx.await?;
        Ok(resp)
    }

    pub async fn status(&self) -> anyhow::Result<StatusResponse> {
        let (responder, rx) = tokio::sync::oneshot::channel();
        self.tx.send(StatusRequest { responder }.into()).await?;
        let resp = rx.await?;
        Ok(resp)
    }

    pub async fn stop(&self) {
        let (responder, rx) = tokio::sync::oneshot::channel();
        match self.tx.send(StopRequest { responder }.into()).await {
            Ok(_) => {
                let _ = rx.await;
            }
            Err(_) => {
                tracing::info!("voice task already stopped");
            }
        }
    }

    pub fn source_id(&self) -> u32 {
        self.source_id
    }

    pub fn user_name(&self) -> &str {
        self.user_name.as_ref()
    }
}

#[derive(Debug)]
pub enum ControlRequest {
    /// Initialize the voice connection
    ///
    /// Takes the peer [`SocketAddr`] and returns voice connection init data like crypt key and src_id.
    Init(InitRequest),
    /// Check the status of the voice connection
    Status(StatusRequest),
    /// Stop the voice connection
    Stop(StopRequest),
}

impl From<StatusRequest> for ControlRequest {
    fn from(v: StatusRequest) -> Self {
        Self::Status(v)
    }
}

impl From<StopRequest> for ControlRequest {
    fn from(v: StopRequest) -> Self {
        Self::Stop(v)
    }
}

impl From<InitRequest> for ControlRequest {
    fn from(v: InitRequest) -> Self {
        Self::Init(v)
    }
}

#[derive(Debug)]
pub struct InitRequest {
    client_addr: SocketAddr,
    responder: tokio::sync::oneshot::Sender<InitResponse>,
}

#[derive(Debug)]
pub struct InitResponse {
    pub crypt_key: Vec<u8>,
    pub source_id: u32,
}

#[derive(Debug)]
pub struct StopRequest {
    responder: tokio::sync::oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct StatusRequest {
    responder: tokio::sync::oneshot::Sender<StatusResponse>,
}

#[derive(Debug)]
pub struct StatusResponse {
    pub state: ConnectionState,
    pub udp_addr: SocketAddr,
}

impl StatusResponse {
    pub fn peered(udp_addr: SocketAddr) -> Self {
        Self {
            state: ConnectionState::Peered,
            udp_addr,
        }
    }

    pub fn waiting(udp_addr: SocketAddr) -> Self {
        Self {
            state: ConnectionState::Waiting,
            udp_addr,
        }
    }
}

#[derive(Debug)]
pub enum ConnectionState {
    Waiting,
    Peered,
}

pub struct VoiceTask {
    handle: JoinHandle<()>,
    client_id: Uuid,
}

impl std::fmt::Debug for VoiceTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceTask")
            .field("client_id", &self.client_id)
            .finish()
    }
}

impl VoiceTask {
    pub fn new(handle: JoinHandle<()>, client_id: Uuid) -> Self {
        Self { handle, client_id }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }
}

impl std::future::Future for VoiceTask {
    type Output = Uuid;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let id = self.client_id;
        std::pin::Pin::new(&mut self.handle).poll(cx).map(|_| id)
    }
}

pub struct VoiceConnection {
    udp: UdpWithBuf,
    control_chan: mpsc::Receiver<ControlRequest>,
    voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>,
    client_id: Uuid,
    source_id: u32,
    crypt_key: Key,
    host_ip: IpAddr,
    port: PortRef,
}

impl VoiceConnection {
    pub async fn start(
        client_id: Uuid,
        user_name: String,
        port: PortRef,
        voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>,
        host_ip: IpAddr,
    ) -> anyhow::Result<(VoiceControl, VoiceTask)> {
        let sock = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port.port)))
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "Failed to bind port");
                e
            })?;
        let udp = UdpWithBuf::new(sock);
        let (tx, rx) = mpsc::channel(10);
        let source_id = rand::random();
        let crypt_key = XSalsa20Poly1305::generate_key(&mut thread_rng());
        let slf = Self {
            udp,
            control_chan: rx,
            crypt_key,
            client_id,
            source_id,
            port,
            voice_tx,
            host_ip,
        };
        let handle = tokio::spawn(slf.run().instrument(tracing::Span::current()));
        let task = VoiceTask::new(handle, client_id);
        let tx = VoiceControl { source_id, user_name, tx };
        Ok((tx, task))
    }

    #[tracing::instrument(name="voice_run", skip(self), fields(client_id=%self.client_id, source_id=%self.source_id))]
    pub async fn run(mut self) {
        // FIXME: timeout
        let mut init_timeout = Box::pin(tokio::time::sleep(Duration::from_secs(15)));
        loop {
            tokio::select! {
                packet = self.udp.recv_from() => {
                    match packet {
                        Ok((addr, msg)) => {
                            if let Err(e) = self.ip_discovery(addr, msg).await {
                                tracing::warn!("ip disco failed: {}", e);
                                return;
                            } else {
                                tracing::info!("Succesfully handled ip disco");
                            }
                        }
                        Err(e) => {
                            tracing::error!("{}", e);
                        }
                    }
                },
                addr = self.control_chan.recv() => {
                    match addr {
                        Some(ControlRequest::Init(init_msg)) => {
                            tracing::info!("Peering {}", init_msg.client_addr);
                            let _ = init_msg.responder.send(InitResponse { crypt_key: self.crypt_key.to_vec(), source_id: self.source_id }).map_err(|_| {
                                tracing::warn!("failed to send init response");
                            });
                            if let Err(e) = self.peered(init_msg.client_addr).await {
                                tracing::error!("{}", e);
                            }
                        },
                        Some(ControlRequest::Stop(stop_msg)) => {
                            tracing::info!("received stop signal");
                            let _ = stop_msg.responder.send(());
                            return;
                        },
                        Some(ControlRequest::Status(status)) => {
                            let response = StatusResponse::waiting(SocketAddr::from((self.host_ip, self.port.port)));
                            let _ = status.responder.send(response);
                            continue;
                        }
                        None => {
                            tracing::warn!("client addr announcer dropped before sending");
                            return;
                        }
                    }
                    break;
                }
                _ = &mut init_timeout => {
                    tracing::info!("client timed out");
                    return;
                }
            }
        }
    }

    async fn ip_discovery(
        &mut self,
        addr: SocketAddr,
        req: voice_proto::IpDiscoveryRequest,
    ) -> anyhow::Result<()> {
        tracing::debug!("handling discovery from {}", addr);
        let uuid = req.uuid;
        anyhow::ensure!(
            self.client_id == Uuid::from_slice(&uuid)?,
            "bad id: {:?}",
            std::str::from_utf8(&uuid)
        );
        let response = IpDiscoveryResponse {
            ip: addr.ip().to_string(),
            port: addr.port() as _,
        };
        self.udp.send_to(&response, addr).await?;
        tracing::debug!("succesfully handled discovery from {}", addr);
        Ok(())
    }

    async fn peered(mut self, addr: SocketAddr) -> anyhow::Result<()> {
        self.udp.connect(addr).await?;
        tracing::info!("successfully peered with {addr}");
        let cipher = XSalsa20Poly1305::new(&self.crypt_key);
        let source_id = self.source_id;
        let mut seq = 0;
        let mut sent = 0;
        let mut ping_deadline = Instant::now() + Duration::from_secs(15);
        let mut received = 0;
        let mut voice_rx = self.voice_tx.subscribe();
        loop {
            tokio::select! {
                    udp_pkt = self.udp.recv::<voice_proto::ClientMessage>() => {
                        match udp_pkt {
                            Ok(msg) => {
                                match msg.payload {
                                    Some(voice_proto::client_message::Payload::Voice(mut voice)) => {
                                        cipher.decrypt(&mut voice.payload, voice.sequence, voice.stream_time).map_err(|e| {
                                            tracing::error!(error=?e, "Failed to decrypt");
                                            e
                                        })?;
                                        received += 1;
                                        if voice.sequence < seq {
                                            tracing::debug!(seq, voice.sequence, "out of order voice");
                                        }
                                        seq = voice.sequence;
                                        let _ = self.voice_tx.send((source_id, voice));
                                    },
                                    Some(voice_proto::client_message::Payload::Ping(ping)) => {
                                        ping_deadline = Instant::now() + Duration::from_secs(15);
                                        // FIXME: stats are unreliable
                                        tracing::debug!(seq, received);
                                        let pong = voice_proto::ServerMessage::pong(voice_proto::Pong { seq: ping.seq, sent, received });
                                        self.udp.send(&pong).await?;
                                    },
                                    None => {
                                        tracing::warn!("client didn't send payload");
                                        return Ok(())
                                    },
                                }
                            }
                            Err(e) => return Err(e.into()),
                        }
                    },
                    recv = voice_rx.recv() => {
                        match recv {
                            // FIXME: rx_pkt_id is used to filter our own messages, would be nicer to filter that elsewhere
                            Ok((rx_pkt_id, mut voice)) => {
                                tracing::trace!("{} {}", source_id, rx_pkt_id);
                                if source_id == rx_pkt_id {
                                    continue;
                                }
                                cipher.encrypt(&mut voice.payload, voice.sequence, voice.stream_time)?;
                                let voice = voice_proto::ServerMessage::voice(voice, rx_pkt_id);
                                self.udp.send(&voice).await?;
                                sent += 1;
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("missed {n} voice packets");
                                continue;
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                anyhow::bail!("no more sending on this channel :(");
                            },
                        }
                    }
                    _ = tokio::time::sleep_until(ping_deadline.into()) => {
                        tracing::warn!("keep alive elapsed");
                        return Ok(())
                    },
                    ctl = self.control_chan.recv() => {
                        match ctl {
                            Some(ControlRequest::Init(init)) => {
                                tracing::info!("repeated init message");
                                self.udp.connect(init.client_addr).await?;
                                let _ = init.responder.send(InitResponse { crypt_key: self.crypt_key.to_vec(), source_id: self.source_id});
                            }
                            Some(ControlRequest::Status(status)) => {
                                let response = StatusResponse::peered(SocketAddr::from((self.host_ip, self.port.port)));
                                let _ = status.responder.send(response);
                                continue;
                            }
                            None => {
                                tracing::warn!("control sender dropped");
                                return Ok(())
                            },
                            Some(ControlRequest::Stop(resp)) => {
                                tracing::info!("stop reading from udp");
                                let _ = resp.responder.send(());
                                return Ok(())
                            }
                        }
                    }
            }
        }
    }
}
