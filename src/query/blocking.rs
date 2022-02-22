//! Blocking implementation of the Query protocol.
//!
//! Uses [std::net::UdpSocket] for sending and receiving UDP data.

use std::{
    io,
    net::{Ipv4Addr, ToSocketAddrs, UdpSocket},
    time::Duration,
};

use super::*;

/// A blocking Query client using the [`std`] networking primitives.
#[derive(Debug)]
pub struct QueryClient {
    socket: UdpSocket,
    session_id: u32,
}

impl QueryClient {
    /// Build a new QueryClient from the given IP address.
    ///
    /// If not port is specified in the IP address, the [default port](DEFAULT_PORT) is used.
    ///
    /// The default [timeout duration](DEFAULT_TIMEOUT) is used.
    pub fn new(ip: &str) -> io::Result<Self> {
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

        Self::new_with_port(ip, port)
    }

    /// Build a new QueryClient from the given IP address and port.
    ///
    /// If the IP address already contains a port, an error is returned.
    ///
    /// The default [timeout duration](DEFAULT_TIMEOUT) is used.
    pub fn new_with_port(ip: &str, port: u16) -> io::Result<Self> {
        if ip.contains(':') {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid IP address: must not contain a port.",
            ));
        }

        Self::new_with_socket_address(ip, port, (Ipv4Addr::UNSPECIFIED, 0), Some(DEFAULT_TIMEOUT))
    }

    /// Builds a new QueryClient from the given IP address, port, socket address and optional timeout.
    ///
    /// The IP adress must not contain a port.
    pub fn new_with_socket_address(
        ip: &str,
        port: u16,
        addr: impl ToSocketAddrs,
        timeout: Option<Duration>,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(timeout)?;
        socket.connect((ip, port))?;

        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time cannot be before UNIX_EPOCH")
            .as_nanos() as u32;

        Ok(Self { socket, session_id })
    }

    /// Send a UDP handshake packet to the client socket.
    ///
    /// Receive and parse the response into a Query token, valid up to 30 seconds.
    pub fn handshake(&self) -> io::Result<Token> {
        let handshake = packets::Handshake::new(self.session_id);
        self.socket.send(&handshake)?;

        let mut buf = [0; Token::RESPONSE_SIZE];
        let received = self.socket.recv(&mut buf)?;

        Ok(Token::from_payload(
            &buf.get(RESPONSE_HEADER_SIZE..received)
                .ok_or_else(not_enough_data)?,
        ))
    }

    /// Request and wait for a basic status packet on the client socket.
    ///
    /// If the token is no longer valid, no packet is received and an error is returned.
    pub fn basic_stat(&self, token: Token) -> std::io::Result<BasicStat> {
        let request = packets::BasicStat::new(self.session_id, token.0);
        self.socket.send(&request)?;

        let mut buf = vec![0; BasicStat::RESPONSE_SIZE];
        let received = self.socket.recv(&mut buf)?;

        BasicStat::from_payload(
            buf.get(RESPONSE_HEADER_SIZE..received)
                .ok_or_else(not_enough_data)?,
        )
    }

    /// Request and wait for a full status packet on the client socket.
    ///
    /// If the token is no longer valid, no packet is received and an error is returned.
    pub fn full_stat(&self, token: Token) -> std::io::Result<FullStat> {
        let request = packets::FullStat::new(self.session_id, token.0);
        self.socket.send(&request)?;

        let mut buf = vec![0; FullStat::RESPONSE_SIZE];
        let received = self.socket.recv(&mut buf)?;

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
pub fn query(ip: &str) -> io::Result<FullStat> {
    let client = QueryClient::new(ip)?;
    let token = client.handshake()?;

    client.full_stat(token)
}

#[cfg(test)]
mod tests {
    const TEST_IP: &str = "lotr.g.akliz.net:25565";

    #[test]
    fn test_handshake() {
        let client = super::QueryClient::new(TEST_IP).unwrap();
        client.handshake().unwrap();
    }

    #[test]
    fn test_basic_stat() {
        let client = super::QueryClient::new(TEST_IP).unwrap();
        let token = client.handshake().unwrap();

        let basic_stat = client.basic_stat(token).unwrap();
        assert_eq!(basic_stat.hostport, crate::query::DEFAULT_PORT);
    }

    #[test]
    fn test_full_stat() {
        let full_stat = super::query(TEST_IP).unwrap();

        assert_eq!(full_stat.hostport, crate::query::DEFAULT_PORT);
        assert_eq!(full_stat.numplayers as usize, full_stat.player_list.len());
        assert_eq!(full_stat.version, "1.7.10");
        assert_eq!(full_stat.game_id, "MINECRAFT");
    }
}
