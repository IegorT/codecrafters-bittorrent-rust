use std::fmt::Write;

pub fn url_encode(value: &[u8]) -> String {
    let mut encoded = String::with_capacity(value.len() * 3);

    for &byte in value {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }

    encoded
}
