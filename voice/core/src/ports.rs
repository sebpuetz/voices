use std::net::{Ipv6Addr, SocketAddr};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::net::UdpSocket;

#[derive(Clone, Debug)]
pub struct Ports {
    start: u16,
    state: Vec<Arc<AtomicBool>>,
}

impl Ports {
    pub async fn new(mut start: u16, limit: u16) -> std::io::Result<Self> {
        if start == 0 {
            let addr = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).await?;
            start = addr.local_addr()?.port();
        }
        let state = (0..limit)
            .map(|_| Arc::new(AtomicBool::new(true)))
            .collect();
        Ok(Self { start, state })
    }

    pub fn get(&self) -> Option<PortRef> {
        for (i, state) in self.state.iter().enumerate() {
            tracing::warn!("{i}, ports: {:?}", self);
            if state.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return Some(PortRef {
                    port: i as u16 + self.start,
                    taken: state.clone(),
                });
            }
        }
        None
    }

    pub fn start(&self) -> u16 {
        self.start
    }

    pub fn total(&self) -> u16 {
        self.state.len() as u16
    }
}

#[derive(Debug)]
pub struct PortRef {
    pub port: u16,
    taken: Arc<AtomicBool>,
}

impl Drop for PortRef {
    fn drop(&mut self) {
        tracing::debug!("Opening port {}", self.port);
        self.taken.store(true, std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod test {
    use super::Ports;

    #[tokio::test]
    async fn test_ports() {
        let ports = Ports::new(0, 2).await.unwrap();
        let start = ports.start;
        let port = ports.get().unwrap();
        assert_eq!(port.port, start);
        let state1 = port.taken.clone();
        assert!(!state1.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!ports.state[0].load(std::sync::atomic::Ordering::SeqCst));
        assert!(ports.state[1].load(std::sync::atomic::Ordering::SeqCst));

        let port2 = ports.get().unwrap();
        let state2 = port2.taken.clone();
        assert!(!state2.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!ports.state[0].load(std::sync::atomic::Ordering::SeqCst));
        assert!(!ports.state[1].load(std::sync::atomic::Ordering::SeqCst));

        drop(port);
        assert!(state1.load(std::sync::atomic::Ordering::SeqCst));
        assert!(ports.state[0].load(std::sync::atomic::Ordering::SeqCst));
        assert!(!ports.state[1].load(std::sync::atomic::Ordering::SeqCst));

        drop(port2);
        assert!(state2.load(std::sync::atomic::Ordering::SeqCst));
        assert!(ports.state[1].load(std::sync::atomic::Ordering::SeqCst))
    }
}
