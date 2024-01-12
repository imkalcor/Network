use binary::{
    datatypes::{Bool, I64, U16, U8},
    prefixed::UnsizedBytes,
};
use byteorder::BE;

use super::binary::{Magic, SystemAddresses, UDPAddress};

macro_rules! build_message {
    (
        $(
            $id:expr; $name:ident {
                $(
                    $field:ident: $field_type:ty
                ),* $(,)?
            };
        )+) => {
            use binary::Binary;
            use std::io::{Error, ErrorKind, Cursor, Result, Write};
            use byteorder::{ReadBytesExt, WriteBytesExt};

            #[derive(Debug)]
            pub enum Message<'a> {
                $(
                    $name {
                        $(
                            $field: $field_type
                        ),*
                    }
                ),*
            }

            impl<'a> Message<'a> {
                /// Returns the message's unique ID
                pub fn id(&self) -> u8 {
                    match self {
                        $(
                            Message::$name {..} => $id
                        ,)*
                    }
                }
            }

            impl<'a> Binary<'a> for Message<'a> {
                fn serialize(&self, buf: &mut impl Write) {
                    buf.write_u8(self.id()).unwrap();

                    match self {
                        $(Message::$name { $($field),* } => {
                            $(
                                $field.serialize(buf);
                            )*
                        })*
                    }
                }

                fn deserialize(buf: &mut Cursor<&'a [u8]>) -> Result<Self> {
                    let id = buf.read_u8()?;

                    match id {
                        $(
                            $id => Ok(Message::$name {
                               $(
                                    $field: Binary::deserialize(buf)?
                               ),*
                            })
                        ),*,
                        _ => Err(Error::new(ErrorKind::Other, "Unknown Message ID"))
                    }
                }
            }
        };
}

build_message! {
    0x01; UnconnectedPing {
        send_timestamp: I64<BE>,
        magic: Magic,
        client_guid: I64<BE>
    };
    0x02; UnconnectedPingOpenConnections {
        send_timestamp: I64<BE>,
        magic: Magic,
        client_guid: I64<BE>
    };
    0x1c; UnconnectedPong {
        send_timestamp: I64<BE>,
        server_guid: I64<BE>,
        magic: Magic,
        data: UnsizedBytes<'a>
    };
    0x05; OpenConnectionRequest1 {
        magic: Magic,
        protocol: U8,
        emptybuf: UnsizedBytes<'a>
    };
    0x06; OpenConnectionReply1 {
        magic: Magic,
        server_guid: I64<BE>,
        secure: Bool,
        server_mtu: U16<BE>
    };
    0x07; OpenConnectionRequest2 {
        magic: Magic,
        server_address: UDPAddress,
        client_mtu: U16<BE>,
        client_guid: I64<BE>
    };
    0x08; OpenConnectionReply2 {
        magic: Magic,
        server_guid: I64<BE>,
        client_address: UDPAddress,
        mtu_size: U16<BE>,
        secure: Bool
    };
    0x19; IncompatibleProtocolVersion {
        server_protocol: U8,
        magic: Magic,
        server_guid: I64<BE>
    };
    0x00; ConnectedPing {
        client_timestamp: I64<BE>
    };
    0x03; ConnectedPong {
        client_timestamp: I64<BE>,
        server_timestamp: I64<BE>
    };
    0x09; ConnectionRequest {
        client_guid: I64<BE>,
        request_timestamp: I64<BE>,
        secure: Bool
    };
    0x10; ConnectionRequestAccepted {
        client_address: UDPAddress,
        system_addresses: SystemAddresses,
        request_timestamp: I64<BE>,
        accept_timestamp: I64<BE>
    };
    0x13; NewIncomingConnection {
        server_address: UDPAddress,
        system_addresses: SystemAddresses,
        request_timestamp: I64<BE>,
        accept_timestamp: I64<BE>
    };
    0x04; DetectLostConnections {

    };
    0x15; DisconnectNotification {

    };
    0xfe; GamePacket {
        data: UnsizedBytes<'a>
    };
}
