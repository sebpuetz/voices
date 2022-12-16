pub mod mic;
mod play;
mod udp;
mod ws;

use std::net::SocketAddr;

use clap::Parser;
use futures_util::FutureExt;
use tracing_subscriber::prelude::*;
use uuid::Uuid;

use ws::ControlStream;
use ws_proto::*;

#[derive(Parser)] // requires `derive` feature
#[clap(name = "voice-client")]
#[clap(author, version, about, long_about = None)]
struct Config {
    #[clap(long, default_value = "ws://localhost:33332")]
    ws_endpoint: String,
    #[clap(long)]
    room_id: Option<Uuid>,
    #[clap(long, default_value = "Foo")]
    name: String,
    #[clap(long)]
    deaf: bool,
    #[clap(long)]
    mute: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    rt.block_on(async_main_())
}

async fn async_main_() -> anyhow::Result<()> {
    let config = Config::parse();
    let (stream, _) = tokio_tungstenite::connect_async(config.ws_endpoint).await?;
    let mut stream = ControlStream::new(stream);
    let user_id = Uuid::new_v4();
    stream.init(user_id, config.name).await?;

    let channel_id = config.room_id.unwrap_or_else(Uuid::new_v4);
    stream.join(channel_id).await?;
    let Announce { ip, port } = stream.await_voice_udp().await?;
    let remote_udp = SocketAddr::from((ip, port));
    tracing::debug!("connecting UDP to {}", remote_udp);
    let mut udp = udp::UdpSetup::new(remote_udp, user_id).await?;
    let mut attempt = 0;
    loop {
        if attempt == 5 {
            anyhow::bail!("failed ip disco");
        }
        attempt += 1;
        match udp.discover_ip().await {
            Ok(slf_addr) => {
                stream.announce_udp(slf_addr).await?;
                break;
            }
            Err(_) => continue,
        }
    }
    let ready = stream.await_ready().await?;
    tracing::info!("{:?}", ready);
    let voice_event_tx = udp.run(ready.src_id, config.deaf, config.mute);
    for user in ready.present {
        tracing::info!("{:?}", user);
        voice_event_tx.already_present(user.source_id).await;
    }
    let mut stop = tokio::signal::ctrl_c().boxed();
    loop {
        let evt = tokio::select! {
            _ = &mut stop => {
                stream.leave().await?;
                stream.stop().await?;
                return Ok(())
            }
            evt = stream.next_event() => {
                evt
            }
        };
        match evt? {
            ServerEvent::Keepalive(_) | ServerEvent::Ready(_) | ServerEvent::UdpAnnounce(_) => {
                continue
            }
            ServerEvent::JoinError(err) => {
                tracing::info!("Failed to join: {:?}", err);
                stream.stop().await?;
                return Ok(());
            }
            ServerEvent::Joined(joined) => {
                voice_event_tx.joined(joined.source_id).await;
                tracing::info!("{:?}", joined);
            }
            ServerEvent::Left(left) => {
                voice_event_tx.left(left.source_id).await;
                tracing::info!("{:?}", left);
            }
        }
    }
}
