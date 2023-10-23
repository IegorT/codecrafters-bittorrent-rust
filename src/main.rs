use bittorrent_starter_rust::{common, decoder::parse as decode_bencoded_value, torrent, tracker};
use clap::{Parser, Subcommand};

// Available if you need it!
// use serde_bencode

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
    Info { value: String },
    Peers { value: String },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Decode { value } => {
            let decoded_value =
                decode_bencoded_value(value.as_bytes()).expect("Failed to decode value");
            println!("{}", decoded_value);
        }
        Command::Info { value } => {
            let torrent_meta =
                torrent::Torrent::read_from_file(&value).expect("Failed to read torrent file");
            println!("Tracker URL: {}", torrent_meta.announce);
            println!("Length: {}", torrent_meta.info.length);
            println!(
                "Info Hash: {}",
                common::from_hash_to_string(&torrent_meta.info.get_hash())
            );
            println!("Piece Length: {}", torrent_meta.info.piece_length);
            println!("Piece Hashes:");
            for hash in torrent_meta.info.pieces.chunks(20) {
                println!("{}", common::from_hash_to_string(hash));
            }
        }
        Command::Peers { value } => {
            let torrent_meta =
                torrent::Torrent::read_from_file(&value).expect("Failed to read torrent file");
            let mut tracker = tracker::Tracker::new(&torrent_meta);
            let req = tracker.get_request();

            let body = reqwest::blocking::get(req.to_string())
                .expect("Failed to get peers")
                .bytes()
                .expect("Failed to get response body");

            tracker.response(body);

            for peer in tracker.peers {
                println!("{}", peer.to_string());
            }
        }
    }
    Ok(())
}
