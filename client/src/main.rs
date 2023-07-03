pub mod channels_api;
pub mod config;
pub mod mic;
mod play;
mod udp;

use std::net::SocketAddr;

use base64::Engine;
use clap::Parser;
use futures_util::FutureExt;
use tracing::subscriber::set_global_default;
use tracing_log::LogTracer;
use tracing_subscriber::prelude::*;
use uuid::Uuid;
use voices_voice_crypto::xsalsa20poly1305;
use voices_ws_proto::client::ControlStream;
use voices_ws_proto::*;
use xsalsa20poly1305::KeyInit;

use crate::config::Config;

fn main() -> anyhow::Result<()> {
    set_global_default(
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".into()),
            ))
            .with(tracing_subscriber::fmt::layer()),
    )?;
    LogTracer::init()?;
    let config = Config::parse();
    let client = channels_api::Client::new(config.gateway_url.parse()?)?;
    let servers = client.get_servers()?.info;
    let Some(srv) = read_choice(servers) else {return Ok(())};
    let server = client.get_server(srv.server_id)?;
    let Some(chan) = read_choice(server.channels) else {return Ok(())};
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async_main_(config, chan.channel_id))
}

fn read_choice<V: std::fmt::Debug>(mut list: Vec<V>) -> Option<V> {
    list.iter()
        .enumerate()
        .for_each(|(i, it)| println!("{}: {:#?}", i, it));
    let mut input = String::new();
    while let Ok(read) = std::io::stdin().read_line(&mut input) {
        if input.starts_with('q') || input.starts_with('Q') {
            break;
        }
        let stripped_input = input[..read].replace(char::is_whitespace, "");
        if let Ok(i) = stripped_input.parse::<usize>() {
            if list.len() > i {
                return Some(list.remove(i));
            } else {
                tracing::info!("list index out of bounds: {}", stripped_input)
            }
        } else {
            tracing::info!("not an integer: {}", stripped_input);
        }
        input.clear();
    }
    None
}

async fn async_main_(config: Config, channel_id: Uuid) -> anyhow::Result<()> {
    let (stream, _) = tokio_tungstenite::connect_async(config.ws_endpoint).await?;
    let mut stream = ControlStream::new(stream);
    let user_id = config.client_id.unwrap_or_else(Uuid::new_v4);
    stream.init(user_id, config.name).await?;
    let _ = dbg!(stream.await_initialized().await?);
    stream.join(channel_id).await?;
    let ServerAnnounce {
        ip,
        port,
        source_id,
    } = stream.await_voice_udp().await?;
    let remote_udp = SocketAddr::from((ip, port));
    tracing::debug!("connecting UDP to {}", remote_udp);
    let mut udp = udp::UdpSetup::new(remote_udp, user_id).await?;
    let mut attempt = 0;
    loop {
        if attempt == 5 {
            anyhow::bail!("failed ip disco");
        }
        attempt += 1;
        match udp.discover_ip(source_id).await {
            Ok(slf_addr) => {
                stream.announce_udp(slf_addr).await?;
                break;
            }
            Err(_) => continue,
        }
    }
    let ready = stream.await_ready().await?;
    let key = base64::engine::general_purpose::STANDARD.decode(ready.crypt_key.unsecure())?;
    let cipher = xsalsa20poly1305::XSalsa20Poly1305::new_from_slice(&key)?;
    tracing::info!("{:?}", ready);
    let voice_event_tx = udp.run(ready.src_id, cipher, config.deaf, config.mute)?;
    for user in ready.present {
        if user.source_id == ready.src_id {
            continue;
        }
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
            ServerEvent::Keepalive(_)
            | ServerEvent::Init(_)
            | ServerEvent::Ready(_)
            | ServerEvent::UdpAnnounce(_) => continue,
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
            ServerEvent::Disconnected(_) => {
                anyhow::bail!("voice disconnected");
            }
        }
    }
}
