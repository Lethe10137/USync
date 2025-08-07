use super::UdpSocketLike;
use async_trait::async_trait;
use bytes::Bytes;
use flume::{Receiver, Sender};
use std::net::SocketAddr;

#[derive(Clone)]
pub struct MockSocket {
    sender: Sender<(Bytes, SocketAddr)>,
    receiver: Receiver<(Bytes, SocketAddr)>,
    local_addr: SocketAddr,
}

impl MockSocket {
    pub fn pair(addr1: SocketAddr, addr2: SocketAddr) -> (Self, Self) {
        // Duplex：A -> B, B -> A
        let (tx1, rx1) = flume::unbounded::<(Bytes, SocketAddr)>();
        let (tx2, rx2) = flume::unbounded::<(Bytes, SocketAddr)>();

        let socket1 = MockSocket {
            sender: tx1,
            receiver: rx2,
            local_addr: addr1,
        };
        let socket2 = MockSocket {
            sender: tx2,
            receiver: rx1,
            local_addr: addr2,
        };

        (socket1, socket2)
    }
}

#[async_trait]
impl UdpSocketLike for MockSocket {
    async fn send_to(&self, bufs: &[Bytes], target: SocketAddr) -> std::io::Result<usize> {
        // Splice the parts together to simulate the payload of a UDP packet.
        let total_len: usize = bufs.iter().map(|b| b.len()).sum();
        let combined = if bufs.len() == 1 {
            bufs[0].clone()
        } else {
            let mut v = Vec::with_capacity(total_len);
            for b in bufs {
                v.extend_from_slice(b);
            }
            Bytes::from(v)
        };
        self.sender
            .send_async((combined, target))
            .await
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    format!("channel closed: {e}"),
                )
            })?;
        Ok(total_len)
    }

    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        let (data, from) = self.receiver.recv_async().await.map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("channel closed: {e}"),
            )
        })?;

        let len = data.len().min(buf.len());
        buf[..len].copy_from_slice(&data[..len]);

        Ok((len, from))
    }
}

#[tokio::test]
async fn test_mock_socket_pair() -> std::io::Result<()> {
    use std::net::SocketAddr;

    let addr1: SocketAddr = "127.0.0.1:10000".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:10001".parse().unwrap();

    let (sock1, sock2) = MockSocket::pair(addr1, addr2);

    // sock1 Send
    let send_data = vec![Bytes::from("hello"), Bytes::from(" world")];
    let sent = sock1.send_to(&send_data, addr2).await?;
    assert_eq!(sent, 11);

    // sock2 Receive
    let mut buf = vec![0u8; 100];
    let (len, from) = sock2.recv_from(&mut buf).await?;
    let received_str = std::str::from_utf8(&buf[..len]).unwrap();

    assert_eq!(received_str, "hello world");
    assert_eq!(from, addr2);

    Ok(())
}
