use std::net::IpAddr;
use std::sync::Arc;

use crate::registry::Register;
use crate::{Ports, VoiceServerImpl};
use anyhow::Context;

#[derive(clap::Parser, Debug)]
pub struct VoiceServerConfig {
    #[clap(long, default_value_t = 33333, env)]
    pub first_udp_port: u16,
    #[clap(long, default_value_t = 2, env)]
    pub udp_ports: u16,
    #[clap(long, default_value = "localhost", env)]
    pub udp_host: String,
}

impl VoiceServerConfig {
    pub async fn udp_ports(&self) -> anyhow::Result<Ports> {
        let start = self.first_udp_port;
        let limit = self.udp_ports;
        let ports = Ports::new(start, limit).await?;
        Ok(ports)
    }

    pub async fn udp_host(&self) -> anyhow::Result<IpAddr> {
        Ok(tokio::net::lookup_host((&*self.udp_host, 0))
            .await
            .context("Failed to resolve UDP host")?
            .next()
            .context("Failed to resolve UDP host")?
            .ip())
    }

    pub async fn server(&self) -> anyhow::Result<VoiceServerImpl> {
        let ip = self.udp_host().await?;
        let ports = self.udp_ports().await?;
        Ok(VoiceServerImpl::new(ip, ports, None))
    }

    pub async fn server_with_registry(
        &self,
        registry: Arc<dyn Register>,
    ) -> anyhow::Result<VoiceServerImpl> {
        let ip = self.udp_host().await?;
        let ports = self.udp_ports().await?;
        Ok(VoiceServerImpl::new(ip, ports, Some(registry)))
    }
}
