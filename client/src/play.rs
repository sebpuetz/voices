use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::dynamic_mixer::DynamicMixerController;
use rodio::queue::SourcesQueueInput;
use rodio::OutputStream;
use tokio::sync::mpsc;

use crate::voice::ServerVoice;

pub struct PcmI16<I> {
    samples: I,
    len: usize,
}

impl<I> PcmI16<I> {
    pub fn new(samples: I, len: usize) -> Self {
        Self { samples, len }
    }
}

impl<I> Iterator for PcmI16<I>
where
    I: Iterator<Item = u8>,
{
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        self.samples
            .next()
            .and_then(|a| self.samples.next().map(|b| i16::from_be_bytes([a, b])))
    }
}

impl<I> rodio::Source for PcmI16<I>
where
    I: Iterator<Item = u8>,
{
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.len)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        16000
    }

    fn total_duration(&self) -> Option<Duration> {
        let micros = (self.len * 1_000_000) / (self.sample_rate() as usize * 1_000_000);
        println!("{:?}", Duration::from_micros(micros as u64));
        Some(Duration::from_micros(micros as u64))
    }
}

pub struct Player {
    tx: mpsc::Sender<PlayerEvent>,
}

pub enum PlayerEvent {
    Joined { rx: PlayRx, quiet: bool },
    Left { quiet: bool },
    SetVolume(f32),
}

impl Player {
    pub async fn new() -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        std::thread::spawn(|| Self::sink_thread(rx));
        Ok(Self { tx })
    }

    fn sink_thread(rx: mpsc::Receiver<PlayerEvent>) {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let (ctl, mixer) = rodio::dynamic_mixer::mixer::<i16>(1, 16000);
        let sink = rodio::Sink::try_new(&stream_handle).unwrap();
        let (notify_tx, notify_src) = rodio::queue::queue(true);
        ctl.add(notify_src);
        sink.append(mixer);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let localset = tokio::task::LocalSet::new();
        localset.block_on(&rt, play(ctl, rx, sink, notify_tx));
        tracing::warn!("output ended");
    }

    pub fn new_stream(&self, quiet: bool) -> PlayTx {
        let (play_tx, rx) = PlayRx::new();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PlayerEvent::Joined { rx, quiet }).await;
        });
        play_tx
    }

    pub fn stream_ended(&self, quiet: bool) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PlayerEvent::Left { quiet }).await;
        });
    }
    pub fn mute(&self) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PlayerEvent::SetVolume(0.0)).await;
        });
    }
}
pub struct PlayRx {
    rx: mpsc::Receiver<ServerVoice>,
}

#[derive(Clone)]
pub struct PlayTx {
    tx: mpsc::Sender<ServerVoice>,
}

impl PlayTx {
    pub async fn send(
        &self,
        voice: ServerVoice,
    ) -> Result<(), mpsc::error::SendError<ServerVoice>> {
        self.tx.send(voice).await
    }
}

impl PlayRx {
    pub fn new() -> (PlayTx, Self) {
        let (tx, rx) = mpsc::channel(10);
        let tx = PlayTx { tx };
        (tx, Self { rx })
    }

    pub async fn recv(&mut self) -> Option<ServerVoice> {
        self.rx.recv().await
    }
}

pub async fn play(
    ctl: Arc<DynamicMixerController<i16>>,
    mut play_rx: tokio::sync::mpsc::Receiver<PlayerEvent>,
    sink: rodio::Sink,
    notify_tx: Arc<SourcesQueueInput<i16>>,
) {
    let mut playing: Option<tokio::task::JoinHandle<()>> = None;
    while let Some(stream) = play_rx.recv().await {
        let quiet = match stream {
            PlayerEvent::Joined { rx: joined, quiet } => {
                tracing::debug!("Received play stream");
                let (tx, samples) = rodio::queue::queue(true);
                ctl.add(samples);
                tokio::task::spawn_local(play_stream(tx, joined));
                quiet
            }
            PlayerEvent::Left { quiet } => quiet,
            PlayerEvent::SetVolume(vol) => {
                sink.set_volume(vol);
                true
            }
        };
        if !quiet {
            let bell = rodio::decoder::Decoder::new_wav(Cursor::new(BELL)).unwrap();
            let notify_tx = notify_tx.clone();
            let not_playing = playing.as_ref().map(|v| v.is_finished()).unwrap_or(true);
            if not_playing {
                playing = Some(tokio::task::spawn_blocking(move || {
                    let rx = notify_tx.append_with_signal(bell);
                    let _ = rx.recv();
                }));
            } else {
                tracing::debug!("not playing sound");
            }
        }
    }
    tracing::debug!("play ended");
}

async fn play_stream(tx: Arc<SourcesQueueInput<i16>>, mut stream: PlayRx) {
    tracing::debug!("Started play stream");
    let mut state = State::new();
    while let Some(pack) = stream.recv().await {
        // tracing::debug!("received voice");
        if state.push(pack) {
            let (len, chunk) = state.drain();
            let src = PcmI16::new(chunk, len / 2);
            tx.append(src);
        }
    }
    tracing::debug!("stream ended");
}

struct State {
    start: Instant,
    last: Instant,
    queue: Vec<ServerVoice>,
    n_bytes: usize,
    seq_cutoff: u64,
}

impl State {
    fn new() -> Self {
        State {
            start: Instant::now(),
            last: Instant::now(),
            queue: Vec::with_capacity(10),
            n_bytes: 0,
            seq_cutoff: 0,
        }
    }
}
impl State {
    fn push(&mut self, voice: ServerVoice) -> bool {
        if self.seq_cutoff > voice.sequence {
            tracing::warn!(
                "dropping stale voice packet oldest ({}) dropped ({})",
                self.seq_cutoff,
                voice.sequence
            );
        }
        if self.last.elapsed() > Duration::from_millis(100) {
            tracing::warn!("clearing state after receiving 100ms later frame");
            self.clear();
            self.n_bytes += voice.payload.len();
            self.queue.push(voice);
            self.last = Instant::now();
            return false;
        }
        self.last = Instant::now();
        self.n_bytes += voice.payload.len();
        self.queue.push(voice);
        self.start.elapsed() > Duration::from_millis(50)
    }

    fn clear(&mut self) {
        self.queue.clear();
    }

    fn drain(&mut self) -> (usize, impl Iterator<Item = u8> + 'static) {
        self.queue.sort_by(|v1, v2| v1.sequence.cmp(&v2.sequence));
        if let Some(last) = self.queue.last() {
            self.seq_cutoff = last.sequence;
        }
        let mut out = Vec::with_capacity(10);
        std::mem::swap(&mut out, &mut self.queue);
        self.start = Instant::now();
        let len = self.n_bytes;
        self.n_bytes = 0;
        (
            len,
            out.into_iter().map(|v| v.payload.into_iter()).flatten(),
        )
    }
}

const BELL: &[u8] = include_bytes!("../assets/bell.wav");
