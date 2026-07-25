use anyhow::Context;
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
    pub pieces: ByteBuf,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    // For single file torrents
    pub length: usize,
}

impl Torrent {
    pub fn read_from_file(file_path: &str) -> anyhow::Result<Torrent> {
        let file_contents = std::fs::read(file_path).context("Failed to read file")?;
        Ok(serde_bencode::from_bytes(&file_contents)?)
    }
}

impl Info {
    pub fn info_hash(&self) -> [u8; 20] {
        let mut hasher = Sha1::new();
        hasher.update(to_bytes(&self).unwrap());
        hasher.finalize().into()
    }

    pub fn piece_count(&self) -> usize {
        self.pieces.len() / 20
    }

    pub fn piece_hash(&self, index: usize) -> &[u8] {
        &self.pieces[index * 20..index * 20 + 20]
    }

    pub fn length_of_piece(&self, index: usize) -> usize {
        if index == self.piece_count() - 1 {
            let remainder = self.length % self.piece_length;
            if remainder == 0 {
                self.piece_length
            } else {
                remainder
            }
        } else {
            self.piece_length
        }
    }
}
