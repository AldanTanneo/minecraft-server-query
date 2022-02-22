//! Raw bytes packets used by the Query protocol
//!
//! # Base packet format
//! ## Client to Server Packet Format
//!
//! | Field name | Field type     | Notes                           |
//! |------------|----------------|---------------------------------|
//! | Magic      | [`u16`]        | Always `65277` (`0xFEFD`)       |
//! | Type       | [`PacketType`] | `9` for handshake, `0` for stat |
//! | Session ID | [`u32`]        |                                 |
//! | Payload    | Varies         | See per-packet documentation    |
//!
//! ## Server to Client Packet Format
//!
//! | Field name | Field type     | Notes                           |
//! |------------|----------------|---------------------------------|
//! | Type       | [`PacketType`] | `9` for handshake, `0` for stat |
//! | Session ID | [`u32`]        |                                 |
//! | Payload    | Varies         | See per-packet documentation    |

use bytes::BufMut;
use std::ops::Deref;

/// Magic number used in server bound packets
const MAGIC_NUMBER: u16 = 0xFEFD;
/// Session mask: the higher 4 bits of a byte are not taken into account
const SESSION_MASK: u32 = 0x0F0F0F0F;

/// Single byte constants representing the type of a packet
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PacketType {
    /// Type of a status packet
    Stat = 0,
    /// Type of a handshake packet
    Handshake = 9,
}

/// Write a server-bound packet to a byte array
fn write_packet<const N: usize, const P: usize>(
    packet_type: PacketType,
    session_id: u32,
    payload: [u32; P],
) -> [u8; N] {
    let mut res = [0; N];
    {
        let mut packet = &mut res[..];
        packet.put_u16(MAGIC_NUMBER);
        packet.put_u8(packet_type as u8);
        packet.put_u32(session_id & SESSION_MASK);
        for p in payload {
            packet.put_u32(p);
        }
    }

    res
}

/// Handshake request packet, 7 bytes long
///
/// The payload is empty.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Handshake([u8; 7]);

impl Handshake {
    /// Build a new handshake request packet from the given session id
    pub fn new(session_id: u32) -> Self {
        Self(write_packet(PacketType::Handshake, session_id, []))
    }
}

impl Deref for Handshake {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Basic status request packet, 11 bytes long
///
/// The payload contains the token obtained from a handshake.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BasicStat([u8; 11]);

impl BasicStat {
    /// Build a new basic status request packet from the given session ID and token
    pub fn new(session_id: u32, token: u32) -> Self {
        Self(write_packet(PacketType::Stat, session_id, [token]))
    }
}

impl Deref for BasicStat {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Full status request packet, 15 bytes long
///
/// The payload contains the token obtained from a handshake, and is padded to 8 bytes.
#[derive(Debug, Clone)]
pub struct FullStat([u8; 15]);

impl FullStat {
    /// Build a new full status request packet from the given session ID and token
    pub fn new(session_id: u32, token: u32) -> Self {
        Self(write_packet(PacketType::Stat, session_id, [token, 0]))
    }
}

impl Deref for FullStat {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
