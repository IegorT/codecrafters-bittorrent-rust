use clap::{Parser, Subcommand};
use bittorrent_starter_rust::decoder::parse as decode_bencoded_value;

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
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Decode { value } => {
            let decoded_value = decode_bencoded_value(value.as_bytes()).expect("Failed to decode value");
            println!("{}", decoded_value);
        }
    }
    Ok(())
}
