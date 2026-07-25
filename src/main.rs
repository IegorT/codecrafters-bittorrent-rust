use anyhow::Context;
use bittorrent_starter_rust::{decoder::parse as decode_bencoded_value, peer, torrent, tracker};
use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};
use std::net::SocketAddrV4;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// Available if you need it!
// use serde_bencode

const PEER_ID: &[u8; 20] = b"00112233445566778899";

async fn fetch_peers(torrent_meta: &torrent::Torrent) -> anyhow::Result<Vec<SocketAddrV4>> {
    let tracker_req = tracker::TrackerRequest::new(PEER_ID, torrent_meta);
    let response = reqwest::get(tracker_req.url())
        .await
        .context("Failed to get tracker response")?;
    let body = response
        .bytes()
        .await
        .context("Failed to get response bytes")?;
    let tracker_response = tracker::TrackerResponse::try_from(&body)
        .context("Failed to parse tracker response")?;
    Ok(tracker_response.get_peers().0)
}

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
    Download {
        #[arg(short = 'o')]
        output: String,
        torrent: String,
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
            for peer in fetch_peers(&torrent_meta).await? {
                println!("{}", peer);
            }
        }
        Command::Handshake { torrent, peer } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let peer_addr: SocketAddrV4 = peer.parse().context("Failed to parse peer address")?;
            let _peer_addr = fetch_peers(&torrent_meta)
                .await?
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
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            anyhow::ensure!(
                piece_index < torrent_meta.info.piece_count(),
                "Piece index out of range"
            );
            let peer_addr = fetch_peers(&torrent_meta)
                .await?
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut connection =
                peer::PeerConnection::connect(peer_addr, torrent_meta.info.info_hash(), *PEER_ID)
                    .await
                    .context("Failed to establish peer connection")?;

            let piece_data = connection
                .download_piece(
                    piece_index as u32,
                    torrent_meta.info.length_of_piece(piece_index),
                )
                .await
                .context("Failed to download piece")?;
            verify_piece(&piece_data, torrent_meta.info.piece_hash(piece_index))?;

            std::fs::write(&output, &piece_data).context("Failed to write piece to disk")?;
            println!("Piece {} downloaded to {}.", piece_index, output);
        }
        Command::Download { output, torrent } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let peer_addr = fetch_peers(&torrent_meta)
                .await?
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut connection =
                peer::PeerConnection::connect(peer_addr, torrent_meta.info.info_hash(), *PEER_ID)
                    .await
                    .context("Failed to establish peer connection")?;

            let mut file_data = Vec::with_capacity(torrent_meta.info.length);
            for piece_index in 0..torrent_meta.info.piece_count() {
                let piece_data = connection
                    .download_piece(
                        piece_index as u32,
                        torrent_meta.info.length_of_piece(piece_index),
                    )
                    .await
                    .with_context(|| format!("Failed to download piece {piece_index}"))?;
                verify_piece(&piece_data, torrent_meta.info.piece_hash(piece_index))?;
                file_data.extend_from_slice(&piece_data);
            }

            std::fs::write(&output, &file_data).context("Failed to write file to disk")?;
            println!("Downloaded {} to {}.", torrent, output);
        }
    }
    Ok(())
}

fn verify_piece(piece_data: &[u8], expected_hash: &[u8]) -> anyhow::Result<()> {
    let mut hasher = Sha1::new();
    hasher.update(piece_data);
    let actual_hash: [u8; 20] = hasher.finalize().into();
    anyhow::ensure!(actual_hash == expected_hash[..], "Piece hash mismatch");
    Ok(())
}
