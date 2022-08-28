use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;

pub struct UdpWithBuf {
    sock: Arc<UdpSocket>,
    buf: Vec<u8>,
}

impl Clone for UdpWithBuf {
    fn clone(&self) -> Self {
        Self {
            sock: self.sock.clone(),
            buf: vec![0; self.buf.len()],
        }
    }
}

impl UdpWithBuf {
    const DEFAULT_BUF_SIZE: usize = 1600;

    pub async fn bind(addr: SocketAddr) -> Result<Self, UdpError> {
        let sock = UdpSocket::bind(addr).await?;
        Ok(Self::new(sock))
    }

    pub fn new(sock: UdpSocket) -> Self {
        Self::new_with_buf_size(sock, Self::DEFAULT_BUF_SIZE)
    }

    pub fn new_with_buf_size(sock: UdpSocket, buf_size: usize) -> Self {
        Self {
            sock: Arc::new(sock),
            buf: vec![0; buf_size],
        }
    }

    pub async fn connect(&self, addr: SocketAddr) -> Result<(), UdpError> {
        self.sock.connect(addr).await?;
        Ok(())
    }

    pub async fn send<M: prost::Message>(&mut self, message: &M) -> Result<(), UdpError> {
        let len = message.encoded_len();
        let mut buf = &mut *self.buf;
        message.encode(&mut buf)?;
        self.sock.send(&self.buf[..len]).await?;
        Ok(())
    }
    pub async fn send_to<M: prost::Message>(
        &mut self,
        message: &M,
        target: SocketAddr,
    ) -> Result<(), UdpError> {
        let len = message.encoded_len();
        let mut buf = &mut *self.buf;
        message.encode(&mut buf)?;
        self.sock.send_to(&self.buf[..len], target).await?;
        Ok(())
    }

    pub async fn recv<M: prost::Message + Default>(&mut self) -> Result<M, UdpError> {
        let len = self.sock.recv(&mut self.buf).await?;
        M::decode(&self.buf[..len]).map_err(Into::into)
    }

    pub async fn recv_from<M: prost::Message + Default>(
        &mut self,
    ) -> Result<(SocketAddr, M), UdpError> {
        let (len, addr) = self.sock.recv_from(&mut self.buf).await?;
        M::decode(&self.buf[..len])
            .map_err(Into::into)
            .map(|m| (addr, m))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum UdpError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Failed to decode: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Failed to decode: {0}")]
    EncodeError(#[from] prost::EncodeError),
}

impl UdpError {
    pub fn fatal(&self) -> bool {
        match self {
            UdpError::IoError(io) => match io.kind() {
                std::io::ErrorKind::NotFound
                | std::io::ErrorKind::PermissionDenied
                | std::io::ErrorKind::AddrInUse
                | std::io::ErrorKind::ConnectionRefused => true,
                _ => false,
            },
            UdpError::DecodeError(_) => true,
            UdpError::EncodeError(_) => true,
        }
    }
}
