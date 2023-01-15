use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Context;
use voices_channels::grpc::proto::channels_server::Channels;

use crate::{Ports, VoiceServerImpl};

#[derive(clap::Parser, Debug)]
pub struct VoiceServerConfig {
    #[clap(long, default_value_t = 33333, env)]
    first_udp_port: u16,
    #[clap(long, default_value_t = 2, env)]
    udp_ports: u16,
    #[clap(long, default_value = "localhost", env)]
    udp_host: String,
}

impl VoiceServerConfig {
    pub fn udp_ports(&self) -> Ports {
        Ports::new(self.first_udp_port, self.udp_ports)
    }

    pub async fn udp_host(&self) -> anyhow::Result<IpAddr> {
        Ok(tokio::net::lookup_host((&*self.udp_host, 0))
            .await
            .context("Failed to resolve UDP host")?
            .next()
            .context("Failed to resolve UDP host")?
            .ip())
    }

    pub async fn server(&self, channels: Arc<dyn Channels>) -> anyhow::Result<VoiceServerImpl> {
        let ip = self.udp_host().await?;
        let ports = self.udp_ports();
        Ok(VoiceServerImpl::new(ip, ports, channels))
    }
}
