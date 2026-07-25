use serde_bencode::value::Value as BencodeValue;
use serde_json::Value as JsonValue;

pub fn parse(input: &[u8]) -> Result<JsonValue, serde_bencode::Error> {
    let value = serde_bencode::from_bytes::<BencodeValue>(input)?;
    Ok(decode_bencoded_value(&value))
}

pub fn decode_bencoded_value(val: &BencodeValue) -> JsonValue {
    match val {
        BencodeValue::Bytes(b) => JsonValue::String(String::from_utf8_lossy(b).to_string()),
        BencodeValue::Int(i) => JsonValue::Number((*i).into()),
        BencodeValue::List(l) => JsonValue::Array(l.iter().map(decode_bencoded_value).collect()),
        BencodeValue::Dict(d) => JsonValue::Object(
            d.iter()
                .map(|(key, value)| {
                    (
                        String::from_utf8_lossy(key).to_string(),
                        decode_bencoded_value(value),
                    )
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parser_works_str() {
        let input = "4:spam".as_bytes();
        let output = super::parse(input).unwrap();
        assert_eq!(output, serde_json::json!("spam"));
    }

    #[test]
    fn parser_works_int() {
        let input = "i3e".as_bytes();
        let output = super::parse(input).unwrap();
        assert_eq!(output, serde_json::json!(3));
    }

    #[test]
    fn parser_works_list() {
        let input = "l4:spami3ee".as_bytes();
        let output = super::parse(input).unwrap();
        assert_eq!(output, serde_json::json!(["spam", 3]));
    }

    #[test]
    fn parser_works_dict() {
        let input = "d3:cow3:moo4:spam4:eggse".as_bytes();
        let output = super::parse(input).unwrap();
        assert_eq!(
            output,
            serde_json::json!({
                "cow": "moo",
                "spam": "eggs"
            })
        );
    }
}
