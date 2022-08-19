use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Ports {
    start: u16,
    state: Vec<Arc<AtomicBool>>,
}

impl Ports {
    pub fn new(start: u16, limit: u16) -> Self {
        let state = (0..limit)
            .map(|_| Arc::new(AtomicBool::new(true)))
            .collect();
        Self { start, state }
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

    #[test]
    fn test_ports() {
        let ports = Ports::new(0, 2);
        let port = ports.get().unwrap();
        assert_eq!(port.port, 0);
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
