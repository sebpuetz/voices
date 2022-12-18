#[cfg(not(target_os = "windows"))]
use cpal::platform::AlsaStream as Stream;
#[cfg(not(target_os = "windows"))]
use cpal::platform::AlsaHost as Host;
#[cfg(target_os = "windows")]
use cpal::platform::WasapiStream as Stream;
#[cfg(target_os = "windows")]
use cpal::platform::WasapiHost as Host;

use cpal::traits::{HostTrait, StreamTrait};
use cpal::StreamConfig;
use rodio::DeviceTrait;

use crate::udp::RecordTx;

pub fn record(mut tx: RecordTx) -> anyhow::Result<Stream> {
    let host = Host::new()?;
    let device = host.default_input_device().unwrap();
    tracing::info!("Input device: {}", device.name().unwrap());
    let config = device
        .supported_input_configs()
        .unwrap()
        .next()
        .unwrap()
        .with_sample_rate(cpal::SampleRate(48000));
    let err_fn = move |err| {
        tracing::error!("an error occurred on stream: {}", err);
    };
    let config: StreamConfig = config.into();
    tracing::debug!("Input config: {:#?}", config);
    let audio_stream =
        device.build_input_stream(&config, move |data, _: &_| tx.send(data).expect(""), err_fn)?;
    audio_stream.play()?;
    Ok(audio_stream)
}
