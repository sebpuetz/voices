use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use prost::Message;
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::Instant;
use uuid::Uuid;

use crate::voice::{
    server_message, ClientMessage, IpDiscoveryRequest, IpDiscoveryResponse, Ping, ServerMessage,
    ServerVoice,
};

// pub async fn set_up_voice(target: SocketAddr, id: Uuid) -> anyhow::Result<SocketAddr> {
//     let slf_addr = SocketAddr::from(([0, 0, 0, 0], 0));
//     let udp = UdpSocket::bind(slf_addr).await?;
//     udp.connect(target).await?;
//     let mut buf = vec![0; 1500];
//     let req = IpDiscoveryRequest {
//         uuid: id.as_bytes().to_vec(),
//     };
//     req.encode(&mut buf)?;
//     let mut remaining = 5;
//     while remaining > 0 {
//         match try_discovery(&mut udp, &mut buf, req.encoded_len()).await {
//             Ok(addr) => return Ok(addr),
//             Err(e) => tracing::warn!("ip discovery failed"),
//         }
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         remaining -= 1;
//     }
// }

// async fn try_discovery(
//     udp: &mut UdpSocket,
//     buf: &mut [u8],
//     len: usize,
// ) -> anyhow::Result<SocketAddr> {
//     udp.send(&buf[..len]).await?;

//     let len = udp.recv(&mut buf).await?;
//     let IpDiscoveryResponse { ip, port } = IpDiscoveryResponse::decode(&buf[..len])?;
//     let ip = IpAddr::from_str(ip.as_str())?;
//     let port = port as u16;
//     Ok(SocketAddr::from((ip, port)))
// }

async fn udp_tx(
    sock: Arc<UdpSocket>,
    mut voice_recv: mpsc::Receiver<Vec<u8>>,
    get_received: Arc<AtomicU64>,
) -> anyhow::Result<()> {
    let mut buf = vec![0; 1650];
    let mut deadline = Instant::now();
    let keepalive_interval = Duration::from_secs(1);
    deadline += keepalive_interval;
    loop {
        let sleep_until = tokio::time::sleep_until(deadline);
        select! {
            voice = voice_recv.recv() => {
                if let Some(voice) = voice {
                    let sequence = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let payload = crate::voice::Voice {
                        payload: voice,
                        sequence: sequence as _,
                    };
                    let voice = ClientMessage::voice(payload);
                    voice.encode(&mut buf)?;
                    sock.send(&buf[..voice.encoded_len()]).await?;
                }
            }
            _ = sleep_until => {
                let seq = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis();
                let recv = get_received.load(std::sync::atomic::Ordering::SeqCst);
                let ping = ClientMessage::ping(Ping {
                    seq: seq as _,
                    recv,
                });
                ping.encode(&mut buf)?;
                sock.send(&buf[..ping.encoded_len()]).await?;
            }
        }
    }
    Ok(())
}

async fn udp_rx(
    sock: Arc<UdpSocket>,
    decoder_tx: crossbeam::channel::Sender<ServerVoice>,
    set_received: Arc<AtomicU64>,
) -> anyhow::Result<()> {
    let mut buf = vec![0; 1600];
    loop {
        let len = match sock.recv(&mut buf).await {
            Ok(len) => len,
            Err(e) => {
                tracing::error!("recv error {}", e);
                continue;
            }
        };
        let msg = ServerMessage::decode(&buf[..len])?;
        set_received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match msg {
            ServerMessage {
                message: Some(server_message::Message::Voice(voice)),
            } => match decoder_tx.try_send(voice) {
                Ok(()) => (),
                Err(crossbeam::channel::TrySendError::Full(_)) => {
                    tracing::warn!("voice receiver is stalled, dropping package");
                }
                Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                    anyhow::bail!("receiver is gone");
                }
            },
            ServerMessage {
                message: Some(server_message::Message::Pong(pong)),
            } => {
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
                let received = set_received.load(std::sync::atomic::Ordering::SeqCst);
                let expected = pong.sent;
                tracing::info!(
                    "Pong: {}, received: {}, expected: {}",
                    (now - Duration::from_millis(pong.seq)).as_millis(),
                    received,
                    expected
                );
            }
            ServerMessage { message: None } => {
                anyhow::bail!("fucked up");
            }
        }
    }
}
