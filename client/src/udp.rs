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
use uuid::Uuid;
use voice_proto::*;
use voices_voice_crypto::xsalsa20poly1305::XSalsa20Poly1305;
use voices_voice_crypto::VoiceCrypto;

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
    pub async fn discover_ip(&mut self, source_id: u32) -> Result<SocketAddr, SetupError> {
        tracing::debug!("start ip discovery");

        let req = IpDiscoveryRequest {
            uuid: self.user_id.as_bytes().to_vec(),
            source_id,
        };
        self.sock.send(&ClientMessage::ip_disco(req)).await?;

        let ServerMessage { message } = timeout(Duration::from_millis(100), self.sock.recv())
            .await
            .map_err(|_| SetupError::Timeout)??;
        let msg = message.ok_or(SetupError::BadData)?;
        if let server_message::Message::IpDisco(IpDiscoveryResponse { ip, port }) = msg {
            let ip = IpAddr::from_str(ip.as_str()).map_err(|_| SetupError::BadData)?;
            let port = port as u16;
            let external_addr = SocketAddr::from((ip, port));
            tracing::debug!("external addr: {}", external_addr);
            Ok(external_addr)
        } else {
            tracing::warn!("expected ip disco response, got something different");
            Err(SetupError::BadData)
        }
    }

    #[tracing::instrument(skip_all, fields(source_id=src_id))]
    pub fn run(
        self,
        src_id: u32,
        cipher: XSalsa20Poly1305,
        deaf: bool,
        mute: bool,
    ) -> anyhow::Result<VoiceEventTx> {
        let received = Arc::new(AtomicU64::new(0));
        let set_received = received.clone();
        let get_received = received;
        let sent = Arc::new(AtomicU64::new(0));
        let get_sent = sent.clone();
        let set_sent = sent;

        let sock2 = self.sock.clone();
        let (voice_event_tx, voice_event_rx) = mpsc::channel(10);
        let cipher_rx = cipher.clone();
        tokio::spawn(async move {
            if let Err(e) = udp_rx(
                sock2,
                set_received,
                get_sent,
                voice_event_rx,
                deaf,
                cipher_rx,
            )
            .await
            {
                tracing::error!("udp rx dead: {}", e)
            }
        });
        let (tx, rx) = RecordTx::new();
        let stream = mic::record(tx)?;
        tokio::spawn(async move {
            let _stream = stream;
            if let Err(e) =
                udp_tx(self.sock, rx, get_received, set_sent, mute, cipher, src_id).await
            {
                tracing::error!("udp tx dead: {}", e)
            }
        });
        Ok(VoiceEventTx(voice_event_tx))
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
}

impl RecordTx {
    pub fn new() -> (Self, RecordRx) {
        let (tx, rx) = mpsc::channel(10);
        let rx = RecordRx { rx };
        (Self { tx }, rx)
    }

    pub fn send(&mut self, voice: Vec<u8>) -> anyhow::Result<()> {
        self.tx
            .blocking_send(voice)
            .map_err(|_| anyhow::anyhow!("failed to send encoded chunk"))?;
        Ok(())
    }
}

pub async fn udp_tx(
    mut sock: UdpWithBuf,
    mut voice_recv: RecordRx,
    get_received: Arc<AtomicU64>,
    set_sent: Arc<AtomicU64>,
    mute: bool,
    cipher: XSalsa20Poly1305,
    source_id: u32,
) -> anyhow::Result<()> {
    let mut deadline = Instant::now();
    let keepalive_interval = Duration::from_secs(5);
    deadline += keepalive_interval;
    let start_seq_num = rand::random();
    let mut sequence = start_seq_num;
    let mut stream_time = rand::random();
    loop {
        let sleep_until = tokio::time::sleep_until(deadline);
        select! {
            voice = voice_recv.rx.recv() => {
                if let Some(mut voice) = voice {
                    if !mute {
                        cipher.encrypt(&mut voice, sequence, stream_time)?;
                        let payload = Voice {
                            payload: voice,
                            sequence: sequence as _,
                            stream_time,
                            source_id,
                        };
                        let voice = ClientMessage::voice(payload);
                        sock.send(&voice).await?;
                        sequence = sequence.wrapping_add(1);
                        stream_time = stream_time.wrapping_add(960);
                    }
                } else {
                    anyhow::bail!("mic dead")
                }
            }
            _ = sleep_until => {
                let seq = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_millis();
                let recv = get_received.load(std::sync::atomic::Ordering::SeqCst);
                let sent = set_sent.swap(sequence.wrapping_sub(start_seq_num), std::sync::atomic::Ordering::SeqCst);
                let ping = ClientMessage::ping(Ping {seq:seq as _,recv,sent, source_id });
                sock.send(&ping).await?;
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

    // FIXME: this should be a priority queue (drop old packets, based on sequence)
    // instead of FIFO (drop new packets, based on arrival)
    // => directly push onto heap
    fn send(&mut self, id: u32, msg: ServerVoice) {
        match self.current.get(&id).cloned() {
            Some(sender) => {
                if let Err(mpsc::error::TrySendError::Closed(_)) = sender.send(msg) {
                    panic!("ded")
                }
            }
            None => {
                tracing::warn!("dropping voice packet for unknown id: {}", id);
            }
        };
    }

    fn register(&mut self, id: u32, announce_quiet: bool) -> PlayTx {
        tracing::info!("Registering voice stream for id: {}", id);
        let tx = self.player.new_stream(announce_quiet);
        self.current.insert(id, tx.clone());
        tx
    }

    fn remove(&mut self, id: u32, announce_quiet: bool) {
        tracing::info!("Stopping voice stream for id: {}", id);
        self.current.remove(&id);
        self.player.stream_ended(announce_quiet);
    }

    fn mute(&self) {
        self.player.mute();
    }
}

pub async fn udp_rx(
    mut sock: UdpWithBuf,
    set_received: Arc<AtomicU64>,
    get_sent: Arc<AtomicU64>,
    mut voice_event_rx: mpsc::Receiver<VoiceEvent>,
    deaf: bool,
    cipher: XSalsa20Poly1305,
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
                message: Some(server_message::Message::Voice(mut voice)),
            } => {
                cipher.decrypt(&mut voice.payload, voice.sequence, voice.stream_time)?;
                set_received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.send(voice.source_id, voice);
            }
            ServerMessage {
                message: Some(server_message::Message::Pong(pong)),
            } => {
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
                let received = set_received.load(std::sync::atomic::Ordering::SeqCst);
                let expected_received = pong.sent;
                let sent = get_sent.load(std::sync::atomic::Ordering::Relaxed);
                let expected_sent = pong.received;
                tracing::debug!(
                    received,
                    expected_received,
                    sent,
                    expected_sent,
                    "Pong: {}ms",
                    (now - Duration::from_millis(pong.seq)).as_millis(),
                );
            }
            ServerMessage { message: Some(_) } => todo!(),
            ServerMessage { message: None } => {
                anyhow::bail!("fucked up");
            }
        }
    }
}
