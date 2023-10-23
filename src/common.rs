pub fn from_hash_to_string(hash: &[u8]) -> String {
    hash.chunks(20).fold(String::new(), |acc, chunk| {
        acc + chunk
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
                .as_str() + "\n"
    })
}
