use cpal::InputCallbackInfo;
use cpal::traits::HostTrait;
use crossbeam::channel::Sender;
use rodio::DeviceTrait;

mod voice;

// fn mic_input() {
    
//     let (tx, rx) = crossbeam::channel::bounded(25);
//     let host = cpal::default_host();
//     let device = host.default_input_device().unwrap();
//     eprintln!("Input device: {}", device.name().unwrap());
//     let config = device
//         .supported_input_configs()
//         .unwrap()
//         .next()
//         .unwrap()
//         .with_sample_rate(cpal::SampleRate(16000));
//     let err_fn = move |err| {
//         eprintln!("an error occurred on stream: {}", err);
//     };
//     let audio_stream = match config.sample_format() {
//         cpal::SampleFormat::F32 => device.build_input_stream(
//             &config.into(),
//             move |data, b: &InputCallbackInfo| write_input_data::<f32, i16>(data, tx.clone()),
//             err_fn,
//         )?,
//         cpal::SampleFormat::I16 => device.build_input_stream(
//             &config.into(),
//             move |data, _: &_| write_input_data::<i16, i16>(data, tx.clone()),
//             err_fn,
//         )?,
//         cpal::SampleFormat::U16 => device.build_input_stream(
//             &config.into(),
//             move |data, _: &_| write_input_data::<u16, i16>(data, tx.clone()),
//             err_fn,
//         )?,
//     };
//     audio_stream.play()?;
// }

fn write_input_data<T, U>(input: &[T], tx: Sender<Vec<U>>)
where
    T: cpal::Sample,
    U: cpal::Sample + std::fmt::Debug,
{
    let samples: Vec<U> = input.iter().map(cpal::Sample::from).collect::<_>();
    // eprintln!("{:?}", samples.len());
    tx.send(samples).expect("");
    // for &sample in input.iter() {
    //     let sample: U = cpal::Sample::from(&sample);
    //     writer.write_sample(sample).ok();
    // }
}
