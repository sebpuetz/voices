use std::io;
use std::net::SocketAddr;

use prost::Message;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
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
    udp: UdpSocket,
    client_id: Uuid,
    control_chan: mpsc::Receiver<ControlMessage>,
    buf: [u8; 1500],
    _port_ref: PortRef,
}

impl VoiceConnection {
    pub fn addr(&self) -> Result<SocketAddr, io::Error> {
        // FIXME: want to return outside addr here
        self.udp.local_addr()
    }

    pub async fn new(
        client_id: Uuid,
        chatroom: Chatroom,
        port: PortRef,
    ) -> anyhow::Result<(Self, ControlChannel)> {
        let udp = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port.port))).await?;
        let (tx, rx) = mpsc::channel(10);
        Ok((
            VoiceConnection {
                chatroom,
                udp,
                client_id,
                control_chan: rx,
                buf: [0; 1500],
                _port_ref: port,
            },
            ControlChannel(tx),
        ))
    }

    #[tracing::instrument(skip(self))]
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                packet = self.udp.recv_from(&mut self.buf) => {
                    match packet {
                        Err(e) => {
                            tracing::error!("{}", e);
                        }
                        Ok((len, sock)) => {
                            // tracing::debug!("read {len}  {:?}", self.buf);
                            if let Err(e) = self.ip_discovery(len, sock, self.client_id).await {
                                tracing::warn!("ip disco failed: {}\nread {len}, {:?}", e, &self.buf[..len]);
                            } else {
                                tracing::info!("Succesfully handled ip disco");
                            }
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

    async fn peered(mut self, addr: SocketAddr) -> anyhow::Result<()> {
        self.udp.connect(addr).await?;
        tracing::info!("successfully peered with {addr}");
        let mut channel_handle = self.chatroom.join(self.client_id);
        let id = channel_handle.id();
        let mut sent = 0;
        loop {
            tokio::select! {
                udp_pkt = self.udp.recv(&mut self.buf) => {
                    match udp_pkt {
                        Ok(len) => {
                            let voice = ClientMessage::decode(&self.buf[..len])?;
                            match voice.payload {
                                Some(client_message::Payload::Voice(voice)) => {
                                    let _ = channel_handle.send(voice);
                                },
                                Some(client_message::Payload::Ping(ping)) => {
                                    let pong = ServerMessage::pong(Pong { seq: ping.seq, sent });
                                    pong.encode(&mut self.buf.as_mut())?;
                                    self.udp.send(&self.buf[..pong.encoded_len()]).await?;
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
                            if id == pkt_id {
                                continue;
                            }
                            let voice = ServerMessage::voice(voice, pkt_id);
                            voice.encode(&mut self.buf.as_mut_slice())?;
                            self.udp.send(&self.buf[..voice.encoded_len()]).await?;
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

    async fn ip_discovery(
        &mut self,
        buf_len: usize,
        addr: SocketAddr,
        expected_id: Uuid,
    ) -> anyhow::Result<()> {
        tracing::debug!("handling discovery from {}", addr);
        let IpDiscoveryRequest { uuid } = IpDiscoveryRequest::decode(&self.buf[..buf_len])?;
        anyhow::ensure!(
            expected_id == Uuid::from_slice(&uuid)?,
            "bad id: {:?}",
            std::str::from_utf8(&uuid)
        );
        let response = IpDiscoveryResponse {
            ip: addr.ip().to_string(),
            port: addr.port() as _,
        };
        response.encode(&mut self.buf.as_mut_slice())?;
        let len = response.encoded_len();
        // let addr = SocketAddr::from(([127, 0, 0, 1], 12345));
        self.udp.send_to(&self.buf[..len], addr).await?;
        tracing::debug!("succesfully handled discovery from {}", addr);
        Ok(())
    }
}

pub struct DiscoverRequest {}

impl ServerMessage {
    pub fn new(msg: server_message::Message) -> Self {
        ServerMessage { message: Some(msg) }
    }

    pub fn pong(pong: Pong) -> Self {
        ServerMessage::new(server_message::Message::Pong(pong))
    }

    pub fn voice(voice: Voice, src_id: Uuid) -> Self {
        ServerMessage::new(server_message::Message::Voice(ServerVoice {
            sequence: voice.sequence,
            source_id: src_id.as_bytes().to_vec(),
            payload: voice.payload,
        }))
    }
}
