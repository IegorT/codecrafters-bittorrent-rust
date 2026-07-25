use anyhow::Context;
use bittorrent_starter_rust::{decoder::parse as decode_bencoded_value, peer, torrent, tracker};
use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};
use std::net::SocketAddrV4;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Available if you need it!
// use serde_bencode

const PEER_ID: &[u8; 20] = b"00112233445566778899";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "snake_case")]
enum Command {
    // Usage: your_bittorrent.sh decode "<encoded_value>"
    Decode { value: String },
    Info { torrent: String },
    Peers { torrent: String },
    Handshake { torrent: String, peer: String },
    DownloadPiece {
        #[arg(short = 'o')]
        output: String,
        torrent: String,
        piece_index: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Decode { value } => {
            let decoded_value =
                decode_bencoded_value(value.as_bytes()).context("Failed to decode value")?;
            println!("{}", decoded_value);
        }
        Command::Info { torrent } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            println!("Tracker URL: {}", torrent_meta.announce);
            println!("Length: {}", torrent_meta.info.length);
            println!("Info Hash: {}", hex::encode(torrent_meta.info.info_hash()));
            println!("Piece Length: {}", torrent_meta.info.piece_length);
            println!("Piece Hashes:");
            for hash in torrent_meta.info.pieces.chunks(20) {
                println!("{}", hex::encode(hash));
            }
        }
        Command::Peers { torrent } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let tracker_req = tracker::TrackerRequest::new(PEER_ID, &torrent_meta);
            let response = reqwest::get(tracker_req.url())
                .await
                .context("Failed to get tracker response")?;
            let body = response
                .bytes()
                .await
                .context("Failed to get response bytes")?;
            let tracker_response = tracker::TrackerResponse::try_from(&body)
                .context("Failed to parse tracker response")?;
            for peer in tracker_response.get_peers().0 {
                println!("{}", peer);
            }
        }
        Command::Handshake { torrent, peer } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let tracker_req = tracker::TrackerRequest::new(PEER_ID, &torrent_meta);
            let response = reqwest::get(tracker_req.url())
                .await
                .context("Failed to get tracker response")?;
            let body = response
                .bytes()
                .await
                .context("Failed to get response bytes")?;
            let tracker_response = tracker::TrackerResponse::try_from(&body)
                .context("Failed to parse tracker response")?;
            let peer_addr: SocketAddrV4 = peer.parse().context("Failed to parse peer address")?;
            let _peer_addr = tracker_response
                .get_peers()
                .0
                .into_iter()
                .find(|p| *p == peer_addr)
                .context("Peer not found in tracker response")?;
            let mut stream = tokio::net::TcpStream::connect(peer_addr)
                .await
                .context("Failed to connect to peer")?;
            let handshake = bytes::Bytes::from(&peer::Handshake::new(
                torrent_meta.info.info_hash(),
                *PEER_ID,
            ));
            let mut buf = [0u8; 68];
            stream
                .write_all(&handshake)
                .await
                .context("Failed to write handshake")?;
            stream
                .read_exact(&mut buf)
                .await
                .context("Failed to read handshake")?;
            let handshake = peer::Handshake::try_from(&buf).context("Failed to parse handshake")?;
            println!("Peer ID: {}", hex::encode(handshake.peer_id));
        }
        Command::DownloadPiece {
            output,
            torrent,
            piece_index,
        } => {
            const BLOCK_SIZE: u32 = 16 * 1024;

            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let tracker_req = tracker::TrackerRequest::new(PEER_ID, &torrent_meta);
            let response = reqwest::get(tracker_req.url())
                .await
                .context("Failed to get tracker response")?;
            let body = response
                .bytes()
                .await
                .context("Failed to get response bytes")?;
            let tracker_response = tracker::TrackerResponse::try_from(&body)
                .context("Failed to parse tracker response")?;
            let peer_addr = tracker_response
                .get_peers()
                .0
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut stream = tokio::net::TcpStream::connect(peer_addr)
                .await
                .context("Failed to connect to peer")?;
            let handshake = bytes::Bytes::from(&peer::Handshake::new(
                torrent_meta.info.info_hash(),
                *PEER_ID,
            ));
            let mut handshake_buf = [0u8; 68];
            stream
                .write_all(&handshake)
                .await
                .context("Failed to write handshake")?;
            stream
                .read_exact(&mut handshake_buf)
                .await
                .context("Failed to read handshake")?;

            let bitfield = peer::Message::read(&mut stream)
                .await
                .context("Failed to read bitfield message")?;
            anyhow::ensure!(
                bitfield.id == peer::Message::BITFIELD,
                "Expected bitfield message, got id {}",
                bitfield.id
            );

            peer::Message {
                id: peer::Message::INTERESTED,
                payload: Vec::new(),
            }
            .write(&mut stream)
            .await
            .context("Failed to send interested message")?;

            let unchoke = peer::Message::read(&mut stream)
                .await
                .context("Failed to read unchoke message")?;
            anyhow::ensure!(
                unchoke.id == peer::Message::UNCHOKE,
                "Expected unchoke message, got id {}",
                unchoke.id
            );

            let piece_count = torrent_meta.info.pieces.len() / 20;
            anyhow::ensure!(piece_index < piece_count, "Piece index out of range");
            let piece_length = if piece_index == piece_count - 1 {
                let remainder = torrent_meta.info.length % torrent_meta.info.piece_length;
                if remainder == 0 {
                    torrent_meta.info.piece_length
                } else {
                    remainder
                }
            } else {
                torrent_meta.info.piece_length
            };

            let mut piece_data = Vec::with_capacity(piece_length);
            let mut begin: u32 = 0;
            while (begin as usize) < piece_length {
                let block_length = std::cmp::min(BLOCK_SIZE, piece_length as u32 - begin);
                peer::Message {
                    id: peer::Message::REQUEST,
                    payload: peer::Message::request_payload(
                        piece_index as u32,
                        begin,
                        block_length,
                    ),
                }
                .write(&mut stream)
                .await
                .context("Failed to send request message")?;

                let piece_msg = peer::Message::read(&mut stream)
                    .await
                    .context("Failed to read piece message")?;
                anyhow::ensure!(
                    piece_msg.id == peer::Message::PIECE,
                    "Expected piece message, got id {}",
                    piece_msg.id
                );
                piece_data.extend_from_slice(&piece_msg.payload[8..]);

                begin += block_length;
            }

            let expected_hash = &torrent_meta.info.pieces[piece_index * 20..piece_index * 20 + 20];
            let mut hasher = Sha1::new();
            hasher.update(&piece_data);
            let actual_hash: [u8; 20] = hasher.finalize().into();
            anyhow::ensure!(actual_hash == expected_hash[..], "Piece hash mismatch");

            std::fs::write(&output, &piece_data).context("Failed to write piece to disk")?;
            println!("Piece {} downloaded to {}.", piece_index, output);
        }
    }
    Ok(())
}
