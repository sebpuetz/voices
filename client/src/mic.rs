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
use nnnoiseless::DenoiseState;
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
    let audio_stream = device.build_input_stream(&config, cb, err_fn, None)?;
    audio_stream.play()?;
    Ok(audio_stream)
}

pub struct Input {
    input_buf: Vec<f32>,
    opus: audiopus::coder::Encoder,
    resampler: Option<Resampler>,
    channels: u32,
    sample_rate: u32,
    denoiser_l: Box<DenoiseState<'static>>,
    denoiser_r: Box<DenoiseState<'static>>,
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

fn interleave<I: AsRef<[f32]>>(input: &[I], output: &mut [f32]) {
    assert_eq!(input.len(), 2);
    let l = input[0].as_ref();
    let r = input[1].as_ref();

    assert_eq!(l.len() + r.len(), output.len());
    for i in 0..l.len() {
        output[i * 2] = l[i];
        output[i * 2 + 1] = r[i];
    }
}

fn deinterleave(input: &[f32], output: &mut [Vec<f32>]) {
    assert_eq!(input.len(), output[0].len() + output[1].len());
    assert_eq!(output.len(), 2);
    for i in 0..output[0].len() {
        output[0][i] = input[i * 2];
        output[1][i] = input[i * 2 + 1];
    }
}

fn scale_up(v: &mut [f32]) {
    v.iter_mut().for_each(|v| {
        *v *= 32767.0;
    })
}

fn scale_down(v: &mut [f32]) {
    v.iter_mut().for_each(|v| {
        *v /= 32767.0;
    })
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
        let denoiser_l = DenoiseState::new();
        let denoiser_r = DenoiseState::new();
        Ok(Self {
            input_buf: Vec::with_capacity(4096),
            opus,
            channels,
            tx,
            resampler,
            denoiser_l,
            denoiser_r,
            sample_rate,
        })
    }

    pub fn data_callback(mut self) -> impl FnMut(&[f32], &InputCallbackInfo) + Send + 'static {
        let samples_per_period = (2 * self.sample_rate) / 50;

        let mut denoised_l = vec![0.0; 480];
        let mut denoised_r = vec![0.0; 480];
        let mut deinterleave_buf: Option<[Vec<f32>; 2]> = None;
        move |data: &[f32], _: &InputCallbackInfo| {
            if self.channels == 2 {
                self.input_buf.extend_from_slice(data);
            } else {
                data.iter().copied().for_each(|it| {
                    self.input_buf.push(it);
                    self.input_buf.push(it);
                });
            };

            let mut chunks = self.input_buf.chunks_exact_mut(samples_per_period as usize);
            let mut resampled = Vec::new();
            for mut chunk in chunks.by_ref() {
                scale_up(chunk);
                if let Some(resampler) = &mut self.resampler {
                    deinterleave(chunk, &mut resampler.buf_in);
                    resampler.process().expect("resampler failed");
                    let samples = resampler.buf_out[0].len();
                    resampled.resize(samples * 2, 0f32);
                    for ((denoise_chunk_l, denoise_chunk_r), out) in resampler.buf_out[0]
                        .chunks_exact(480)
                        .zip(resampler.buf_out[1].chunks_exact(480))
                        .zip(resampled.chunks_exact_mut(960))
                    {
                        self.denoiser_l
                            .process_frame(&mut denoised_l, denoise_chunk_l);
                        self.denoiser_r
                            .process_frame(&mut denoised_r, denoise_chunk_r);
                        interleave(&[&denoised_l, &denoised_r], out);
                    }
                    chunk = &mut resampled;
                } else {
                    let out =
                        deinterleave_buf.get_or_insert_with(|| [vec![0.; 480], vec![0.; 480]]);
                    let mut denoise_chunks = chunk.chunks_exact_mut(960);
                    for denoise_chunk in denoise_chunks.by_ref() {
                        deinterleave(denoise_chunk, out);
                        self.denoiser_l.process_frame(&mut denoised_l, &out[0]);
                        self.denoiser_r.process_frame(&mut denoised_r, &out[1]);
                        let l = &out[0];
                        let r = &out[1];
                        interleave(&[l, r], denoise_chunk);
                    }
                }
                scale_down(chunk);

                let mut out = vec![0; 500];
                let written = self.opus.encode_float(chunk, &mut out).unwrap();
                out.truncate(written);
                self.tx.send(out).expect("mic receiver died");
            }
            let rem = chunks.into_remainder();
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
