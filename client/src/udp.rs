use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};
use udp_proto::{UdpError, UdpWithBuf};
use voice_proto::*;
use uuid::Uuid;

use crate::play::{PlayTx, Player};
use crate::{mic, play};

pub struct UdpSetup {
    sock: UdpWithBuf,
    user_id: Uuid,
}

impl UdpSetup {
    pub async fn new(target: SocketAddr, user_id: Uuid) -> Result<Self, SetupError> {
        let slf_addr = SocketAddr::from((IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0));
        let sock = UdpWithBuf::bind(slf_addr).await?;
        sock.connect(target)
            .await
            .map_err(|e| SetupError::from(e).context("connect failed"))?;
        Ok(Self { sock, user_id })
    }

    #[tracing::instrument(skip_all)]
    pub async fn discover_ip(&mut self) -> Result<SocketAddr, SetupError> {
        tracing::debug!("start ip discovery");
        let req = IpDiscoveryRequest {
            uuid: self.user_id.as_bytes().to_vec(),
        };
        self.sock.send(&req).await?;

        let IpDiscoveryResponse { ip, port } =
            timeout(Duration::from_millis(100), self.sock.recv())
                .await
                .map_err(|_| SetupError::Timeout)??;

        let ip = IpAddr::from_str(ip.as_str()).map_err(|_| SetupError::BadData)?;
        let port = port as u16;
        let external_addr = SocketAddr::from((ip, port));
        tracing::debug!("external addr: {}", external_addr);
        Ok(external_addr)
    }

    #[tracing::instrument(skip_all, fields(source_id=src_id))]
    pub fn run(self, src_id: u32, deaf: bool) -> VoiceEventTx {
        let received = Arc::new(AtomicU64::new(0));
        let set_received = received.clone();
        let get_received = received;
        let sock2 = self.sock.clone();
        let (voice_event_tx, voice_event_rx) = mpsc::channel(10);
        tokio::spawn(async move {
            if let Err(e) = udp_rx(sock2, set_received, voice_event_rx, deaf).await {
                tracing::error!("udp rx dead: {}", e)
            }
        });
        tokio::spawn(async move {
            let (tx, rx) = RecordTx::new();
            let _stream = mic::record(tx).expect("oof");
            if let Err(e) = udp_tx(self.sock, rx, get_received).await {
                tracing::error!("udp tx dead: {}", e)
            }
        });
        VoiceEventTx(voice_event_tx)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SetupError {
    #[error(transparent)]
    Udp(#[from] UdpError),
    #[error("timeout waiting for response")]
    Timeout,
    #[error("invalid data")]
    BadData,
    #[error("{0}")]
    Context(&'static str, #[source] Box<Self>),
}

impl SetupError {
    pub fn context(self, msg: &'static str) -> Self {
        SetupError::Context(msg, Box::new(self))
    }
}

pub struct VoiceEventTx(mpsc::Sender<VoiceEvent>);

impl VoiceEventTx {
    pub async fn joined(&self, id: u32) {
        let _ = self.0.send(VoiceEvent::Joined(id)).await;
    }

    pub async fn already_present(&self, id: u32) {
        let _ = self.0.send(VoiceEvent::AlreadyPresent(id)).await;
    }
    pub async fn left(&self, id: u32) {
        let _ = self.0.send(VoiceEvent::Left(id)).await;
    }
}

pub enum VoiceEvent {
    AlreadyPresent(u32),
    Joined(u32),
    Left(u32),
}

pub struct RecordRx {
    rx: mpsc::Receiver<Vec<u8>>,
}

pub struct RecordTx {
    tx: mpsc::Sender<Vec<u8>>,
    buf: Vec<i16>,
}

impl RecordTx {
    pub fn new() -> (Self, RecordRx) {
        let (tx, rx) = mpsc::channel(10);
        let rx = RecordRx { rx };
        (
            Self {
                tx,
                buf: Vec::with_capacity(std::mem::size_of::<i16>() * 16000),
            },
            rx,
        )
    }

    pub fn send(&mut self, voice: &[i16]) -> Result<(), ()> {
        self.buf.extend_from_slice(voice);
        while !self.buf.is_empty() {
            let lim = std::cmp::min(self.buf.len(), 700);
            let mut chunk = self.buf.split_off(lim);
            std::mem::swap(&mut chunk, &mut self.buf);
            let chunk = chunk.into_iter().flat_map(|s| s.to_be_bytes()).collect();
            self.tx.blocking_send(chunk).expect("FIXME error");
        }
        Ok(())
    }
}

pub async fn udp_tx(
    mut sock: UdpWithBuf,
    mut voice_recv: RecordRx,
    get_received: Arc<AtomicU64>,
) -> anyhow::Result<()> {
    let mut deadline = Instant::now();
    let keepalive_interval = Duration::from_secs(1);
    deadline += keepalive_interval;
    loop {
        let sleep_until = tokio::time::sleep_until(deadline);
        select! {
            voice = voice_recv.rx.recv() => {
                if let Some(voice) = voice {
                    let sequence = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let payload = Voice {
                        payload: voice,
                        sequence: sequence as _,
                    };
                    let voice = ClientMessage::voice(payload);
                    sock.send(&voice).await.expect("oops");
                } else {
                    anyhow::bail!("mic dead")
                }
            }
            _ = sleep_until => {
                let seq = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_millis();
                let recv = get_received.load(std::sync::atomic::Ordering::SeqCst);
                let ping = ClientMessage::ping(Ping {
                    seq: seq as _,
                    recv,
                });
                sock.send(&ping).await.expect("oops");
                deadline += keepalive_interval;
            }
        }
    }
}

struct ReceiverState {
    current: BTreeMap<u32, PlayTx>,
    player: Player,
}

impl ReceiverState {
    fn new(announcer: Player) -> Self {
        ReceiverState {
            current: BTreeMap::new(),
            player: announcer,
        }
    }

    fn send(&mut self, id: u32, msg: ServerVoice) {
        let sender = match self.current.get(&id).cloned() {
            Some(sender) => sender,
            None => {
                tracing::warn!("starting voice for unknown id: {}", id);
                self.register(id, true)
            }
        };
        tokio::spawn(async move {
            sender
                .send(msg)
                .await
                .map_err(|_| anyhow::anyhow!("ded"))
                .unwrap()
        });
    }

    fn register(&mut self, id: u32, quiet: bool) -> PlayTx {
        tracing::info!("Registering voice stream for id: {}", id);
        let tx = self.player.new_stream(quiet);
        self.current.insert(id, tx.clone());
        tx
    }

    fn remove(&mut self, id: u32, quiet: bool) {
        tracing::info!("Stopping voice stream for id: {}", id);
        self.current.remove(&id);
        self.player.stream_ended(quiet);
    }

    fn mute(&self) {
        self.player.mute();
    }
}

pub async fn udp_rx(
    mut sock: UdpWithBuf,
    set_received: Arc<AtomicU64>,
    mut voice_event_rx: mpsc::Receiver<VoiceEvent>,
    deaf: bool,
) -> anyhow::Result<()> {
    let player = play::Player::new().await?;

    let mut state = ReceiverState::new(player);
    if deaf {
        state.mute();
    }
    loop {
        let sock = sock.recv();
        let voice_event = voice_event_rx.recv();
        let msg = tokio::select! {
            sock_msg = sock => {
                sock_msg
            }
            voice_evt_msg = voice_event => {
                match voice_evt_msg {
                    Some(VoiceEvent::Joined(id)) => {
                        state.register(id, deaf);
                    },
                    Some(VoiceEvent::AlreadyPresent(id)) => {
                        state.register(id, true);
                    }
                    Some(VoiceEvent::Left(id)) => state.remove(id, deaf),
                    None => {
                        tracing::debug!("Voice event sender stopped, exitting UDP task");
                        return Ok(())
                    },
                }
                continue
            }
        };
        let msg: ServerMessage = match msg {
            Ok(len) => len,
            Err(e) => {
                tracing::error!("recv error: {}", e);
                if e.fatal() {
                    return Err(e.into());
                }
                continue;
            }
        };
        match msg {
            ServerMessage {
                message: Some(server_message::Message::Voice(voice)),
            } => {
                set_received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.send(voice.source_id, voice);
            }
            ServerMessage {
                message: Some(server_message::Message::Pong(pong)),
            } => {
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
                let received = set_received.load(std::sync::atomic::Ordering::SeqCst);
                let expected = pong.sent;
                tracing::debug!(
                    "Pong: {}, received: {}, expected: {}",
                    (now - Duration::from_millis(pong.seq)).as_millis(),
                    received,
                    expected
                );
            }
            ServerMessage { message: None } => {
                anyhow::bail!("fucked up");
            }
        }
    }
}
