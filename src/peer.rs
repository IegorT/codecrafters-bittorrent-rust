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
    // 20th bit from the right in the reserved bytes, see BEP 10.
    const EXTENSION_SUPPORT_BYTE: usize = 5;
    const EXTENSION_SUPPORT_MASK: u8 = 0x10;

    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash,
            peer_id,
        }
    }

    pub fn with_extension_support(mut self) -> Self {
        self.reserved[Self::EXTENSION_SUPPORT_BYTE] |= Self::EXTENSION_SUPPORT_MASK;
        self
    }

    pub fn supports_extensions(&self) -> bool {
        self.reserved[Self::EXTENSION_SUPPORT_BYTE] & Self::EXTENSION_SUPPORT_MASK != 0
    }

    pub async fn perform(
        stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        support_extensions: bool,
    ) -> anyhow::Result<Self> {
        let mut handshake = Handshake::new(info_hash, peer_id);
        if support_extensions {
            handshake = handshake.with_extension_support();
        }

        let bytes = Bytes::from(&handshake);
        stream
            .write_all(&bytes)
            .await
            .context("Failed to write handshake")?;

        let mut buf = [0u8; 68];
        stream
            .read_exact(&mut buf)
            .await
            .context("Failed to read handshake")?;

        Handshake::try_from(&buf).context("Failed to parse handshake")
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

#[derive(Serialize, Deserialize, Debug)]
struct ExtensionHandshakePayload {
    m: std::collections::HashMap<String, i64>,
}

impl Message {
    pub const UNCHOKE: u8 = 1;
    pub const INTERESTED: u8 = 2;
    pub const BITFIELD: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const PIECE: u8 = 7;
    pub const EXTENSION: u8 = 20;

    pub fn request_payload(index: u32, begin: u32, length: u32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&begin.to_be_bytes());
        payload.extend_from_slice(&length.to_be_bytes());
        payload
    }

    pub fn extension_handshake(ut_metadata_id: u8) -> anyhow::Result<Self> {
        let dict = ExtensionHandshakePayload {
            m: std::collections::HashMap::from([("ut_metadata".to_string(), ut_metadata_id as i64)]),
        };

        let mut payload = vec![0u8]; // extended message id 0 = handshake
        payload.extend(serde_bencode::to_bytes(&dict)?);

        Ok(Message {
            id: Message::EXTENSION,
            payload,
        })
    }

    pub fn parse_extension_handshake(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        anyhow::ensure!(
            self.id == Self::EXTENSION,
            "Expected extension message, got id {}",
            self.id
        );
        anyhow::ensure!(
            self.payload.first() == Some(&0),
            "Expected extension handshake, got extended message id {:?}",
            self.payload.first()
        );

        let dict: ExtensionHandshakePayload = serde_bencode::from_bytes(&self.payload[1..])
            .context("Failed to parse extension handshake payload")?;
        Ok(dict.m)
    }

    pub fn metadata_request(peer_metadata_id: u8, piece: u32) -> anyhow::Result<Self> {
        #[derive(Serialize)]
        struct MetadataRequestPayload {
            msg_type: i64,
            piece: i64,
        }

        let dict = MetadataRequestPayload {
            msg_type: 0,
            piece: piece as i64,
        };

        let mut payload = vec![peer_metadata_id];
        payload.extend(serde_bencode::to_bytes(&dict)?);

        Ok(Message {
            id: Message::EXTENSION,
            payload,
        })
    }

    pub fn parse_metadata_data(&self) -> anyhow::Result<Vec<u8>> {
        #[derive(Deserialize)]
        struct MetadataDataHeader {
            msg_type: i64,
            total_size: usize,
        }

        anyhow::ensure!(
            self.id == Self::EXTENSION,
            "Expected extension message, got id {}",
            self.id
        );

        // The bencoded header is followed directly by the raw metadata bytes;
        // serde_bencode stops at the end of the dict and ignores the rest, so
        // we just take the last `total_size` bytes as the metadata piece.
        let body = &self.payload[1..];
        let header: MetadataDataHeader =
            serde_bencode::from_bytes(body).context("Failed to parse metadata message header")?;
        anyhow::ensure!(
            header.msg_type == 1,
            "Expected metadata data message (msg_type 1), got msg_type {}",
            header.msg_type
        );
        anyhow::ensure!(
            body.len() >= header.total_size,
            "Metadata message shorter than its declared total_size"
        );

        Ok(body[body.len() - header.total_size..].to_vec())
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

pub struct ExtendedHandshake {
    pub handshake: Handshake,
    pub peer_metadata_id: Option<u8>,
}

pub async fn perform_extended_handshake(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    our_metadata_id: u8,
) -> anyhow::Result<ExtendedHandshake> {
    let handshake = Handshake::perform(stream, info_hash, peer_id, true).await?;

    let bitfield = Message::read(stream)
        .await
        .context("Failed to read bitfield message")?;
    anyhow::ensure!(
        bitfield.id == Message::BITFIELD,
        "Expected bitfield message, got id {}",
        bitfield.id
    );

    let peer_metadata_id = if handshake.supports_extensions() {
        Message::extension_handshake(our_metadata_id)?
            .write(stream)
            .await
            .context("Failed to send extension handshake")?;

        let peer_extensions = Message::read(stream)
            .await
            .context("Failed to read extension handshake message")?
            .parse_extension_handshake()?;
        peer_extensions.get("ut_metadata").map(|id| *id as u8)
    } else {
        None
    };

    Ok(ExtendedHandshake {
        handshake,
        peer_metadata_id,
    })
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

        Handshake::perform(&mut stream, info_hash, peer_id, false).await?;

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
        const PIPELINE_DEPTH: u32 = 5;

        let mut piece_data = vec![0u8; length];
        let mut requested: u32 = 0;
        let mut received: usize = 0;
        let mut in_flight: u32 = 0;

        while received < length {
            while in_flight < PIPELINE_DEPTH && (requested as usize) < length {
                let block_length = std::cmp::min(BLOCK_SIZE, length as u32 - requested);
                Message {
                    id: Message::REQUEST,
                    payload: Message::request_payload(index, requested, block_length),
                }
                .write(&mut self.stream)
                .await
                .context("Failed to send request message")?;
                requested += block_length;
                in_flight += 1;
            }

            let piece_msg = Message::read(&mut self.stream)
                .await
                .context("Failed to read piece message")?;
            anyhow::ensure!(
                piece_msg.id == Message::PIECE,
                "Expected piece message, got id {}",
                piece_msg.id
            );
            let begin =
                u32::from_be_bytes(piece_msg.payload[4..8].try_into().unwrap()) as usize;
            let block = &piece_msg.payload[8..];
            piece_data[begin..begin + block.len()].copy_from_slice(block);
            received += block.len();
            in_flight -= 1;
        }

        Ok(piece_data)
    }
}
