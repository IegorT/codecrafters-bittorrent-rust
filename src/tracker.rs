use std::net::Ipv4Addr;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::common::url_encode;
use crate::torrent::Torrent;

#[derive(Serialize, Deserialize, Debug)]
pub struct Tracker {
    url: String,
    info_hash: [u8; 20],
    peer_id: String,
    port: usize,
    uploaded: usize,
    downloaded: usize,
    compact: u8,
    pub peers: Vec<Peer>,
    interval: Option<usize>,
    left: usize,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker {
            url: String::new(),
            info_hash: [0; 20],
            peer_id: "00112233445566778899".to_string(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            compact: 1,
            peers: Vec::new(),
            interval: None,
            left: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Peer {
    pub ip: [u8; 4],
    pub port: u16,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TrackerResponse {
    interval: usize,
    peers: ByteBuf,
}

pub struct TrackerRequest {
    url: String,
    info_hash: [u8; 20],
    peer_id: String,
    port: usize,
    uploaded: usize,
    downloaded: usize,
    compact: u8,
    left: usize,
}


impl ToString for TrackerRequest {
    fn to_string(&self) -> String {
        let mut url = String::new();
        url.push_str(&self.url);
        url.push_str("?");
        url.push_str("info_hash=");
        url.push_str(&url_encode(&self.info_hash));
        url.push_str("&");
        url.push_str("peer_id=");
        url.push_str(&self.peer_id);
        url.push_str("&");
        url.push_str("port=");
        url.push_str(&self.port.to_string());
        url.push_str("&");
        url.push_str("uploaded=");
        url.push_str(&self.uploaded.to_string());
        url.push_str("&");
        url.push_str("downloaded=");
        url.push_str(&self.downloaded.to_string());
        url.push_str("&");
        url.push_str("left=");
        url.push_str(&self.left.to_string());
        url.push_str("&");
        url.push_str("compact=");
        url.push_str(&self.compact.to_string());
        url
    }
}

impl ToString for Peer {
    fn to_string(&self) -> String {
        format!("{}:{}", Ipv4Addr::from(self.ip), self.port)
    }
}

impl Tracker {
    pub fn new(torrent: &Torrent) -> Self {
        let mut tr = Tracker::default();
        tr.url = torrent.announce.clone();
        tr.left = torrent.info.length;
        tr.info_hash = torrent.info.get_hash().clone();
        tr
    }

    pub fn get_request(&self) -> TrackerRequest {
        TrackerRequest {
            url: self.url.clone(),
            info_hash: self.info_hash,
            peer_id: self.peer_id.clone(),
            port: self.port,
            uploaded: self.uploaded,
            downloaded: self.downloaded,
            compact: self.compact,
            left: self.left,
        }
    }

    pub fn response(&mut self, res: Bytes) {
        let res = serde_bencode::from_bytes::<TrackerResponse>(&res)
            .expect("Failed to parse tracker response");
        self.peers = res
            .peers
            .chunks(6)
            .map(|chunk| Peer {
                ip: [chunk[0], chunk[1], chunk[2], chunk[3]],
                port: u16::from_be_bytes([chunk[4], chunk[5]]),
            })
            .collect();
    }
}
