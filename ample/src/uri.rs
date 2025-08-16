// NONALNUM here meaning non-alphanumeric
const UNRESERVED_NONALNUM_CHARS: [char; 4] = ['-', '_', '~', '.'];

fn is_unreserved_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || UNRESERVED_NONALNUM_CHARS.contains(&c)
}

pub fn percent_encode(value: &str) -> String {
    let mut encoded_string = String::new();
    for c in value.chars() {
        if !is_unreserved_char(c) {
            let char32: u32 = c as u32;
            if c.is_ascii() {
                // formatting string should format the char as a hex number with 0's padding the beginning of the number
                encoded_string.push_str(&format!("%{char32:02x}"));
            } else {
                let mut char_bytes: [u8;4] = [0;4];
                c.encode_utf8(&mut char_bytes);
                for b in char_bytes {
                    if b == 0 {
                        continue;
                    }
                    encoded_string.push_str(&format!("%{b:02x}"));
                }
            }
        } else {
            encoded_string.push(c);
        }
    }

    encoded_string
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("!#$&\'()*+,/:;=?@[]"), "%21%23%24%26%27%28%29%2a%2b%2c%2f%3a%3b%3d%3f%40%5b%5d");
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("ABC123"), "ABC123");
        assert_eq!(percent_encode("King Gizzard and the Lizard Wizard"), "King%20Gizzard%20and%20the%20Lizard%20Wizard");
        assert_eq!(percent_encode("€"), "%e2%82%ac")
    }
}