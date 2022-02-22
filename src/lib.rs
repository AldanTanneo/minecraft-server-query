#![cfg_attr(doc, feature(doc_cfg))]

//! Implementation of the Minecraft server [Query UDP protocol](https://wiki.vg/Query)
//!
//! The [`blocking`] and [`async`](self::tokio) [versions](self::async_std) have
//! the same API, adding a few `async` and `.await` here and there :
//!
//! ```rust
//! # use minecraft_server_query::*;
//! # use std::net::Ipv4Addr;
//! # use std::time::Duration;
//! # let ip_to_query = "lotr.g.akliz.net";
//! use blocking::QueryClient;
//!
//! let client = QueryClient::new(ip_to_query)?;
//! let client2 = QueryClient::new_with_port(ip_to_query, 25565)?;
//! let client3 = QueryClient::new_with_socket_address(
//!     ip_to_query,
//!     25565,
//!     (Ipv4Addr::UNSPECIFIED, 0),
//!     Some(Duration::from_secs(3)),
//! )?;
//!
//! let token = client.handshake()?;
//! let basic_stat = client.basic_stat(token)?;
//! let full_stat = client.full_stat(token)?;
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! The convenience function [`query`](blocking::query) is also available in each module,
//! and handles the handshake for you:
//!
//! ```rust
//! # use minecraft_server_query::*;
//! # let ip_to_query = "lotr.g.akliz.net";
//! let full_stat = blocking::query(ip_to_query)?;
//! # Ok::<(), std::io::Error>(())
//! ```

#[cfg(feature = "async-std")]
#[cfg_attr(doc, doc(cfg(feature = "async-std")))]
pub mod async_std;
pub mod blocking;
pub mod packets;
#[cfg(feature = "tokio")]
#[cfg_attr(doc, doc(cfg(feature = "tokio")))]
pub mod tokio;

use std::{
    io,
    ops::{Add, Mul},
    time::Duration,
};

use bytes::Buf;

#[cfg(feature = "tokio")]
#[cfg_attr(doc, doc(cfg(feature = "tokio")))]
pub use self::tokio::*;
#[cfg(all(feature = "async-std", not(feature = "tokio")))]
#[cfg_attr(doc, doc(cfg(feature = "async-std")))]
pub use async_std::*;
#[cfg(all(not(feature = "async-std"), not(feature = "tokio")))]
#[cfg_attr(doc, doc(cfg(all(not(feature = "async-std"), not(feature = "tokio")))))]
pub use blocking::*;

/// Default port for a Minecraft server.
pub const DEFAULT_PORT: u16 = 25565;
/// Default timeout for the UDP sockets in [`QueryClient`](crate::blocking::QueryClient)
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

/// Header size, in bytes
const RESPONSE_HEADER_SIZE: usize = std::mem::size_of::<u8>() + std::mem::size_of::<u32>();

/// Returns an IO error with error kind set to `Other`
#[inline]
fn custom_io_error(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}

/// Custom IO error for missing data in UDP payload
#[inline]
fn not_enough_data() -> io::Error {
    custom_io_error("Not enough data in UDP payload.")
}

/// Converts a slice of raw bytes to a string, interpreting each byte as a
/// unicode code point
#[inline]
fn latin1_to_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Parse a decimal number from a slice of bytes. Every byte must be a valid decimal digit.
fn decimal_from_bytes<T>(bytes: &[u8]) -> io::Result<T>
where
    T: Add<T, Output = T> + Mul<T, Output = T> + From<u8>,
{
    bytes
        .iter()
        .try_fold(T::from(0), |acc, &b| {
            if b'0' <= b && b <= b'9' {
                Some(acc * T::from(10) + T::from(b - b'0'))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            custom_io_error("Failed to parse decimal unsigned integer on reading non-digit byte.")
        })
}

/// Split a slice of bytes at the first occurence of a subslice.
///
/// The pattern is not contained in the returned slices.
fn split_at_subslice<'a, T: PartialEq>(
    slice: &'a [T],
    pattern: &[T],
) -> Option<(&'a [T], &'a [T])> {
    if pattern.len() <= slice.len() {
        for (i, subslice) in slice.windows(pattern.len()).enumerate() {
            if subslice == pattern {
                let (a, b) = slice.split_at(i);
                return b.get(pattern.len()..).map(|b| (a, b));
            }
        }
    }
    None
}

/// Return an iterator on pairs of the iterator in argument. If the iterator
/// has an odd number of elements, the last element will be discarded.
fn pairs<T, I: Iterator<Item = T>>(iter: I) -> impl Iterator<Item = (T, T)> {
    struct Pairs<T, It: Iterator<Item = T>>(It);

    impl<T, It: Iterator<Item = T>> Iterator for Pairs<T, It> {
        type Item = (T, T);
        fn next(&mut self) -> Option<(T, T)> {
            self.0
                .next()
                .map(|it1| self.0.next().map(|it2| (it1, it2)))
                .flatten()
        }
    }

    Pairs(iter)
}

/// A Query token, returned by a UDP handshake
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Token(pub u32);

impl Token {
    /// Handshake response max size, in bytes
    const RESPONSE_SIZE: usize = 16;

    /// Parse a token from a UDP payload, discarding the terminating null byte.
    ///
    /// ```rust
    /// # use minecraft_server_query::Token;
    /// assert_eq!(Token::from_payload(&b"123456\0"[..]), Token(123456));
    /// ```
    pub fn from_payload(payload: &[u8]) -> Self {
        Self(
            payload
                .iter()
                .map_while(|&b| {
                    if b'0' <= b && b <= b'9' {
                        Some((b - b'0') as u32)
                    } else {
                        None
                    }
                })
                .fold(0, |acc, digit| acc * 10 + digit),
        )
    }
}

/// Basic status information on a minecraft server
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicStat {
    /// Server MoTD as displayed in the in-game server browser
    pub motd: String,
    /// The server's gametype, hardcoded to `"SMP"`
    pub gametype: String,
    /// Name of the default world
    pub map: String,
    /// How many players are currently online
    pub numplayers: u32,
    /// Maximum number of players this server supports
    pub maxplayers: u32,
    /// Port the server is listening on
    pub hostport: u16,
    /// IP that the server may receive connections on
    pub hostip: String,
}

impl BasicStat {
    /// Basic stat response max size, in bytes
    const RESPONSE_SIZE: usize = 512;

    /// Parse a basic stat struct from a UDP payload. Fails if fields are
    /// missing, returning an IO error for missing data
    ///
    /// ```rust
    /// # use minecraft_server_query::BasicStat;
    /// let payload = b"A Minecraft Server\0SMP\0world\02\020\0\xDD\x63127.0.0.1\0";
    ///
    /// assert_eq!(
    ///     BasicStat::from_payload(&payload[..])?,
    ///     BasicStat {
    ///         motd: "A Minecraft Server".to_string(),
    ///         gametype: "SMP".to_string(),
    ///         map: "world".to_string(),
    ///         numplayers: 2,
    ///         maxplayers: 20,
    ///         hostport: 25565,
    ///         hostip: "127.0.0.1".to_string(),
    ///     }
    /// );
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn from_payload(payload: &[u8]) -> io::Result<Self> {
        let mut values = payload.split(|&b| b == b'\0');

        let motd = latin1_to_string(values.next().ok_or_else(not_enough_data)?);
        let gametype = latin1_to_string(values.next().ok_or_else(not_enough_data)?);
        let map = latin1_to_string(values.next().ok_or_else(not_enough_data)?);
        let numplayers = decimal_from_bytes(values.next().ok_or_else(not_enough_data)?)?;
        let maxplayers = decimal_from_bytes(values.next().ok_or_else(not_enough_data)?)?;

        let ip = values.next().ok_or_else(not_enough_data)?;

        let hostport = {
            let mut buf = ip.get(..2).ok_or_else(not_enough_data)?;
            buf.get_u16_le()
        };
        let hostip = latin1_to_string(ip.get(2..).ok_or_else(not_enough_data)?);

        Ok(Self {
            motd,
            gametype,
            map,
            numplayers,
            maxplayers,
            hostport,
            hostip,
        })
    }
}

/// Full status information for a minecraft server
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullStat {
    /// Server MoTD as displayed in the in-game server browser
    pub hostname: String,
    /// Game type, hardcoded to `"SMP"`
    pub gametype: String,
    /// Game ID, hardcoded to `"MINECRAFT"`
    pub game_id: String,
    /// Game version (`"1.7.10"`, `"1.16.2"`...)
    pub version: String,
    /// Server plugins. Format varies with server framework
    pub plugins: String,
    /// Name of the default world
    pub map: String,
    /// How many players are currently online
    pub numplayers: u32,
    /// Maximum number of players this server supports
    pub maxplayers: u32,
    /// Port the server is listening on
    pub hostport: u16,
    /// IP that the server may receive connections on
    pub hostip: String,
    /// Names of the players currently online
    pub player_list: Vec<String>,
}

impl FullStat {
    /// Full stat response max size, in bytes
    const RESPONSE_SIZE: usize = 1472;
    /// Padding at the start of the payload
    const PADDING_START_SIZE: usize = 11;
    /// Padding in the middle of the payload, between the KV and players sections
    const SECTIONS_SEPARATOR: &'static [u8; 12] = b"\0\0\x01player_\0\0";

    /// Parse the key-value section of the payload. Fails with an IO error on missing keys.
    fn parse_kv_section(bytes: &[u8]) -> io::Result<Self> {
        let mut values = pairs(bytes.split(|&b| b == b'\0'))
            .map(|(key, value)| (latin1_to_string(key), latin1_to_string(value)))
            .collect::<std::collections::HashMap<_, _>>();

        let hostname = values.remove("hostname").ok_or_else(not_enough_data)?;
        let gametype = values.remove("gametype").ok_or_else(not_enough_data)?;
        let game_id = values.remove("game_id").ok_or_else(not_enough_data)?;
        let version = values.remove("version").ok_or_else(not_enough_data)?;
        let plugins = values.remove("plugins").ok_or_else(not_enough_data)?;
        let map = values.remove("map").ok_or_else(not_enough_data)?;
        let numplayers = values
            .remove("numplayers")
            .ok_or_else(not_enough_data)?
            .parse::<u32>()
            .map_err(|_| {
                custom_io_error(
                    "Failed to parse decimal unsigned integer on reading non-digit byte.",
                )
            })?;
        let maxplayers = values
            .remove("maxplayers")
            .ok_or_else(not_enough_data)?
            .parse::<u32>()
            .map_err(|_| {
                custom_io_error(
                    "Failed to parse decimal unsigned integer on reading non-digit byte.",
                )
            })?;
        let hostport = values
            .remove("hostport")
            .ok_or_else(not_enough_data)?
            .parse::<u16>()
            .map_err(|_| {
                custom_io_error(
                    "Failed to parse decimal unsigned integer on reading non-digit byte.",
                )
            })?;
        let hostip = values.remove("hostip").ok_or_else(not_enough_data)?;

        Ok(Self {
            hostname,
            gametype,
            game_id,
            version,
            plugins,
            map,
            numplayers,
            maxplayers,
            hostport,
            hostip,
            player_list: Vec::new(),
        })
    }

    /// Parse a full stat struct from a UDP payload. Fails if fields are
    /// missing, returning an IO error for missing data
    ///
    /// ```rust
    /// # use minecraft_server_query::FullStat;
    /// let payload = b"...........\
    ///     hostname\0A Minecraft Server\0\
    ///     gametype\0SMP\0game_id\0MINECRAFT\0\
    ///     version\01.7.10\0plugins\0\0map\0world\0\
    ///     numplayers\02\0maxplayers\020\0\
    ///     hostport\025565\0hostip\0127.0.0.1\
    ///     \0\0\x01player_\0\0\
    ///     AldanTanneo\0Dinnerbone\0\0";
    ///
    /// assert_eq!(
    ///     FullStat::from_payload(&payload[..])?,
    ///     FullStat {
    ///         hostname: "A Minecraft Server".to_string(),
    ///         gametype: "SMP".to_string(),
    ///         game_id: "MINECRAFT".to_string(),
    ///         version: "1.7.10".to_string(),
    ///         plugins: "".to_string(),
    ///         map: "world".to_string(),
    ///         numplayers: 2,
    ///         maxplayers: 20,
    ///         hostport: 25565,
    ///         hostip: "127.0.0.1".to_string(),
    ///         player_list: vec![
    ///             "AldanTanneo".to_string(),
    ///             "Dinnerbone".to_string(),
    ///         ],
    ///     }
    /// );
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn from_payload(payload: &[u8]) -> io::Result<Self> {
        let (kv_section, players_section) = split_at_subslice(
            payload
                .get(Self::PADDING_START_SIZE..)
                .ok_or_else(not_enough_data)?,
            Self::SECTIONS_SEPARATOR.as_slice(),
        )
        .ok_or_else(|| custom_io_error("Failed to parse full stat payload due to missing data."))?;

        let mut res = Self::parse_kv_section(kv_section)?;

        res.player_list
            .extend(players_section.split(|&b| b == b'\0').filter_map(|name| {
                if !name.is_empty() {
                    Some(latin1_to_string(name))
                } else {
                    None
                }
            }));

        Ok(res)
    }
}
