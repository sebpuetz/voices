pub mod connection;

use std::collections::BTreeMap;
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures_util::stream::FuturesUnordered;
use futures_util::FutureExt;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::StreamExt;
use udp_proto::UdpWithBuf;
use uuid::Uuid;
use voice_proto::*;

use crate::ports::PortRef;
use crate::{
    ChannelNotFound, ConnectionState, EstablishSessionError, OpenConnectionError, Peer,
    PeerNotFound, StatusError,
};

use self::connection::{InitResponse, StatusResponse, VoiceConnection, VoiceControl, VoiceTask};

#[derive(Clone, Debug)]
pub struct Channel {
    channel_ctl: mpsc::Sender<ChannelCtl>,
    port: Arc<PortRef>,
}

impl Channel {
    pub const CHANNEL_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

    pub async fn new(
        room_id: Uuid,
        end_notify: mpsc::Sender<Uuid>,
        port: PortRef,
    ) -> anyhow::Result<Self> {
        let sock_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, port.port));
        let socket = UdpWithBuf::bind(sock_addr).await?;
        let (fwd, channel_ctl) = Forwarder::new(socket, end_notify, room_id);
        tokio::spawn(async move {
            fwd.run().await;
            tracing::info!("forwarder exitted")
        });
        Ok(Self {
            channel_ctl,
            port: Arc::new(port),
        })
    }

    pub fn port(&self) -> u16 {
        self.port.port
    }

    pub async fn establish_session(
        &self,
        client_id: Uuid,
        client_addr: SocketAddr,
    ) -> Result<InitResponse, EstablishSessionError> {
        let (msg, rx) = ChannelCtl::establish(client_id, client_addr);
        self.channel_ctl.send(msg).await.map_err(|_| {
            EstablishSessionError::Other(anyhow::anyhow!("failed to send channel request"))
        })?;

        rx.await.map_err(|_| {
            tracing::warn!("failed to establish session");
            EstablishSessionError::Other(anyhow::anyhow!("failed to open connection"))
        })?
    }

    #[tracing::instrument(skip_all)]
    pub async fn open_connection(
        &self,
        client_id: Uuid,
        user_name: String,
    ) -> Result<u32, OpenConnectionError> {
        let (msg, rx) = ChannelCtl::open_connection(client_id, user_name);
        self.channel_ctl
            .send(msg)
            .await
            .map_err(|_| ChannelNotFound)?;
        let resp = rx.await.map_err(|e| {
            tracing::warn!("failed to open connection: {}", e);
            OpenConnectionError::Other(anyhow::anyhow!("failed to open connection"))
        })?;
        Ok(resp)
    }

    pub async fn peers(&self) -> anyhow::Result<Vec<Peer>> {
        let (msg, rx) = ChannelCtl::peers();
        self.channel_ctl.send(msg).await?;
        let resp = rx.await?;
        Ok(resp)
    }

    pub async fn status(&self, client_id: Uuid) -> Result<StatusResponse, StatusError> {
        let (msg, rx) = ChannelCtl::status(client_id);
        self.channel_ctl
            .send(msg)
            .await
            .context("failed sending channel request")?;
        rx.await.context("status response failed")?
    }

    pub async fn leave(&self, client_id: Uuid) -> Option<()> {
        let (msg, rx) = ChannelCtl::leave(client_id);
        let _ = self.channel_ctl.send(msg).await;
        rx.await.ok()
    }

    pub fn healthy(&self) -> bool {
        !self.channel_ctl.is_closed()
    }
}

#[derive(Debug)]
enum ChannelCtl {
    OpenConnection {
        client_id: Uuid,
        user_name: String,
        responder: oneshot::Sender<u32>,
    },
    Establish {
        client_id: Uuid,
        client_addr: SocketAddr,
        responder: oneshot::Sender<Result<InitResponse, EstablishSessionError>>,
    },
    Leave {
        client_id: Uuid,
        responder: oneshot::Sender<()>,
    },
    Peers {
        responder: oneshot::Sender<Vec<Peer>>,
    },
    Status {
        client_id: Uuid,
        responder: oneshot::Sender<Result<StatusResponse, StatusError>>,
    },
}

impl ChannelCtl {
    fn open_connection(client_id: Uuid, user_name: String) -> (Self, oneshot::Receiver<u32>) {
        let (responder, rx) = oneshot::channel();
        (
            Self::OpenConnection {
                client_id,
                user_name,
                responder,
            },
            rx,
        )
    }

    fn establish(
        client_id: Uuid,
        client_addr: SocketAddr,
    ) -> (
        Self,
        oneshot::Receiver<Result<InitResponse, EstablishSessionError>>,
    ) {
        let (responder, rx) = oneshot::channel();
        (
            Self::Establish {
                client_id,
                client_addr,
                responder,
            },
            rx,
        )
    }

    fn leave(client_id: Uuid) -> (Self, oneshot::Receiver<()>) {
        let (responder, rx) = oneshot::channel();
        (
            Self::Leave {
                client_id,
                responder,
            },
            rx,
        )
    }

    fn peers() -> (Self, oneshot::Receiver<Vec<Peer>>) {
        let (responder, rx) = oneshot::channel();
        (Self::Peers { responder }, rx)
    }

    fn status(client_id: Uuid) -> (Self, oneshot::Receiver<Result<StatusResponse, StatusError>>) {
        let (responder, rx) = oneshot::channel();
        (
            Self::Status {
                client_id,
                responder,
            },
            rx,
        )
    }
}

struct Forwarder {
    socket: UdpWithBuf,
    control: mpsc::Receiver<ChannelCtl>,
    connections: BTreeMap<u32, Conn>,
    client_id_to_src_id: BTreeMap<Uuid, u32>,
    voice_tx: broadcast::Sender<(u32, voice_proto::Voice)>,
    tasks: FuturesUnordered<VoiceTask>,
    end_notify: mpsc::Sender<Uuid>,
    room_id: Uuid,
}

impl Forwarder {
    fn new(
        socket: UdpWithBuf,
        end_notify: mpsc::Sender<Uuid>,
        room_id: Uuid,
    ) -> (Self, mpsc::Sender<ChannelCtl>) {
        let (control_tx, control) = mpsc::channel(100);
        let (voice_tx, _) = broadcast::channel(10);
        (
            Forwarder {
                socket,
                control,
                connections: Default::default(),
                client_id_to_src_id: Default::default(),
                tasks: Default::default(),
                voice_tx,
                end_notify,
                room_id,
            },
            control_tx,
        )
    }

    async fn run(mut self) {
        loop {
            let timeout = if self.tasks.is_empty() {
                tracing::info!("timeout for {} started", self.room_id);
                tokio::time::sleep(Channel::CHANNEL_IDLE_TIMEOUT).boxed()
            } else {
                std::future::pending().boxed()
            };
            tokio::select! {
                evt = self.event() => {
                    match evt {
                        Ok(true) => continue,
                        Ok(false) => break,
                        Err(e) => {
                            tracing::info!("{}", e);
                        }
                    }
                }
                _ = timeout => {
                    tracing::info!("timeout for {} reached", self.room_id);
                    break
                }
            }
        }
        let _ = self.end_notify.send(self.room_id).await;
        tracing::info!("end notify sent");
    }

    // FIXME: return explicit controlflow instead of bool
    async fn event(&mut self) -> anyhow::Result<bool> {
        tokio::select! {
            pack = self.socket.recv_proto_from::<ClientMessage>() => {
                match pack {
                    Ok((addr, pack)) => self.socket_msg(addr, pack).await,
                    Err(e) => tracing::warn!("msg receive failed {}", e),
                }
            }
            ctl = self.control.recv() => {
                match ctl {
                    Some(ctl) => self.control_msg(ctl).await?,
                    None => return Ok(false),
                }
            }
            Some((src_id, client_id)) = self.tasks.next() => {
                if let std::collections::btree_map::Entry::Occupied(o) = self.client_id_to_src_id.entry(client_id) {
                    if *o.get() == src_id {
                        o.remove();
                    }
                    self.connections.remove(&src_id);
                }
            }
        }
        Ok(true)
    }

    async fn control_msg(&mut self, ctl: ChannelCtl) -> anyhow::Result<()> {
        match ctl {
            ChannelCtl::OpenConnection {
                client_id,
                user_name,
                responder,
            } => {
                let (msg_tx, msg_rx) = mpsc::channel(10);
                let (control, task) = VoiceConnection::start(
                    client_id,
                    user_name,
                    msg_rx,
                    self.voice_tx.clone(),
                    self.socket.clone(),
                );
                self.tasks.push(task);
                let src_id = control.source_id();
                let conn = Conn::new(msg_tx, control);
                self.connections.insert(src_id, conn);
                self.client_id_to_src_id.insert(client_id, src_id);
                let _ = responder.send(src_id);
                Ok(())
            }
            ChannelCtl::Establish {
                client_id,
                client_addr,
                responder,
            } => {
                let resp = match self.client_id_to_src_id(client_id) {
                    Ok(src_id) => {
                        let conn = self.connections.get_mut(&src_id).expect("always present");
                        conn.bind(client_addr);
                        Ok(conn.ctl.init(client_addr).await?)
                    }
                    Err(e) => Err(e.into()),
                };
                let _ = responder.send(resp);
                Ok(())
            }
            ChannelCtl::Peers { responder } => {
                let mut peers = Vec::new();
                for (id, conn) in self
                    .client_id_to_src_id
                    .keys()
                    .zip(self.connections.values())
                {
                    if matches!(
                        conn.ctl.status().await,
                        Ok(StatusResponse {
                            state: ConnectionState::Peered,
                            ..
                        })
                    ) {
                        peers.push(Peer {
                            id: *id,
                            source_id: conn.ctl.source_id(),
                            name: conn.ctl.user_name().into(),
                        });
                    }
                }
                let _ = responder.send(peers);
                Ok(())
            }
            ChannelCtl::Leave {
                client_id,
                responder,
            } => {
                if let Some(conn) = self
                    .client_id_to_src_id
                    .get(&client_id)
                    .and_then(|sid| self.connections.get(sid))
                {
                    conn.ctl.stop().await;
                }
                let _ = responder.send(());
                Ok(())
            }
            ChannelCtl::Status {
                client_id,
                responder,
            } => {
                let resp = match self.client_id_to_src_id(client_id) {
                    Ok(sid) => {
                        let conn = &self.connections[&sid];
                        Ok(conn.ctl.status().await.unwrap_or_else(|e| {
                            tracing::info!("status request failed: {}", e);
                            StatusResponse {
                                state: ConnectionState::Stopped,
                            }
                        }))
                    }
                    Err(e) => Err(e.into()),
                };
                let _ = responder.send(resp);
                Ok(())
            }
        }
    }

    async fn socket_msg(&mut self, addr: SocketAddr, msg: ClientMessage) {
        match msg {
            ClientMessage {
                payload: Some(client_message::Payload::IpDisco(disco)),
            } => {
                self.ip_disco(addr, disco).await;
            }
            ClientMessage {
                payload: Some(client_message::Payload::Voice(voice)),
            } => {
                if let Err(e) = self.forward(addr, voice.source_id, voice.into()).await {
                    tracing::warn!("failed to forward voice: {}", e);
                }
            }
            ClientMessage {
                payload: Some(client_message::Payload::Ping(ping)),
            } => {
                if let Err(e) = self.forward(addr, ping.source_id, ping.into()).await {
                    tracing::warn!("failed to forward ping: {}", e);
                }
            }
            ClientMessage { payload: None } => todo!(),
        }
    }

    async fn ip_disco(&mut self, addr: SocketAddr, msg: IpDiscoveryRequest) {
        if self.connections.get(&msg.source_id).is_some() {
            let _ = self
                .socket
                .send_to(
                    &ServerMessage::new(server_message::Message::IpDisco(IpDiscoveryResponse {
                        ip: addr.ip().to_string(),
                        port: addr.port() as _,
                    })),
                    addr,
                )
                .await
                .map_err(|e| tracing::info!("failed to respond to IP disco, {}", e));
            tracing::info!("handled IP disco");
        } else {
            tracing::info!("IP disco response from unknown src {}", msg.source_id);
        }
    }

    fn client_id_to_src_id(&self, client_id: Uuid) -> Result<u32, PeerNotFound> {
        self.client_id_to_src_id
            .get(&client_id)
            .copied()
            .ok_or(PeerNotFound)
    }

    async fn forward(
        &mut self,
        addr: SocketAddr,
        source_id: u32,
        msg: InternalMsg,
    ) -> Result<(), ForwardError> {
        let conn = self
            .connections
            .get(&source_id)
            .ok_or(UnexpectedSource(source_id))?;
        conn.forward(addr, msg).await
    }
}

enum InternalMsg {
    Voice(Voice),
    Ping(Ping),
}

impl From<Ping> for InternalMsg {
    fn from(v: Ping) -> Self {
        Self::Ping(v)
    }
}

impl From<Voice> for InternalMsg {
    fn from(v: Voice) -> Self {
        Self::Voice(v)
    }
}

struct Conn {
    tx: mpsc::Sender<InternalMsg>,
    ctl: VoiceControl,
    addr: Option<SocketAddr>,
}

impl Conn {
    fn new(tx: mpsc::Sender<InternalMsg>, ctl: VoiceControl) -> Self {
        Self {
            tx,
            ctl,
            addr: None,
        }
    }

    fn bind(&mut self, addr: SocketAddr) {
        // if let Some(old_addr) = self.addr {
        //     if old_addr != addr {
        //         todo!("error")
        //     }
        // }
        self.addr = Some(addr);
    }

    async fn forward(&self, from: SocketAddr, msg: InternalMsg) -> Result<(), ForwardError> {
        if let Some(addr) = self.addr {
            if !Self::same_addr(addr, from) {
                return Err(ForwardError::BadAddr(addr, from));
            }
        } else {
            return Err(ForwardError::SessionNotEstablished);
        }
        self.tx.send(msg).await.map_err(|_| {
            tracing::warn!("connection task stopped receiving");
            ForwardError::Stopped
        })
    }

    fn same_addr(l: SocketAddr, r: SocketAddr) -> bool {
        l.port() == r.port() && {
            match (l.ip(), r.ip()) {
                (l @ std::net::IpAddr::V4(_), r @ std::net::IpAddr::V4(_)) => l == r,
                (std::net::IpAddr::V4(l), r @ std::net::IpAddr::V6(_)) => l.to_ipv6_mapped() == r,
                (l @ std::net::IpAddr::V6(_), std::net::IpAddr::V4(r)) => l == r.to_ipv6_mapped(),
                (l @ std::net::IpAddr::V6(_), r @ std::net::IpAddr::V6(_)) => l == r,
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum ForwardError {
    #[error("expected source {0} got {1}")]
    BadAddr(SocketAddr, SocketAddr),
    #[error("session not established yet")]
    SessionNotEstablished,
    #[error("task has stopped")]
    Stopped,
    #[error(transparent)]
    UnexpectedSource(#[from] UnexpectedSource),
}

#[derive(thiserror::Error, Debug)]
#[error("unknown source id: {0}")]
struct UnexpectedSource(u32);
