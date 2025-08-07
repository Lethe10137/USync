pub mod mock;
pub mod real;

use bytes::Bytes;
use std::net::SocketAddr;

#[async_trait::async_trait]
pub trait UdpSocketLike: Send + Sync {
    async fn send_to(&self, bufs: &[Bytes], target: SocketAddr) -> std::io::Result<usize>;
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;
}
