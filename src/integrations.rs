//! Handing measurements to other apps.

/// CleanShotZ's bundle identifier, to find out whether it is installed.
pub const CLEANSHOTZ_BUNDLE_ID: &str = "io.github.houseofmackee.cleanshotz";

/// The URL that opens `path` in CleanShotZ's editor
/// (`cleanshotz://open-annotate?filepath=…`).
pub fn cleanshotz_annotate_url(path: &str) -> String {
    format!("cleanshotz://open-annotate?filepath={}", percent_encode(path))
}

/// `text` with everything but unreserved URL characters and `/`
/// percent-encoded (UTF-8), for a query value.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotate_urls() {
        assert_eq!(
            cleanshotz_annotate_url("/Users/me/Library/Caches/PixlSnapz/PixlSnapz 2026-11-01 at 18.05.34.png"),
            "cleanshotz://open-annotate?filepath=/Users/me/Library/Caches/PixlSnapz/PixlSnapz%202026-11-01%20at%2018.05.34.png"
        );
        assert_eq!(cleanshotz_annotate_url("/a&b=c+ü"), "cleanshotz://open-annotate?filepath=/a%26b%3Dc%2B%C3%BC");
    }
}
