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
    ctl: Arc<DynamicMixerController<f32>>,
    mut play_rx: tokio::sync::mpsc::Receiver<PlayerEvent>,
    sink: rodio::Sink,
    notify_tx: Arc<SourcesQueueInput<f32>>,
) {
    let mut playing: Option<tokio::task::JoinHandle<()>> = None;
    while let Some(stream) = play_rx.recv().await {
        let quiet = match stream {
            PlayerEvent::Joined { rx: joined, quiet } => {
                tracing::debug!("Received play stream");
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
    let mut state = State::new();
    let mut decoder =
        audiopus::coder::Decoder::new(audiopus::SampleRate::Hz48000, audiopus::Channels::Stereo)?;
    let mut clipper = audiopus::softclip::SoftClip::new(audiopus::Channels::Stereo);
    loop {
        let mut deadline = Instant::now();
        deadline += Duration::from_millis(20);
        tokio::select! {
            biased;
            msg = stream.recv() => {
                let pack = match msg {
                    Some(msg) => msg,
                    None => break,
                };
                if let Some(voice) = state.push(pack) {
                    decode_and_submit(Some(&voice.payload), &mut decoder, &mut clipper, &tx)?;
                }
            }
            // FIXME: deadline needs to depend on stream time
            _ = tokio::time::sleep_until(deadline.into()) => {
                let chunks = state.drain().into_iter().flatten();
                for chunk in chunks {
                    decode_and_submit(chunk.as_ref().map(|v| &*v.payload), &mut decoder, &mut clipper, &tx)?;
                }
            }
        }
    }
    tracing::debug!("stream ended");
    Ok(())
}

fn decode_and_submit(
    payload: Option<&[u8]>,
    decoder: &mut audiopus::coder::Decoder,
    clipper: &mut audiopus::softclip::SoftClip,
    tx: &Arc<SourcesQueueInput<f32>>,
) -> anyhow::Result<()> {
    let mut pcm = vec![0f32; 960 * 2];
    let input = payload.map(TryInto::try_into).transpose().unwrap();
    let n_samples = {
        let output = (&mut pcm).try_into().unwrap();
        match decoder.decode_float(input, output, false) {
            Ok(n_samples) => n_samples,
            Err(err) => {
                tracing::error!(?err);
                return Ok(());
            }
        }
    };
    pcm.truncate(n_samples * 2);
    let signals = (&mut pcm).try_into().unwrap();
    clipper.apply(signals)?;
    let src = PcmF32::new(pcm.into_iter(), n_samples);
    tx.append(src);
    Ok(())
}

#[derive(Debug)]
struct State {
    last: Instant,
    queue: Vec<ServerVoice>,
    seq_cutoff: u64,
}

impl State {
    fn new() -> Self {
        State {
            last: Instant::now(),
            queue: Vec::with_capacity(10),
            seq_cutoff: 0,
        }
    }
}
impl State {
    fn push(&mut self, voice: ServerVoice) -> Option<ServerVoice> {
        if voice.sequence == self.seq_cutoff {
            self.seq_cutoff = voice.sequence + 1;
            self.last = Instant::now();
            return Some(voice);
        }

        // FIXME: need to take stream timestamp into account when determining staleness...
        let time_since_last = self.last.elapsed();
        let stale = time_since_last > Duration::from_millis(100);
        if stale {
            tracing::warn!("clearing state after receiving 100ms later frame");
            self.clear();
        }

        if self.seq_cutoff > voice.sequence {
            tracing::warn!(
                "dropping stale voice packet oldest ({}) dropped ({})",
                self.seq_cutoff,
                voice.sequence
            );
        } else {
            self.queue.push(voice);
        }

        self.last = Instant::now();
        None
    }

    fn clear(&mut self) {
        self.queue.clear();
    }

    fn drain(&mut self) -> Option<Buffered> {
        if self.queue.len() >= 4 || self.last.elapsed() >= Duration::from_millis(80) {
            self.queue.sort_by(|v1, v2| v1.sequence.cmp(&v2.sequence));
            let cur_seq = self.seq_cutoff;
            if let Some(last) = self.queue.last() {
                self.seq_cutoff = last.sequence + 1;
            }

            let mut out = Vec::with_capacity(10);
            std::mem::swap(&mut out, &mut self.queue);
            Some(Buffered::new(cur_seq, out))
        } else {
            None
        }
    }
}

pub struct Buffered {
    cur_seq: u64,
    ready: Option<ServerVoice>,
    inner: std::vec::IntoIter<ServerVoice>,
}

impl std::fmt::Debug for Buffered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffered")
            .field("cur_seq", &self.cur_seq)
            .field("ready", &self.ready)
            .finish()
    }
}

impl Buffered {
    pub fn new(cur_seq: u64, inner: Vec<ServerVoice>) -> Self {
        Self {
            cur_seq,
            ready: None,
            inner: inner.into_iter(),
        }
    }
}

impl Iterator for Buffered {
    type Item = Option<ServerVoice>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.ready.take().or_else(|| self.inner.next())?;
        if self.cur_seq == 0 {
            self.cur_seq = next.sequence;
        }
        let it = if next.sequence == self.cur_seq {
            Some(Some(next))
        } else {
            self.ready = Some(next);
            Some(None)
        };
        self.cur_seq = self.cur_seq.wrapping_add(1);
        it
    }
}

const BELL: &[u8] = include_bytes!("../assets/bell.wav");
