use crate::common::url_encode;
use crate::peer::Peers;
use crate::torrent::Torrent;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackerRequest {
    url: String,
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    port: usize,
    uploaded: usize,
    downloaded: usize,
    compact: u8,
    left: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackerResponse {
    // interval: usize,
    pub peers: ByteBuf,
}

impl TrackerRequest {
    pub fn new(peer_id: &[u8; 20], torrent: &Torrent) -> Self {
        TrackerRequest {
            url: torrent.announce.clone(),
            info_hash: torrent.info.info_hash(),
            peer_id: *peer_id,
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            compact: 1,
            left: torrent.info.length,
        }
    }

    pub fn new_for_magnet(peer_id: &[u8; 20], tracker_url: &str, info_hash: [u8; 20]) -> Self {
        TrackerRequest {
            url: tracker_url.to_string(),
            info_hash,
            peer_id: *peer_id,
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            compact: 1,
            // Actual file length is unknown from a magnet link alone until the
            // metadata exchange happens, so this is a placeholder.
            left: 999,
        }
    }

    pub fn url(&self) -> String {
        format!(
            "{}?info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&compact={}&left={}",
            self.url,
            url_encode(&self.info_hash),
            url_encode(&self.peer_id),
            self.port,
            self.uploaded,
            self.downloaded,
            self.compact,
            self.left
        )
    }
}

impl TryFrom<&Bytes> for TrackerResponse {
    type Error = serde_bencode::Error;

    fn try_from(value: &Bytes) -> Result<Self, Self::Error> {
        serde_bencode::from_bytes(value)
    }
}

impl TrackerResponse {
    pub fn get_peers(&self) -> Peers {
        Peers::new(&self.peers)
    }
}
