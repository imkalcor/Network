use std::time::Duration;

pub mod binary;
pub mod mcpe;
pub mod message;
pub mod reliability;

/// Rust Raknet supports multiple Protocol Versions. The latest protocol version
/// is in the first index of this array.
pub const PROTOCOL_VERSION: u8 = 11;

/// Raknet Messages cannot exceed this MTU size, if they do, they are fragmented
/// into smaller encapsulated frames.
pub const MAX_MTU_SIZE: usize = 1500;

/// Regular Raknet uses 10 by default. MCPE uses 20. Configure this as appropriate.
pub const SYSTEM_ADDRESS_COUNT: usize = 20;

/// This is the number of times a single RakNet message can be split into encapsulated frames.
pub const MAX_SPLIT_PACKETS: u32 = 250;

/// This is the number of maximum encapsulated frames a single RakNet Datagram can carry.
pub const MAX_BATCHED_PACKETS: usize = 100;

/// This contains the size of the UDP Header.
/// IP Header Size (20 bytes)
/// UDP header size (8 bytes)
pub const UDP_HEADER_SIZE: usize = 20 + 8;

/// This contains the datagram header size.
/// Datagram Header (u8)
/// Sequence Number (u24)
pub const DATAGRAM_HEADER_SIZE: usize = 1 + 3;

/// This contains the size of the Raknet Frame Header.
/// Frame Header (u8)
/// Content Length (i16)
/// Message Index (u24)
/// Order Index (u24)
/// Order Channel (u8)
pub const FRAME_HEADER_SIZE: usize = 1 + 2 + 3 + 3 + 1;

/// This contains the additional size of the Raknet Frame Header only if
/// the packet is fragmented:
/// Fragment Count (i32)
/// Fragment ID (i16)
/// Fragment Index (i32)
pub const FRAME_ADDITIONAL_SIZE: usize = 4 + 2 + 4;

/// This flag is set for all datagrams irrespective of whether they contain
/// packet data or receipts.
pub const FLAG_DATAGRAM: u8 = 0x80;

/// This flag is set for every datagram with RakNet message data but it is not
/// actually used by the client.
pub const FLAG_NEEDS_B_AND_AS: u8 = 0x04;

/// This flag is set for every ACK receipt.
pub const FLAG_ACK: u8 = 0x40;

/// This flag is set for every NACK receipt.
pub const FLAG_NACK: u8 = 0x20;

/// RakNet Datagram containing split RakNet message must contain this flag in the fourth bit of the Datagram flag
/// as the first three bits are reliability.
pub const FLAG_FRAGMENTED: u8 = 0x10;

/// This is the maximum size that a Raknet Window can have at an instant.
pub const WINDOW_SIZE: u32 = 2048;

/// Internal Address is the default generic address sent to the network stream in various messages while
/// establishing a RakNet connection.
pub const INTERNAL_ADDRESS: &str = "255.255.255.255:19132";

/// RAKNET_TPS is the duration of how often in milliseconds should the RakNet logic run.
pub const RAKNET_TPS: u128 = 100;

/// This specifies the duration of how often we should be checking the outlived connections.
pub const RAKNET_CHECK_TIMEOUT: Duration = Duration::from_millis(100);

/// This value is the maximum amount of allowed RakNet messages in one second. If the number exceeds this value, the
/// stream gets disconnected.
pub const MAX_MSGS_PER_SEC: u8 = 100;

/// This value is the maximum number of malformed messages that the other side of the connection can send during its lifetime.
pub const MAX_INVALID_MSGS: u8 = 20;

/// This value is the time in milliseconds for which a spammy or a bad connection is blocked from the RakListener for.
pub const RAKNET_BLOCK_DUR: Duration = Duration::from_secs(10);

/// If a RakStream is not responding for more than this time in milliseconds then we assume it is a timeout.
pub const RAKNET_TIMEOUT: u128 = 100;

/// Unconnected Message Sequence is a sequence of bytes found in every Unconnected RakNet message.
pub const UNCONNECTED_MESSAGE_SEQUENCE: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];
