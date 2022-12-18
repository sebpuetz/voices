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
use voices_crypto::xsalsa20poly1305::XSalsa20Poly1305;
use voices_crypto::VoiceCrypto;

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
    pub fn run(
        self,
        src_id: u32,
        cipher: XSalsa20Poly1305,
        deaf: bool,
        mute: bool,
    ) -> VoiceEventTx {
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
        tokio::spawn(async move {
            let (tx, rx) = RecordTx::new().expect("FIXME");
            let _stream = mic::record(tx).expect("oof");
            if let Err(e) = udp_tx(
                self.sock,
                rx,
                get_received,
                set_sent,
                mute,
                cipher,
            )
            .await
            {
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
    opus: audiopus::coder::Encoder,
}

impl RecordTx {
    pub fn new() -> anyhow::Result<(Self, RecordRx)> {
        let (tx, rx) = mpsc::channel(10);
        let rx = RecordRx { rx };
        let opus = audiopus::coder::Encoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Mono,
            audiopus::Application::Voip,
        )?;
        Ok((
            Self {
                tx,
                buf: Vec::with_capacity(std::mem::size_of::<i16>() * 8000),
                opus,
            },
            rx,
        ))
    }

    pub fn send(&mut self, voice: &[i16]) -> anyhow::Result<()> {
        self.buf.extend_from_slice(voice);
        const TWENTY_MS: usize = 48000 / 50;
        let chunks = self.buf.chunks_exact(TWENTY_MS);
        let rem = chunks.remainder();
        for chunk in chunks {
            let mut out = vec![0; 500];
            let written = self.opus.encode(chunk, &mut out).unwrap();
            out.truncate(written);
            self.tx
                .blocking_send(out)
                .map_err(|_| anyhow::anyhow!("failed to send encoded chunk"))?;
        }
        let rem_len = rem.len();
        let start = self.buf.len() - rem_len;
        let src = start..self.buf.len();
        self.buf.copy_within(src, 0);
        self.buf.truncate(rem_len);
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
                let ping = ClientMessage::ping(Ping {
                    seq: seq as _,
                    recv,
                    sent,
                });
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
            ServerMessage { message: None } => {
                anyhow::bail!("fucked up");
            }
        }
    }
}
