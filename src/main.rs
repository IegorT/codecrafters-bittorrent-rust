use anyhow::Context;
use bittorrent_starter_rust::{
    decoder::parse as decode_bencoded_value, magnet, peer, torrent, tracker,
};
use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};
use std::net::SocketAddrV4;

// Available if you need it!
// use serde_bencode

const UT_METADATA_ID: u8 = 1;

async fn fetch_peers(tracker_req: &tracker::TrackerRequest) -> anyhow::Result<Vec<SocketAddrV4>> {
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
    MagnetParse {
        link: String,
    },
    MagnetHandshake {
        link: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // A fixed peer id is prone to collisions when peers only accept one
    // connection per id, so generate a fresh one per run.
    let peer_id: [u8; 20] = rand::random();
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
            let tracker_req = tracker::TrackerRequest::new(&peer_id, &torrent_meta);
            for peer in fetch_peers(&tracker_req).await? {
                println!("{}", peer);
            }
        }
        Command::Handshake { torrent, peer } => {
            let torrent_meta = torrent::Torrent::read_from_file(&torrent)
                .context("Failed to read torrent file")?;
            let peer_addr: SocketAddrV4 = peer.parse().context("Failed to parse peer address")?;
            let tracker_req = tracker::TrackerRequest::new(&peer_id, &torrent_meta);
            let _peer_addr = fetch_peers(&tracker_req)
                .await?
                .into_iter()
                .find(|p| *p == peer_addr)
                .context("Peer not found in tracker response")?;
            let mut stream = tokio::net::TcpStream::connect(peer_addr)
                .await
                .context("Failed to connect to peer")?;
            let handshake =
                peer::Handshake::perform(&mut stream, torrent_meta.info.info_hash(), peer_id, false)
                    .await?;
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
            let tracker_req = tracker::TrackerRequest::new(&peer_id, &torrent_meta);
            let peer_addr = fetch_peers(&tracker_req)
                .await?
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut connection =
                peer::PeerConnection::connect(peer_addr, torrent_meta.info.info_hash(), peer_id)
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
            let tracker_req = tracker::TrackerRequest::new(&peer_id, &torrent_meta);
            let peer_addr = fetch_peers(&tracker_req)
                .await?
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut connection =
                peer::PeerConnection::connect(peer_addr, torrent_meta.info.info_hash(), peer_id)
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
        Command::MagnetParse { link } => {
            let magnet_link = magnet::MagnetLink::parse(&link).context("Failed to parse magnet link")?;
            if let Some(tracker_url) = &magnet_link.tracker_url {
                println!("Tracker URL: {}", tracker_url);
            }
            println!("Info Hash: {}", magnet_link.info_hash);
        }
        Command::MagnetHandshake { link } => {
            let magnet_link = magnet::MagnetLink::parse(&link).context("Failed to parse magnet link")?;
            let info_hash = magnet_link.info_hash_bytes()?;
            let tracker_url = magnet_link
                .tracker_url
                .context("Magnet link missing tracker URL")?;

            let tracker_req = tracker::TrackerRequest::new_for_magnet(&peer_id, &tracker_url, info_hash);
            let peer_addr = fetch_peers(&tracker_req)
                .await?
                .into_iter()
                .next()
                .context("No peers available")?;

            let mut stream = tokio::net::TcpStream::connect(peer_addr)
                .await
                .context("Failed to connect to peer")?;
            let handshake = peer::Handshake::perform(&mut stream, info_hash, peer_id, true).await?;
            println!("Peer ID: {}", hex::encode(handshake.peer_id));

            let bitfield = peer::Message::read(&mut stream)
                .await
                .context("Failed to read bitfield message")?;
            anyhow::ensure!(
                bitfield.id == peer::Message::BITFIELD,
                "Expected bitfield message, got id {}",
                bitfield.id
            );

            if handshake.supports_extensions() {
                peer::Message::extension_handshake(UT_METADATA_ID)?
                    .write(&mut stream)
                    .await
                    .context("Failed to send extension handshake")?;

                let peer_extensions = peer::Message::read(&mut stream)
                    .await
                    .context("Failed to read extension handshake message")?
                    .parse_extension_handshake()?;
                let peer_metadata_id = peer_extensions
                    .get("ut_metadata")
                    .context("Peer does not support the ut_metadata extension")?;
                println!("Peer Metadata Extension ID: {}", peer_metadata_id);
            }
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
