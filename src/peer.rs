use anyhow::Context;
use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Peers(pub Vec<SocketAddrV4>);

impl Peers {
    pub fn new(bytes: &[u8]) -> Self {
        Peers(
            bytes
                .chunks(6)
                .map(|chunk| {
                    SocketAddrV4::new(
                        Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                        u16::from_be_bytes([chunk[4], chunk[5]]),
                    )
                })
                .collect(),
        )
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

impl From<&Handshake> for Bytes {
    fn from(value: &Handshake) -> Self {
        let mut bytes = BytesMut::with_capacity(68);
        bytes.put_u8(value.length);
        bytes.put_slice(&value.protocol);
        bytes.put_slice(&value.reserved);
        bytes.put_slice(&value.info_hash);
        bytes.put_slice(&value.peer_id);
        bytes.freeze()
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

#[derive(Debug)]
pub struct Message {
    pub id: u8,
    pub payload: Vec<u8>,
}

impl Message {
    pub const UNCHOKE: u8 = 1;
    pub const INTERESTED: u8 = 2;
    pub const BITFIELD: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const PIECE: u8 = 7;

    pub fn request_payload(index: u32, begin: u32, length: u32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&begin.to_be_bytes());
        payload.extend_from_slice(&length.to_be_bytes());
        payload
    }

    pub async fn read(stream: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<Self> {
        let mut length_buf = [0u8; 4];
        stream
            .read_exact(&mut length_buf)
            .await
            .context("Failed to read message length")?;
        let length = u32::from_be_bytes(length_buf) as usize;

        let mut id_buf = [0u8; 1];
        stream
            .read_exact(&mut id_buf)
            .await
            .context("Failed to read message id")?;

        let mut payload = vec![0u8; length - 1];
        stream
            .read_exact(&mut payload)
            .await
            .context("Failed to read message payload")?;

        Ok(Message {
            id: id_buf[0],
            payload,
        })
    }

    pub async fn write(&self, stream: &mut (impl AsyncWrite + Unpin)) -> anyhow::Result<()> {
        let length = (self.payload.len() + 1) as u32;
        stream
            .write_all(&length.to_be_bytes())
            .await
            .context("Failed to write message length")?;
        stream
            .write_all(&[self.id])
            .await
            .context("Failed to write message id")?;
        stream
            .write_all(&self.payload)
            .await
            .context("Failed to write message payload")?;
        Ok(())
    }
}

const BLOCK_SIZE: u32 = 16 * 1024;

pub struct PeerConnection {
    stream: TcpStream,
}

impl PeerConnection {
    pub async fn connect(
        addr: SocketAddrV4,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
    ) -> anyhow::Result<Self> {
        let mut stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to peer")?;

        let handshake = Bytes::from(&Handshake::new(info_hash, peer_id));
        stream
            .write_all(&handshake)
            .await
            .context("Failed to write handshake")?;
        let mut handshake_buf = [0u8; 68];
        stream
            .read_exact(&mut handshake_buf)
            .await
            .context("Failed to read handshake")?;
        Handshake::try_from(&handshake_buf).context("Failed to parse handshake")?;

        let bitfield = Message::read(&mut stream)
            .await
            .context("Failed to read bitfield message")?;
        anyhow::ensure!(
            bitfield.id == Message::BITFIELD,
            "Expected bitfield message, got id {}",
            bitfield.id
        );

        Message {
            id: Message::INTERESTED,
            payload: Vec::new(),
        }
        .write(&mut stream)
        .await
        .context("Failed to send interested message")?;

        let unchoke = Message::read(&mut stream)
            .await
            .context("Failed to read unchoke message")?;
        anyhow::ensure!(
            unchoke.id == Message::UNCHOKE,
            "Expected unchoke message, got id {}",
            unchoke.id
        );

        Ok(Self { stream })
    }

    pub async fn download_piece(&mut self, index: u32, length: usize) -> anyhow::Result<Vec<u8>> {
        let mut piece_data = Vec::with_capacity(length);
        let mut begin: u32 = 0;
        while (begin as usize) < length {
            let block_length = std::cmp::min(BLOCK_SIZE, length as u32 - begin);
            Message {
                id: Message::REQUEST,
                payload: Message::request_payload(index, begin, block_length),
            }
            .write(&mut self.stream)
            .await
            .context("Failed to send request message")?;

            let piece_msg = Message::read(&mut self.stream)
                .await
                .context("Failed to read piece message")?;
            anyhow::ensure!(
                piece_msg.id == Message::PIECE,
                "Expected piece message, got id {}",
                piece_msg.id
            );
            piece_data.extend_from_slice(&piece_msg.payload[8..]);

            begin += block_length;
        }
        Ok(piece_data)
    }
}
