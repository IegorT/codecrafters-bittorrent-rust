pub fn url_encode(value: &[u8]) -> String {
    let mut encoded = String::new();

    for byte in value {
        if byte.is_ascii_alphanumeric()
            || *byte == b'-'
            || *byte == b'_'
            || *byte == b'.'
            || *byte == b'~'
        {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(format!("%{:02X}", byte).as_str());
        }
    }

    encoded
}
