use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use channel::Chatroom;
use ports::PortRef;
use rand::thread_rng;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use udp_proto::UdpWithBuf;
use uuid::Uuid;
use voice_proto::*;
use voices_crypto::VoiceCrypto;
use xsalsa20poly1305::{Key, KeyInit, XSalsa20Poly1305};

pub mod channel;
mod ports;
pub use ports::Ports;

#[derive(Clone)]
pub struct VoiceServer {
    host_addr: IpAddr,
    ports: Ports,
}

impl VoiceServer {
    pub fn new(host_addr: IpAddr, ports: Ports) -> Self {
        Self { host_addr, ports }
    }

    pub async fn connection(
        &self,
        client_id: Uuid,
        client_name: String,
        chatroom: Chatroom,
    ) -> Result<(VoiceConnection, VoiceControl), SetupError> {
        let port = self.ports.get().ok_or(SetupError::NoOpenPorts)?;
        VoiceConnection::new(client_id, client_name, chatroom, port, self.host_addr)
            .await
            .map_err(Into::into)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SetupError {
    #[error("no remaining udp ports")]
    NoOpenPorts,
    #[error(transparent)]
    UdpError(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct InitMessage {
    client_addr: SocketAddr,
}

#[derive(Debug)]
pub enum VoiceControlMessage {
    Init(InitMessage),
    Stop,
}

pub struct VoiceControl(mpsc::Sender<VoiceControlMessage>);

impl VoiceControl {
    pub async fn init(
        &self,
        client_udp: SocketAddr,
    ) -> Result<(), mpsc::error::SendError<VoiceControlMessage>> {
        self.0
            .send(VoiceControlMessage::Init(InitMessage {
                client_addr: client_udp,
            }))
            .await
    }

    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<VoiceControlMessage>> {
        self.0.send(VoiceControlMessage::Stop).await
    }
}

pub struct VoiceConnection {
    chatroom: Chatroom,
    udp: UdpWithBuf,
    udp_addr: SocketAddr,
    client_info: ClientInfo,
    control_chan: mpsc::Receiver<VoiceControlMessage>,
    crypt_key: Key,
    _port: PortRef,
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
        host_addr: IpAddr,
    ) -> Result<(Self, VoiceControl), SetupError> {
        let sock = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port.port))).await?;
        let udp = UdpWithBuf::new(sock);
        let (tx, rx) = mpsc::channel(10);
        let source_id = rand::random();
        let crypt_key = XSalsa20Poly1305::generate_key(&mut thread_rng());

        let client_info = ClientInfo {
            client_id,
            source_id,
            name: client_name,
        };
        Ok((
            VoiceConnection {
                chatroom,
                udp,
                udp_addr: SocketAddr::from((host_addr, port.port)),
                client_info,
                control_chan: rx,
                crypt_key,
                _port: port,
            },
            VoiceControl(tx),
        ))
    }

    pub fn client_id(&self) -> Uuid {
        self.client_info.client_id
    }

    pub fn source_id(&self) -> u32 {
        self.client_info.source_id
    }

    pub fn udp_addr(&self) -> SocketAddr {
        self.udp_addr
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
                        Some(VoiceControlMessage::Init(init_msg)) => {
                            tracing::info!("Peering {}", init_msg.client_addr);
                            if let Err(e) = self.peered(init_msg.client_addr).await {
                                tracing::error!("{}", e);
                            }
                        },
                        Some(VoiceControlMessage::Stop) => {
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
        let cipher = XSalsa20Poly1305::new(&self.crypt_key);
        let source_id = self.source_id();
        let mut seq = 0;
        let mut channel_handle = self.chatroom.join(self.client_info);
        let id = channel_handle.id();
        let mut sent = 0;
        let mut ping_deadline = Instant::now() + Duration::from_secs(15);
        let mut received = 0;
        loop {
            tokio::select! {
                udp_pkt = self.udp.recv::<ClientMessage>() => {
                    match udp_pkt {
                        Ok(msg) => {
                            match msg.payload {
                                Some(client_message::Payload::Voice(mut voice)) => {
                                    cipher.decrypt(&mut voice.payload, voice.sequence, voice.stream_time).map_err(|e| {
                                        tracing::error!(error=?e, "Failed to decrypt");
                                        e
                                    })?;
                                    received += 1;
                                    if voice.sequence < seq {
                                        tracing::debug!(seq, voice.sequence, "out of order voice");
                                    }
                                    seq = voice.sequence;
                                    let _ = channel_handle.send(voice);
                                },
                                Some(client_message::Payload::Ping(ping)) => {
                                    ping_deadline += Duration::from_secs(15);
                                    // FIXME: stats are unreliable
                                    tracing::debug!(seq, received);
                                    let pong = ServerMessage::pong(Pong { seq: ping.seq, sent, received });
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
                recv = channel_handle.recv() => {
                    match recv {
                        // FIXME: rx_pkt_id is used to filter our own messages, would be nicer to filter that elsewhere
                        Ok((rx_pkt_id, mut voice)) => {
                            tracing::trace!("{} {}", id, rx_pkt_id);
                            if source_id == rx_pkt_id {
                                continue;
                            }
                            cipher.encrypt(&mut voice.payload, voice.sequence, voice.stream_time)?;
                            let voice = ServerMessage::voice(voice, rx_pkt_id);
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
                    if matches!(ctl, None | Some(VoiceControlMessage::Stop)) {
                        tracing::info!("stop reading from udp");
                        return Ok(())
                    }
                }
            }
        }
    }

    pub fn crypt_key(&self) -> &[u8] {
        &self.crypt_key
    }
}

// FIXME: probably obsolete with opus?
// pub struct SampleRateChecker {
//     interval: Duration,
//     last: Instant,
//     n_bytes: usize,
// }

// impl SampleRateChecker {
//     pub fn new(interval: Duration) -> Self {
//         Self {
//             interval,
//             last: Instant::now(),
//             n_bytes: 0,
//         }
//     }

//     pub fn check(&mut self, read: usize) -> Option<usize> {
//         self.n_bytes += read;
//         let elapsed = self.last.elapsed();
//         if elapsed > self.interval {
//             let samples = self.n_bytes / 2;
//             let elapsed_ms = elapsed.as_millis() as usize;
//             let sec_factor = 1000. / elapsed_ms as f32;
//             let sample_rate = samples as f32 * sec_factor;
//             tracing::trace!("checking samplerate: {}", sample_rate);
//             if sample_rate > (16000. * 1.3) {
//                 return Some(sample_rate as usize);
//             }
//             self.n_bytes = 0;
//             self.last = Instant::now();
//         }
//         None
//     }
// }
