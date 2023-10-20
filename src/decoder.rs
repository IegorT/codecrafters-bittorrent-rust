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
        BencodeValue::List(l) => {
            let mut list: Vec<JsonValue> = Vec::new();

            for item in l {
                let value = decode_bencoded_value(item);
                list.push(value);
            }

            JsonValue::Array(list)
        }
        BencodeValue::Dict(d) => {
            let mut dict: serde_json::Map<String, JsonValue> = serde_json::Map::new();

            for (key, value) in d {
                let key_string = String::from_utf8_lossy(key).to_string();
                let value = decode_bencoded_value(value);
                dict.insert(key_string, value);
            }

            JsonValue::Object(dict)
        }
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
