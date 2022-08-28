use std::net::{Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc};
use udp_proto::UdpWithBuf;
use uuid::Uuid;

use crate::{ports::PortRef, voice::*};

use super::channel::Chatroom;

#[derive(Debug)]
pub struct InitMessage {
    client_addr: SocketAddr,
}

#[derive(Debug)]
pub enum ControlMessage {
    Init(InitMessage),
    Stop,
}

pub struct ControlChannel(mpsc::Sender<ControlMessage>);

impl ControlChannel {
    pub async fn init(
        &self,
        client_udp: SocketAddr,
    ) -> Result<(), mpsc::error::SendError<ControlMessage>> {
        self.0
            .send(ControlMessage::Init(InitMessage {
                client_addr: client_udp,
            }))
            .await
    }

    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<ControlMessage>> {
        self.0.send(ControlMessage::Stop).await
    }
}

pub struct VoiceConnection {
    chatroom: Chatroom,
    udp: UdpWithBuf,
    client_info: ClientInfo,
    control_chan: mpsc::Receiver<ControlMessage>,
    _port_ref: PortRef,
}

#[derive(Clone, Debug)]
pub struct ClientInfo {
    pub client_id: Uuid,
    pub source_id: u32,
    pub name: String,
}

impl VoiceConnection {
    pub async fn new(
        client_id: Uuid,
        client_name: String,
        chatroom: Chatroom,
        port: PortRef,
    ) -> anyhow::Result<(Self, ControlChannel)> {
        let udp = UdpWithBuf::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port.port))).await?;
        let (tx, rx) = mpsc::channel(10);
        let source_id = rand::random();
        let client_info = ClientInfo {
            client_id,
            source_id,
            name: client_name,
        };
        Ok((
            VoiceConnection {
                chatroom,
                udp,
                client_info,
                control_chan: rx,
                _port_ref: port,
            },
            ControlChannel(tx),
        ))
    }

    pub fn client_id(&self) -> Uuid {
        self.client_info.client_id
    }

    pub fn source_id(&self) -> u32 {
        self.client_info.source_id
    }

    #[tracing::instrument(name="voice_run", skip(self), fields(source_id=%self.source_id()))]
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                packet = self.udp.recv_from() => {
                    match packet {
                        Ok((addr, msg)) => {
                            if let Err(e) = self.ip_discovery(addr, msg).await {
                                tracing::warn!("ip disco failed: {}", e);
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
                        Some(ControlMessage::Init(init_msg)) => {
                            tracing::info!("Peering {}", init_msg.client_addr);
                            if let Err(e) = self.peered(init_msg.client_addr).await {
                                tracing::error!("{}", e);
                            }
                        },
                        Some(ControlMessage::Stop) => {
                            tracing::info!("received stop signal");
                            return;
                        },
                        None => {
                            tracing::warn!("client addr announcer dropped before sending");
                            return;
                        }
                    }
                    break;
                }
            }
        }
    }

    async fn ip_discovery(
        &mut self,
        addr: SocketAddr,
        req: IpDiscoveryRequest,
    ) -> anyhow::Result<()> {
        tracing::debug!("handling discovery from {}", addr);
        let uuid = req.uuid;
        anyhow::ensure!(
            self.client_id() == Uuid::from_slice(&uuid)?,
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
        let source_id = self.source_id();
        let mut channel_handle = self.chatroom.join(self.client_info);
        let id = channel_handle.id();
        let mut sent = 0;
        let mut checker = SampleRateChecker::new(Duration::from_secs(5));
        loop {
            tokio::select! {
                udp_pkt = self.udp.recv::<ClientMessage>() => {
                    match udp_pkt {
                        Ok(msg) => {
                            match msg.payload {
                                Some(client_message::Payload::Voice(voice)) => {
                                    if let Some(bad_rate) = checker.check(voice.payload.len()) {
                                        tracing::warn!("Bad sample rate: {}", bad_rate);
                                        return Ok(())
                                    }
                                    let _ = channel_handle.send(voice);
                                },
                                Some(client_message::Payload::Ping(ping)) => {
                                    let pong = ServerMessage::pong(Pong { seq: ping.seq, sent });
                                    self.udp.send(&pong).await?;
                                },
                                None => todo!(),
                            }
                        }
                        Err(e) => return Err(e.into()),
                    }
                },
                recv = channel_handle.recv() => {
                    match recv {
                        Ok((pkt_id, voice)) => {
                            tracing::trace!("{} {}", id, pkt_id);
                            if source_id == pkt_id {
                                continue;
                            }
                            let voice = ServerMessage::voice(voice, pkt_id);
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
                ctl = self.control_chan.recv() => {
                    if matches!(ctl, None | Some(ControlMessage::Stop)) {
                        tracing::info!("stop reading from udp");
                        return Ok(())
                    }
                }
            }
        }
    }
}

pub struct SampleRateChecker {
    interval: Duration,
    last: Instant,
    n_bytes: usize,
}

impl SampleRateChecker {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: Instant::now(),
            n_bytes: 0,
        }
    }

    pub fn check(&mut self, read: usize) -> Option<usize> {
        self.n_bytes += read;
        let elapsed = self.last.elapsed();
        if elapsed > self.interval {
            let samples = self.n_bytes / 2;
            let elapsed_ms = elapsed.as_millis() as usize;
            let sec_factor = 1000. / elapsed_ms as f32;
            let sample_rate = samples as f32 * sec_factor;
            tracing::trace!("checking samplerate: {}", sample_rate);
            if sample_rate > (16000. * 1.3) {
                return Some(sample_rate as usize);
            }
            self.n_bytes = 0;
            self.last = Instant::now();
        }
        None
    }
}

impl ServerMessage {
    pub fn new(msg: server_message::Message) -> Self {
        ServerMessage { message: Some(msg) }
    }

    pub fn pong(pong: Pong) -> Self {
        ServerMessage::new(server_message::Message::Pong(pong))
    }

    pub fn voice(voice: Voice, src_id: u32) -> Self {
        ServerMessage::new(server_message::Message::Voice(ServerVoice {
            sequence: voice.sequence,
            source_id: src_id,
            payload: voice.payload,
        }))
    }
}
