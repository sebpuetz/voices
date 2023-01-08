use std::collections::BinaryHeap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::dynamic_mixer::DynamicMixerController;
use rodio::queue::SourcesQueueInput;
use rodio::{OutputStream, Source};
use tokio::sync::mpsc;
use voice_proto::ServerVoice;

pub struct PcmF32<I> {
    samples: I,
    len: usize,
}

impl<I> PcmF32<I> {
    pub fn new(samples: I, len: usize) -> Self {
        Self { samples, len }
    }
}

impl<I> Iterator for PcmF32<I>
where
    I: Iterator<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.samples.next()
    }
}

impl<I> rodio::Source for PcmF32<I>
where
    I: Iterator<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.len / self.channels() as usize)
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        48000
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct Player {
    tx: mpsc::Sender<PlayerEvent>,
}

pub enum PlayerEvent {
    Joined { rx: PlayRx, announce_quiet: bool },
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

        let (ctl, mixer) = rodio::dynamic_mixer::mixer(2, 48000);
        let sink = rodio::Sink::try_new(&stream_handle).unwrap();
        let (notify_tx, notify_src) = rodio::queue::queue(true);
        ctl.add(notify_src);
        sink.append(mixer);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let localset = tokio::task::LocalSet::new();
        localset.block_on(&rt, play(ctl, rx, sink, notify_tx));
        tracing::warn!("output ended");
    }

    pub fn new_stream(&self, announce_quiet: bool) -> PlayTx {
        let (play_tx, rx) = PlayRx::new();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PlayerEvent::Joined { rx, announce_quiet }).await;
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
    pub fn send(&self, voice: ServerVoice) -> Result<(), mpsc::error::TrySendError<ServerVoice>> {
        self.tx.try_send(voice)
    }
}

impl PlayRx {
    pub fn new() -> (PlayTx, Self) {
        let (tx, rx) = mpsc::channel(3);
        let tx = PlayTx { tx };
        (tx, Self { rx })
    }

    pub async fn recv(&mut self) -> Option<ServerVoice> {
        self.rx.recv().await
    }
}

pub async fn play(
    ctl: Arc<DynamicMixerController<f32>>,
    mut play_rx: tokio::sync::mpsc::Receiver<PlayerEvent>,
    sink: rodio::Sink,
    notify_tx: Arc<SourcesQueueInput<f32>>,
) {
    let mut playing: Option<tokio::task::JoinHandle<()>> = None;
    while let Some(stream) = play_rx.recv().await {
        let quiet = match stream {
            PlayerEvent::Joined { rx: joined, announce_quiet: quiet } => {
                tracing::debug!("Received play stream");
                // FIXME: there can be substantial lag since the `samples` queue will happily accept more data without ever fast forwarding
                let (tx, samples) = rodio::queue::queue(true);
                ctl.add(samples);
                tokio::task::spawn_local(async move {
                    play_stream(tx, joined)
                        .await
                        .map_err(|e| tracing::warn!(error=?e, "play stream errored"))
                });
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
                    let rx = notify_tx.append_with_signal(bell.convert_samples().amplify(0.5));
                    let _ = rx.recv();
                }));
            } else {
                tracing::debug!("not playing sound");
            }
        }
    }
    tracing::debug!("play ended");
}

async fn play_stream(tx: Arc<SourcesQueueInput<f32>>, mut stream: PlayRx) -> anyhow::Result<()> {
    tracing::debug!("Started play stream");
    let mut state = State::new(tx)?;
    let mut deadline = Instant::now();
    loop {
        tokio::select! {
            biased;
            msg = stream.recv() => {
                let pack = match msg {
                    Some(msg) => msg,
                    None => break,
                };
                if let Some(voice) = state.tick(pack.into()) {
                    state.decode_and_submit(Some(&voice.payload))?;
                    deadline += Duration::from_millis(20);
                }
            }
            _ = tokio::time::sleep_until(deadline.into()) => {
                let voice = state.pop();
                state.decode_and_submit(voice.as_ref().map(|v| &*v.payload))?;
                deadline += Duration::from_millis(20);

            }
        }
    }
    tracing::debug!("stream ended");
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
pub struct Voice {
    sequence: u64,
    payload: Vec<u8>,
}

impl From<ServerVoice> for Voice {
    fn from(value: ServerVoice) -> Self {
        Voice {
            sequence: value.sequence,
            payload: value.payload,
        }
    }
}

impl std::fmt::Debug for Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("sequence", &self.sequence)
            .field("payload", &format_args!("[len {}]", self.payload.len()))
            .finish()
    }
}

impl Ord for Voice {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence.cmp(&other.sequence)
    }
}

impl PartialOrd for Voice {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.sequence.partial_cmp(&other.sequence)
    }
}

struct State {
    queue: BinaryHeap<std::cmp::Reverse<Voice>>,
    decoder: audiopus::coder::Decoder,
    clipper: audiopus::softclip::SoftClip,
    next_expected: u64,
    tx: Arc<SourcesQueueInput<f32>>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("queue", &self.queue)
            .field("decoder", &self.decoder)
            .field("clipper", &self.clipper)
            .field("next_expected", &self.next_expected)
            .finish()
    }
}

impl State {
    fn new(tx: Arc<SourcesQueueInput<f32>>) -> anyhow::Result<Self> {
        let decoder = audiopus::coder::Decoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Stereo,
        )?;
        let clipper = audiopus::softclip::SoftClip::new(audiopus::Channels::Stereo);
        Ok(State {
            queue: BinaryHeap::with_capacity(10),
            next_expected: 0,
            decoder,
            clipper,
            tx,
        })
    }

    fn tick(&mut self, voice: Voice) -> Option<Voice> {
        let ready = voice.sequence == self.next_expected;
        let n_buffered = self.queue.len();

        tracing::trace!(self.next_expected, voice.sequence);
        let play = if n_buffered >= 3 {
            let v = self.pop();
            self.push(voice);
            v
        } else if ready {
            self.next_expected = voice.sequence + 1;
            Some(voice)
        } else {
            self.push(voice);
            None
        };

        play
    }

    fn push(&mut self, voice: Voice) {
        let max_seq = self.max_seq();
        if max_seq.checked_sub(voice.sequence).unwrap_or_default() > 3 {
            tracing::warn!(
                max_seq,
                "stale voice packet already played ({}) vs. arrived ({})",
                self.next_expected,
                voice.sequence
            );
        }

        self.queue.push(std::cmp::Reverse(voice));
    }

    fn max_seq(&self) -> u64 {
        self.queue
            .iter()
            .map(|v| v.0.sequence)
            .max()
            .unwrap_or_default()
    }

    fn pop(&mut self) -> Option<Voice> {
        if let Some(v) = self.queue.pop().map(|v| v.0) {
            self.next_expected = v.sequence + 1;
            let max_seq = self.max_seq();
            if max_seq.checked_sub(v.sequence).unwrap_or_default() > 3 {
                tracing::info!("more than 3 packet frame gap to freshest");
            }
            Some(v)
        } else {
            self.next_expected += 1;
            tracing::trace!(self.next_expected, "empty buffer");
            None
        }
    }

    fn decode_and_submit(&mut self, payload: Option<&[u8]>) -> anyhow::Result<()> {
        let mut pcm = vec![0f32; 960 * 2];
        let input = payload.map(TryInto::try_into).transpose().unwrap();
        let n_samples = {
            let output = (&mut pcm).try_into().unwrap();
            match self.decoder.decode_float(input, output, false) {
                Ok(n_samples) => n_samples,
                Err(err) => {
                    tracing::error!(?err);
                    return Ok(());
                }
            }
        };
        pcm.truncate(n_samples * 2);
        let signals = (&mut pcm).try_into().unwrap();
        self.clipper.apply(signals)?;
        let src = PcmF32::new(pcm.into_iter(), n_samples);
        self.tx.append(src);
        Ok(())
    }
}

const BELL: &[u8] = include_bytes!("../assets/bell.wav");
