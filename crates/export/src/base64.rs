//! Standard base64 (RFC 4648) for inline attachment data; no dependency.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Decodes standard or URL-safe base64, ignoring ASCII whitespace. `None` on any other byte,
/// or when the decoded size would exceed `limit`.
pub fn decode(text: &str, limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity((text.len() / 4 * 3).min(limit));
    let mut buffer = 0u32;
    let mut bits = 0;
    let mut padding = false;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                padding = true;
                continue;
            }
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        if padding {
            return None;
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if out.len() >= limit {
                return None;
            }
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(super::decode(&super::encode(data), 1 << 20).unwrap(), data);
        }
        assert_eq!(super::encode(b"foobar"), "Zm9vYmFy");
        assert!(super::decode("Zm9v!", 100).is_none());
        assert!(super::decode("Zm9vYmFy", 3).is_none());
    }
}
