use cpal::platform::AlsaStream as Stream;
use cpal::traits::{HostTrait, StreamTrait};
use rodio::DeviceTrait;

use crate::udp::RecordTx;

// TODO: opus enc

pub fn record(mut tx: RecordTx) -> anyhow::Result<Stream> {
    let host = cpal::platform::AlsaHost::new()?;
    let device = host.default_input_device().unwrap();
    tracing::info!("Input device: {}", device.name().unwrap());
    let config = device
        .supported_input_configs()
        .unwrap()
        .next()
        .unwrap()
        .with_sample_rate(cpal::SampleRate(16000));
    let err_fn = move |err| {
        tracing::error!("an error occurred on stream: {}", err);
    };
    let audio_stream = device.build_input_stream(
        &config.into(),
        move |data, _: &_| tx.send(data).expect(""),
        err_fn,
    )?;
    audio_stream.play()?;
    Ok(audio_stream)
}
