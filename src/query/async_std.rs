//! [`async-std`](https://docs.rs/async-std/*/async_std) implementation of the Query protocol.
//!
//! Uses [`async_std::net::UdpSocket`](https://docs.rs/async-std/*/async_std/net/struct.UdpSocket.html) for sending and receiving UDP data

use ::async_std::{
    future::timeout,
    net::{ToSocketAddrs, UdpSocket},
};
use std::{io, net::Ipv4Addr, time::Duration};

use super::*;

/// An asynchronous Query client using the [`async-std`](https://docs.rs/async-std/*/async_std) networking primitives.
#[derive(Debug)]
pub struct QueryClient {
    socket: UdpSocket,
    session_id: u32,
    timeout: Option<Duration>,
}

impl QueryClient {
    /// Build a new QueryClient from the given IP address.
    ///
    /// If not port is specified in the IP address, the [default port](DEFAULT_PORT) is used.
    ///
    /// The default [timeout duration](DEFAULT_TIMEOUT) is used.
    pub async fn new(ip: &str) -> io::Result<Self> {
        let (ip, port) = if let Some((ip, port)) = ip.split_once(':') {
            (
                ip,
                port.parse::<u16>().map_err(|_| {
                    io::Error::new(io::ErrorKind::Other, "Invalid port in IP address")
                })?,
            )
        } else {
            (ip, DEFAULT_PORT)
        };

        Self::new_with_port(ip, port).await
    }

    /// Build a new QueryClient from the given IP address and port.
    ///
    /// If the IP address already contains a port, an error is returned.
    ///
    /// The default [timeout duration](DEFAULT_TIMEOUT) is used.
    pub async fn new_with_port(ip: &str, port: u16) -> io::Result<Self> {
        Self::new_with_socket_address(ip, port, (Ipv4Addr::UNSPECIFIED, 0), Some(DEFAULT_TIMEOUT))
            .await
    }

    /// Builds a new QueryClient from the given IP address, port, socket address and optional timeout.
    ///
    /// The IP adress must not contain a port.
    pub async fn new_with_socket_address(
        ip: &str,
        port: u16,
        addr: impl ToSocketAddrs,
        timeout: Option<Duration>,
    ) -> io::Result<Self> {
        if ip.contains(':') {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid IP address: must not contain a port.",
            ));
        }

        let socket = UdpSocket::bind(addr).await?;
        socket
            .connect(ip.to_string() + ":" + &port.to_string())
            .await?;

        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time cannot be before UNIX_EPOCH")
            .as_nanos() as u32;

        Ok(Self {
            socket,
            session_id,
            timeout,
        })
    }

    /// Receive a UDP packet from the client socket.
    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let fut = self.socket.recv(buf);
        if let Some(duration) = self.timeout {
            timeout(duration, fut).await.map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "UDP async recv call timed out.")
            })?
        } else {
            fut.await
        }
    }

    /// Send a UDP handshake packet to the client socket.
    ///
    /// Receive and parse the response into a Query token, valid up to 30 seconds.
    pub async fn handshake(&self) -> io::Result<Token> {
        let handshake = packets::Handshake::new(self.session_id);
        self.socket.send(&handshake).await?;

        let mut buf = [0; HANDSHAKE_RESPONSE_SIZE];
        let received = self.recv(&mut buf).await?;

        Ok(Token::from_payload(
            buf.get(RESPONSE_HEADER_SIZE..received)
                .ok_or_else(not_enough_data)?,
        ))
    }

    /// Request and wait for a basic status packet on the client socket.
    ///
    /// If the token is no longer valid, no packet is received and an error is returned.
    pub async fn basic_stat(&self, token: Token) -> std::io::Result<BasicStat> {
        let request = packets::BasicStat::new(self.session_id, token.0);
        self.socket.send(&request).await?;

        let mut buf = vec![0; BASIC_STAT_RESPONSE_SIZE];
        let received = self.recv(&mut buf).await?;

        BasicStat::from_payload(
            buf.get(RESPONSE_HEADER_SIZE..received)
                .ok_or_else(not_enough_data)?,
        )
    }

    /// Request and wait for a full status packet on the client socket.
    ///
    /// If the token is no longer valid, no packet is received and an error is returned.
    pub async fn full_stat(&self, token: Token) -> std::io::Result<FullStat> {
        let request = packets::FullStat::new(self.session_id, token.0);
        self.socket.send(&request).await?;

        let mut buf = vec![0; FULL_STAT_RESPONSE_SIZE];
        let received = self.recv(&mut buf).await?;

        FullStat::from_payload(
            buf.get(RESPONSE_HEADER_SIZE..received)
                .ok_or_else(not_enough_data)?,
        )
    }
}

/// Convenience function to get a full status packet on the client socket.
///
/// Send a handshake first, and if a token is successfully received and parsed,
/// request a full status packet.
pub async fn query(ip: &str) -> io::Result<FullStat> {
    let client = QueryClient::new(ip).await?;
    let token = client.handshake().await?;

    client.full_stat(token).await
}

#[cfg(test)]
mod tests {
    const TEST_IP: &str = "lotr.g.akliz.net:25565";

    #[tokio::test]
    async fn test_handshake() {
        let client = super::QueryClient::new(TEST_IP).await.unwrap();
        client.handshake().await.unwrap();
    }

    #[tokio::test]
    async fn test_basic_stat() {
        let client = super::QueryClient::new(TEST_IP).await.unwrap();
        let token = client.handshake().await.unwrap();

        let basic_stat = client.basic_stat(token).await.unwrap();
        assert_eq!(basic_stat.hostport, crate::query::DEFAULT_PORT);
    }

    #[tokio::test]
    async fn test_full_stat() {
        let full_stat = super::query(TEST_IP).await.unwrap();

        assert_eq!(full_stat.hostport, crate::query::DEFAULT_PORT);
        assert_eq!(full_stat.numplayers as usize, full_stat.player_list.len());
        assert_eq!(full_stat.version, "1.7.10");
        assert_eq!(full_stat.game_id, "MINECRAFT");
    }
}
