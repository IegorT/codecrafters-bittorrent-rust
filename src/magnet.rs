use crate::common::url_decode;
use anyhow::Context;

#[derive(Debug)]
pub struct MagnetLink {
    pub info_hash: String,
    pub display_name: Option<String>,
    pub tracker_url: Option<String>,
}

impl MagnetLink {
    pub fn parse(link: &str) -> anyhow::Result<Self> {
        let query = link
            .strip_prefix("magnet:?")
            .context("Not a valid magnet link")?;

        let mut info_hash = None;
        let mut display_name = None;
        let mut tracker_url = None;

        for pair in query.split('&') {
            let (key, value) = pair
                .split_once('=')
                .context("Malformed magnet link parameter")?;
            match key {
                "xt" => {
                    info_hash = Some(
                        value
                            .strip_prefix("urn:btih:")
                            .context("Unsupported xt format")?
                            .to_string(),
                    )
                }
                "dn" => display_name = Some(url_decode(value)),
                "tr" => tracker_url = Some(url_decode(value)),
                _ => {}
            }
        }

        Ok(MagnetLink {
            info_hash: info_hash.context("Magnet link missing info hash")?,
            display_name,
            tracker_url,
        })
    }
}
