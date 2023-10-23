use serde::{Deserialize, Serialize};
use serde_bencode::to_bytes;
use serde_bytes::ByteBuf;
use sha1::{Digest, Sha1};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Torrent {
    pub announce: String,
    pub info: Info,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Info {
    name: String,
    pieces: ByteBuf,
    #[serde(rename = "piece length")]
    piece_length: usize,
    // For single file torrents
    pub length: usize,
}

impl Torrent {
    pub fn read_from_file(file_path: &str) -> Result<Torrent, serde_bencode::Error> {
        let file_contents = std::fs::read(file_path).unwrap();
        serde_bencode::from_bytes(&file_contents)
    }
}

impl Info {
    pub fn get_hash(&self) -> String {
        let mut hasher = Sha1::new();
        hasher.update(to_bytes(&self).unwrap());
        hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>()
    }
}
