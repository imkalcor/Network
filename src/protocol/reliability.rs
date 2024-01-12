use std::io::{Error, ErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Reliability {
    Unreliable = 0,
    UnreliableSequenced,
    Reliable,
    ReliableOrdered,
    ReliableSequenced,
}

impl Reliability {
    pub fn reliable(&self) -> bool {
        match self {
            Self::Reliable => true,
            Self::ReliableOrdered => true,
            Self::ReliableSequenced => true,
            _ => false,
        }
    }

    pub fn sequenced_or_ordered(&self) -> bool {
        match self {
            Self::ReliableSequenced => true,
            Self::UnreliableSequenced => true,
            Self::ReliableOrdered => true,
            _ => false,
        }
    }

    pub fn sequenced(&self) -> bool {
        match self {
            Self::ReliableSequenced => true,
            Self::UnreliableSequenced => true,
            _ => false,
        }
    }
}

impl TryFrom<u8> for Reliability {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Unreliable),
            0x01 => Ok(Self::UnreliableSequenced),
            0x02 => Ok(Self::Reliable),
            0x03 => Ok(Self::ReliableOrdered),
            0x04 => Ok(Self::ReliableSequenced),
            _ => Err(Error::new(ErrorKind::Other, "Reliability value is invalid")),
        }
    }
}
