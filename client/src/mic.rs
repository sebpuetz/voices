use anyhow::Context;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd"
))]
use cpal::platform::{AlsaDevice as Device, AlsaHost as Host, AlsaStream as Stream};
#[cfg(target_os = "windows")]
use cpal::platform::{WasapiDevice as Device, WasapiHost as Host, WasapiStream as Stream};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, InputCallbackInfo, SampleFormat, SampleRate, StreamConfig};
use rubato::{FftFixedInOut, Resampler as _};

use crate::udp::RecordTx;

pub fn record(tx: RecordTx) -> anyhow::Result<Stream> {
    let host = Host::new()?;
    let device = host.default_input_device().unwrap();
    tracing::info!("Input device: {}", device.name().unwrap());
    let err_fn = move |err| {
        tracing::error!("an error occurred on stream: {}", err);
    };
    let (mut config, fmt) = find_best_input(&device)?;
    let bitrate = (config.sample_rate.0 * config.channels as u32) * fmt.sample_size() as u32;
    config.buffer_size = BufferSize::Fixed(bitrate / 50);
    tracing::debug!("Input config: {:#?}", config);
    let cb = Input::new(config.channels as u32, config.sample_rate.0, tx)?.data_callback();
    let audio_stream = device.build_input_stream(&config, cb, err_fn)?;
    audio_stream.play()?;
    Ok(audio_stream)
}

pub struct Input {
    input_buf: Vec<f32>,

    opus: audiopus::coder::Encoder,
    resampler: Option<Resampler>,
    channels: u32,
    sample_rate: u32,

    tx: RecordTx,
}

pub struct Resampler {
    inner: FftFixedInOut<f32>,
    buf_in: Vec<Vec<f32>>,
    buf_out: Vec<Vec<f32>>,
}

impl Resampler {
    pub fn process(&mut self) -> anyhow::Result<()> {
        self.inner
            .process_into_buffer(&self.buf_in, &mut self.buf_out, None)?;
        Ok(())
    }
}

impl Input {
    fn new(channels: u32, sample_rate: u32, tx: RecordTx) -> anyhow::Result<Self> {
        let resampler = if sample_rate != 48000 {
            tracing::info!(input_sample_rate = sample_rate, "initializing resampler");
            let inner = FftFixedInOut::new(sample_rate as usize, 48_000, 960, 2)?;
            let mut buf_in = inner.input_buffer_allocate();
            buf_in
                .iter_mut()
                .for_each(|c| c.resize((sample_rate / 50) as usize, 0f32));
            let buf_out = inner.output_buffer_allocate();
            Some(Resampler {
                inner,
                buf_in,
                buf_out,
            })
        } else {
            None
        };
        let mut opus = audiopus::coder::Encoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Stereo,
            audiopus::Application::Voip,
        )?;
        opus.set_bitrate(audiopus::Bitrate::BitsPerSecond(128_000))?;
        Ok(Self {
            input_buf: Vec::with_capacity(4096),
            opus,
            channels,
            tx,
            resampler,
            sample_rate,
        })
    }

    pub fn data_callback(mut self) -> impl FnMut(&[f32], &InputCallbackInfo) + Send + 'static {
        let samples_per_period = (2 * self.sample_rate) / 50;

        move |data: &[f32], _: &InputCallbackInfo| {
            if self.channels == 2 {
                self.input_buf.extend_from_slice(data);
            } else {
                data.iter().copied().for_each(|it| {
                    self.input_buf.push(it);
                    self.input_buf.push(it);
                });
            };

            let chunks = self.input_buf.chunks_exact(samples_per_period as usize);
            let rem = chunks.remainder();
            let mut resampled = Vec::new();
            for mut chunk in chunks {
                if let Some(resampler) = &mut self.resampler {
                    for (i, c) in chunk.chunks_exact(2).enumerate() {
                        resampler.buf_in[0][i] = c[0];
                        resampler.buf_in[1][i] = c[1];
                    }
                    resampler.process().expect("resampler failed");
                    let samples = resampler.buf_out[0].len();
                    resampled.resize(samples * 2, 0f32);
                    for i in 0..samples {
                        resampled[i * 2] = resampler.buf_out[0][i];
                        resampled[i * 2 + 1] = resampler.buf_out[1][i];
                    }
                    chunk = &resampled;
                }
                let mut out = vec![0; 500];
                let written = self.opus.encode_float(chunk, &mut out).unwrap();
                out.truncate(written);
                self.tx.send(out).expect("mic receiver died");
            }
            let rem_len = rem.len();
            let start = self.input_buf.len() - rem_len;
            let src = start..self.input_buf.len();
            self.input_buf.copy_within(src, 0);
            self.input_buf.truncate(rem_len);
        }
    }
}

fn find_best_input(dev: &Device) -> anyhow::Result<(StreamConfig, SampleFormat)> {
    let configs = dev
        .supported_input_configs()?
        .filter(|cfg| cfg.sample_format() == SampleFormat::F32 || cfg.channels() <= 2)
        .collect::<Vec<_>>();
    let mut fallback: Option<cpal::SupportedStreamConfigRange> = None;
    for cfg in configs.iter() {
        if cfg.max_sample_rate() > SampleRate(48000)
            && cfg.min_sample_rate() < SampleRate(48000)
            && cfg.channels() == 2
        {
            tracing::debug!("found ideal input config");
            let fmt = cfg.sample_format();
            let cfg = cfg.clone().with_sample_rate(SampleRate(48000)).config();
            return Ok((cfg, fmt));
        }
        if let Some(v) = fallback.as_mut() {
            if cfg.channels() >= v.channels() && cfg.max_sample_rate() > v.max_sample_rate() {
                *v = cfg.clone();
            }
        } else {
            fallback = Some(cfg.clone());
        }
    }
    fallback
        .context("didn't find appropriate streamconfig")
        .map(|cfg| {
            let sample_rate = std::cmp::min(cfg.max_sample_rate(), SampleRate(48000));
            let sample_fmt = cfg.sample_format();
            tracing::debug!("using fallback input config");
            (cfg.with_sample_rate(sample_rate).config(), sample_fmt)
        })
}
