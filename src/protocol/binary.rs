use std::{
    io::{Cursor, Error, ErrorKind, Read, Result, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use binary::debug_impl;
use binary::{
    datatypes::{I16, I32, U16},
    Binary,
};
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use bytes::Buf;
use std::str::FromStr;

use super::{INTERNAL_ADDRESS, UNCONNECTED_MESSAGE_SEQUENCE};

pub struct UDPAddress(SocketAddr);
debug_impl!(UDPAddress);

impl<'a> Binary<'a> for UDPAddress {
    fn serialize(&self, buf: &mut impl Write) {
        match self.0.ip() {
            IpAddr::V4(ip) => {
                buf.write_u8(4).unwrap();
                buf.write_all(&ip.octets()).unwrap();
                U16::<BE>::new(self.0.port()).serialize(buf);
            }
            IpAddr::V6(ip) => {
                buf.write_u8(6).unwrap();
                I16::<LE>::new(23).serialize(buf);
                U16::<BE>::new(self.0.port()).serialize(buf);
                I32::<BE>::new(0).serialize(buf);
                buf.write_all(&ip.octets()).unwrap();
                I32::<BE>::new(0).serialize(buf);
            }
        }
    }

    fn deserialize(buf: &mut Cursor<&'a [u8]>) -> Result<Self> {
        match buf.read_u8()? {
            4 => {
                let mut bytes = [0u8; 4];
                buf.read_exact(&mut bytes)?;

                let ip = IpAddr::V4(Ipv4Addr::from(bytes));
                let port = U16::<BE>::deserialize(buf)?.0;

                Ok(UDPAddress(SocketAddr::new(ip, port)))
            }
            6 => {
                let mut bytes = [0u8; 16];
                buf.advance(2);

                let port = U16::<BE>::deserialize(buf)?.0;
                buf.advance(4);

                buf.read_exact(&mut bytes).unwrap();
                buf.advance(4);

                let ip = IpAddr::V6(Ipv6Addr::from(bytes));

                Ok(UDPAddress(SocketAddr::new(ip, port)))
            }
            _ => Err(Error::new(
                ErrorKind::Other,
                "IP Address can only be of either IPv4 or IPv6 type.",
            )),
        }
    }
}

#[derive(Debug)]
pub struct SystemAddresses;

impl<'a> Binary<'a> for SystemAddresses {
    fn serialize(&self, buf: &mut impl Write) {
        for _ in 0..20 {
            UDPAddress(SocketAddr::from_str(INTERNAL_ADDRESS).unwrap()).serialize(buf);
        }
    }

    fn deserialize(buf: &mut Cursor<&'a [u8]>) -> Result<Self> {
        for _ in 0..20 {
            if buf.remaining() == 16 {
                return Ok(SystemAddresses);
            }

            UDPAddress::deserialize(buf)?;
        }

        Ok(SystemAddresses)
    }
}

#[derive(Debug)]
pub struct Magic;

impl<'a> Binary<'a> for Magic {
    fn serialize(&self, buf: &mut impl Write) {
        buf.write_all(&UNCONNECTED_MESSAGE_SEQUENCE).unwrap();
    }

    fn deserialize(buf: &mut Cursor<&'a [u8]>) -> Result<Self> {
        let start = buf.position() as usize;
        let end = start + 16;

        buf.advance(16);

        if &buf.get_ref()[start..end] != UNCONNECTED_MESSAGE_SEQUENCE {
            return Err(Error::new(
                ErrorKind::Other,
                "Unconnected Message Sequence mismatch",
            ));
        }

        Ok(Magic)
    }
}
