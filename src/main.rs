use anyhow::Context;
use bittorrent_starter_rust::{decoder::parse as decode_bencoded_value, peer, torrent, tracker};
use clap::{Parser, Subcommand};
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Decode { value } => {
            let decoded_value =
                decode_bencoded_value(value.as_bytes()).expect("Failed to decode value");
            println!("{}", decoded_value);
        }
        Command::Info { torrent } => {
            let torrent_meta =
                torrent::Torrent::read_from_file(&torrent).expect("Failed to read torrent file");
            println!("Tracker URL: {}", torrent_meta.announce);
            println!("Length: {}", torrent_meta.info.length);
            println!("Info Hash: {}", hex::encode(&torrent_meta.info.get_hash()));
            println!("Piece Length: {}", torrent_meta.info.piece_length);
            println!("Piece Hashes:");
            for hash in torrent_meta.info.pieces.chunks(20) {
                println!("{}", hex::encode(hash));
            }
        }
        Command::Peers { torrent } => {
            let torrent_meta =
                torrent::Torrent::read_from_file(&torrent).expect("Failed to read torrent file");
            let tracker = tracker::TrackerRequest::new(PEER_ID, &torrent_meta);
            let response = reqwest::get(tracker.get_url())
                .await
                .expect("Failed to get response");
            let response = response
                .bytes()
                .await
                .expect("Failed to get response bytes");
            let response = tracker::TrackerResponse::try_from(&response)
                .expect("Failed to parse tracker response");
            for peer in response.get_peers().0 {
                println!("{}", peer.to_string());
            }
        }
        Command::Handshake { torrent, peer } => {
            let torrent_meta =
                torrent::Torrent::read_from_file(&torrent).expect("Failed to read torrent file");
            let tracker = tracker::TrackerRequest::new(PEER_ID, &torrent_meta);
            let response = reqwest::get(tracker.get_url())
                .await
                .expect("Failed to get response");
            let response = response
                .bytes()
                .await
                .expect("Failed to get response bytes");
            let response = tracker::TrackerResponse::try_from(&response)
                .expect("Failed to parse tracker response");
            let peer = response
                .get_peers()
                .0
                .into_iter()
                .find(|p| p.to_string() == peer)
                .expect("Failed to find peer");
            let mut peer = tokio::net::TcpStream::connect(peer)
                .await
                .context("connect to peer")?;
            let handshake = bytes::Bytes::try_from(&peer::Handshake::new(
                torrent_meta.info.get_hash(),
                *PEER_ID,
            ))
            .expect("Failed to create handshake");
            {
                let mut buf: [u8; 68] = [0; 68];
                peer.write_all(handshake.as_ref())
                    .await
                    .expect("Failed to write handshake");
                peer.read(&mut buf).await.expect("Failed to read handshake");
                let handshake = peer::Handshake::try_from(&buf).expect("Failed to parse handshake");
                println!("Peer ID: {}", hex::encode(&handshake.peer_id));
            }
        }
    }
    Ok(())
}
