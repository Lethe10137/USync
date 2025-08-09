use bytes::Bytes;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::IoSlice;
use std::net::SocketAddr;
use tokio::net::UdpSocket as TokioUdpSocket;

use super::UdpSocketLike;

pub struct RealUdpSocket {
    innner_raw: Socket,
    inner_tokio: TokioUdpSocket,
}

impl RealUdpSocket {
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let domain = match addr {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        };
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;

        socket.bind(&addr.into())?;
        let std_socket = socket.try_clone()?.into();
        let tokio_socket = TokioUdpSocket::from_std(std_socket)?;

        Ok(Self {
            inner_tokio: tokio_socket,
            innner_raw: socket,
        })
    }
}

#[async_trait::async_trait]
impl UdpSocketLike for RealUdpSocket {
    async fn send_to(&self, bufs: &[Bytes], target: SocketAddr) -> std::io::Result<usize> {
        let io_slice = bufs
            .iter()
            .map(|slice| IoSlice::new(slice))
            .collect::<Vec<_>>();

        self.innner_raw
            .send_to_vectored(io_slice.as_slice(), &SockAddr::from(target))
    }

    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.inner_tokio.recv_from(buf).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn test_real_udp_socket_send_recv() -> std::io::Result<()> {
        // 创建两个 socket，分别绑定到不同的端口
        let recv_addr: SocketAddr = "127.0.0.1:40001".parse().unwrap();
        let send_addr: SocketAddr = "127.0.0.1:40002".parse().unwrap();

        let receiver = RealUdpSocket::bind(recv_addr).await?;
        let sender = RealUdpSocket::bind(send_addr).await?;

        // 要发送的数据
        let data = vec![Bytes::from_static(b"Hello, "), Bytes::from_static(b"UDP!")];

        // 启动接收任务
        let recv_task = tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let (len, from) = receiver.recv_from(&mut buf).await.unwrap();
            let received = &buf[..len];
            (received.to_vec(), from)
        });

        // 稍微等一下让接收端准备好
        sleep(Duration::from_millis(100)).await;

        // 发送数据
        let bytes_sent = sender.send_to(&data, recv_addr).await?;
        assert_eq!(bytes_sent, b"Hello, UDP!".len());

        // 获取接收结果
        let (received, from) = recv_task.await.unwrap();

        assert_eq!(received, b"Hello, UDP!");
        assert_eq!(from.ip(), send_addr.ip()); // 可以比较 IP，端口可能是动态的

        Ok(())
    }
}
