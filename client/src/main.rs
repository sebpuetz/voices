mod voice;
use std::collections::VecDeque;
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use cpal::traits::HostTrait;
use hound::WavSpec;
use prost::Message as _;
use rodio::DeviceTrait;
use serde::{Deserialize, Serialize};
use tracing_subscriber::prelude::*;
use tungstenite::Message;
use uuid::Uuid;

use crate::voice::*;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let (mut stream, _) = tungstenite::connect(format!("ws://localhost:3001"))?;
    let id = Uuid::new_v4();
    stream.write_message(Message::Text(serde_json::to_string(&Init {
        id: id.to_string(),
    })?))?;
    let msg = stream.read_message()?;
    let Announce { ip, port } = serde_json::from_str::<Announce>(msg.to_text()?)?;
    let slf_addr = SocketAddr::from(([0, 0, 0, 0], 0));
    let udp = UdpSocket::bind(slf_addr)?;
    let remote_addr = SocketAddr::from((ip, port));

    udp.connect(remote_addr)?;
    // udp.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut buf = [0; 1500];
    let req = IpDiscoveryRequest {
        uuid: id.as_bytes().to_vec(),
    };
    req.encode(&mut buf.as_mut())?;
    udp.send_to(&buf[..req.encoded_len()], remote_addr)?;

    // udp.set_read_timeout(Some(Duration::from_millis(500)))?;
    let len = udp.recv(&mut buf)?;
    let IpDiscoveryResponse { ip, port } = IpDiscoveryResponse::decode(&buf[..len])?;

    stream.write_message(Message::Text(dbg!(serde_json::to_string(&Announce {
        ip: ip.parse()?,
        port: port as _,
    })?)))?;
    let msg = stream.read_message()?;
    let Ready { id } = serde_json::from_str::<Ready>(msg.to_text()?)?;
    println!("{}", id);
    let rx_udp = Arc::new(udp);
    let tx_udp = rx_udp.clone();
    let tx_udp_ctl = rx_udp.clone();
    let get_received = Arc::new(AtomicU64::new(0));
    let set_received = get_received.clone();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut rdr = hound::WavReader::new(std::io::BufReader::new(
        std::fs::File::open("common_voice_de_20237340.wav").unwrap(),
    ))
    .unwrap();
    let data = rdr
        .samples::<i16>()
        .flat_map(|s| s.unwrap().to_be_bytes())
        .collect::<Vec<u8>>();

    let tx_udp = std::thread::spawn(move || {
        // let mut sequence = 0;
        loop {
            let sequence = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis();
            for chunk in data.chunks(400 * 2) {
                let payload = crate::voice::Voice {
                    payload: chunk.to_vec(),
                    sequence: sequence as _,
                };
                let voice = ClientMessage::voice(payload);
                voice.encode(&mut buf.as_mut())?;
                tx_udp.send(&buf[..voice.encoded_len()])?;
                // sequence += 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
        #[allow(unreachable_code)]
        {
            Ok::<(), anyhow::Error>(())
        }
    });
    std::thread::spawn(move || {
        loop {
            let seq = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis();
            let recv = get_received.load(std::sync::atomic::Ordering::SeqCst);
            let ping = ClientMessage::ping(Ping {
                seq: seq as _,
                recv,
            });
            ping.encode(&mut buf.as_mut())?;
            tx_udp_ctl.send(&buf[..ping.encoded_len()])?;
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        #[allow(unreachable_code)]
        {
            Ok::<(), anyhow::Error>(())
        }
    });
    let (tx, rx) = crossbeam::channel::unbounded();
    let rx_udp = std::thread::spawn(move || {
        struct State {
            start: Instant,
            queue: Vec<ServerVoice>,
            received: Arc<AtomicU64>,
        }
        impl State {
            fn push(&mut self, voice: ServerVoice) -> bool {
                self.queue.push(voice);
                self.received
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                Instant::now() > self.start + Duration::from_millis(100)
            }

            fn drain<'a>(&'a mut self) -> std::vec::Drain<'a, ServerVoice> {
                self.queue.sort_by(|v1, v2| v1.sequence.cmp(&v2.sequence));
                let ret = self.queue.drain(..);
                self.start = Instant::now();
                ret
            }
        }
        let mut buf = [0; 1500];
        let mut state = State {
            start: Instant::now(),
            queue: Vec::with_capacity(10),
            received: set_received,
        };
        loop {
            let len = match rx_udp.recv(&mut buf) {
                Ok(len) => len,
                Err(e) => {
                    tracing::error!("recv error {}", e);
                    continue;
                }
            };
            match ServerMessage::decode(&buf[..len]) {
                Ok(ServerMessage {
                    message: Some(server_message::Message::Voice(voice)),
                }) => {
                    if state.push(voice) {
                        let drain = state.drain();
                        tracing::info!("forwarding {} packets", drain.len());
                        for voice in drain {
                            tx.send(voice).unwrap();
                        }
                    }
                }

                Ok(ServerMessage {
                    message: Some(server_message::Message::Pong(pong)),
                }) => {
                    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
                    let received = state.received.load(std::sync::atomic::Ordering::SeqCst);
                    let expected = pong.sent;
                    tracing::info!(
                        "Pong: {}, received: {}, expected: {}",
                        (now - Duration::from_millis(pong.seq)).as_millis(),
                        received,
                        expected
                    );
                }
                Ok(ServerMessage { message: None }) => {
                    anyhow::bail!("fucked up");
                }
                Err(e) => return Err(e.into()),
            }
        }
        #[allow(unreachable_code)]
        {
            Ok::<(), anyhow::Error>(())
        }
    });
    std::thread::spawn(move || {
        struct Rx {
            chan: crossbeam::channel::Receiver<ServerVoice>,
            buf: std::vec::IntoIter<u8>,
        }
        impl Iterator for Rx {
            type Item = i16;

            fn next(&mut self) -> Option<Self::Item> {
                loop {
                    match self
                        .buf
                        .next()
                        .and_then(|v| self.buf.next().map(|v2| i16::from_be_bytes([v, v2])))
                    {
                        Some(nxt) => return Some(nxt),
                        None => {
                            tracing::info!("{}", self.chan.len());
                            let packs = self.chan.recv().unwrap();
                            self.buf = packs.payload.into_iter();
                        }
                    }
                }
            }
        }
        impl rodio::Source for Rx {
            fn current_frame_len(&self) -> Option<usize> {
                None
            }

            fn channels(&self) -> u16 {
                1
            }

            fn sample_rate(&self) -> u32 {
                16000
            }

            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
        let rx = Rx {
            chan: rx,
            buf: Vec::new().into_iter(),
        };
        use rodio::{source::Source, Decoder, OutputStream};
        let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        let config = device.default_output_config().unwrap();
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let writer = hound::WavWriter::new(
            Cursor::new(Vec::new()),
            WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        let sink = rodio::Sink::try_new(&stream_handle).unwrap();
        sink.append(rx);
        sink.sleep_until_end();
    });
    let t2 = std::thread::spawn(move || {
        loop {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap();
            stream.write_message(Message::Text(serde_json::to_string(&Event::Keepalive {
                sent_at: now.as_millis() as _,
            })?))?;
            let evt: Event = serde_json::from_str(stream.read_message()?.to_text()?)?;
            match evt {
                Event::Keepalive { sent_at } => {
                    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
                    tracing::debug!(
                        "WS Ping: {}",
                        (now - Duration::from_millis(sent_at)).as_secs_f32()
                    );
                }
            }
            std::thread::sleep(Duration::from_millis(400));
            // stream.close(None)?;
        }
        #[allow(unreachable_code)]
        {
            Ok::<(), anyhow::Error>(())
        }
    });
    tx_udp.join().unwrap().unwrap();
    rx_udp.join().unwrap().unwrap();
    t2.join().unwrap().unwrap();
    // stream.write_u16::<BigEndian>(port);
    // udp.connect(SocketAddr::from(([127, 0, 0, 1], port)))?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub enum Event {
    Keepalive { sent_at: u64 },
}

#[derive(Serialize, Deserialize)]
pub struct Init {
    id: String,
}

#[derive(Serialize, Deserialize)]
pub struct Announce {
    ip: IpAddr,
    port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct Ready {
    id: Uuid,
}

impl ClientMessage {
    pub fn new(msg: client_message::Payload) -> Self {
        ClientMessage { payload: Some(msg) }
    }

    pub fn ping(ping: Ping) -> Self {
        ClientMessage::new(client_message::Payload::Ping(ping))
    }

    pub fn voice(voice: Voice) -> Self {
        ClientMessage::new(client_message::Payload::Voice(voice))
    }
}
