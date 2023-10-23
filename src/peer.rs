use bytes::{BytesMut, BufMut, Bytes};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Peers(pub Vec<SocketAddrV4>);

impl Peers {
    pub fn new(bytes: &ByteBuf) -> Self {
        return Peers(
            bytes
                .chunks(6)
                .map(|chunk| {
                    SocketAddrV4::new(
                        Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                        u16::from_be_bytes([chunk[4], chunk[5]]),
                    )
                })
                .collect(),
        );
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[repr(C)]
pub struct Handshake {
    pub length: u8,
    pub protocol: [u8; 19],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }
}

impl TryFrom<&Handshake> for Bytes {
    type Error = anyhow::Error;

    fn try_from(value: &Handshake) -> Result<Self, Self::Error> {
        let mut bytes = BytesMut::with_capacity(68);
        bytes.put_u8(value.length);
        bytes.put_slice(&value.protocol);
        bytes.put_slice(&value.reserved);
        bytes.put_slice(&value.info_hash);
        bytes.put_slice(&value.peer_id);
        Ok(bytes.freeze())
    }
    
}

impl TryFrom<&[u8; 68]> for Handshake {
    type Error = anyhow::Error;

    fn try_from(value: &[u8; 68]) -> Result<Self, Self::Error> {
        Ok(Handshake {
            length: value[0],
            protocol: value[1..20].try_into()?,
            reserved: value[20..28].try_into()?,
            info_hash: value[28..48].try_into()?,
            peer_id: value[48..68].try_into()?,
        })
    }
}
