//! Handing measurements to other apps.

/// What a `pixlsnapz://` URL asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlCommand {
    /// `pixlsnapz://measure`, optionally `?set=Name` to load a saved set.
    Measure { set: Option<String> },
    /// `pixlsnapz://settings`, optionally `?tab=measuring`.
    Settings { tab: Option<String> },
}

/// The command in a `pixlsnapz://` URL, or `None` for anything else.
pub fn parse_url(url: &str) -> Option<UrlCommand> {
    let rest = url.strip_prefix("pixlsnapz://").or_else(|| url.strip_prefix("pixlsnapz:"))?;
    let (command, query) = rest.split_once('?').unwrap_or((rest, ""));
    let param = |key: &str| {
        query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == key).then(|| percent_decode(v)).filter(|v| !v.is_empty())
        })
    };
    match command.trim_end_matches('/').to_ascii_lowercase().as_str() {
        "" | "measure" => Some(UrlCommand::Measure { set: param("set") }),
        "settings" => Some(UrlCommand::Settings { tab: param("tab") }),
        _ => None,
    }
}

/// Decodes `%XX` escapes (UTF-8) and `+` as a space; a `%` without two hex
/// digits after it stays as it is.
fn percent_decode(text: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16);
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped =
            (bytes[i] == b'%').then(|| Some(hex(*bytes.get(i + 1)?)? * 16 + hex(*bytes.get(i + 2)?)?)).flatten();
        match (escaped, bytes[i]) {
            (Some(value), _) => {
                out.push(value as u8);
                i += 3;
            }
            (None, b'+') => {
                out.push(b' ');
                i += 1;
            }
            (None, b) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

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
    fn parses_urls() {
        assert_eq!(parse_url("pixlsnapz://measure"), Some(UrlCommand::Measure { set: None }));
        assert_eq!(parse_url("pixlsnapz://"), Some(UrlCommand::Measure { set: None }));
        assert_eq!(
            parse_url("pixlsnapz://measure?set=Header%20grid"),
            Some(UrlCommand::Measure { set: Some("Header grid".into()) })
        );
        assert_eq!(parse_url("pixlsnapz://measure?set=A+b&x=1"), Some(UrlCommand::Measure { set: Some("A b".into()) }));
        assert_eq!(
            parse_url("pixlsnapz://settings/?tab=measuring"),
            Some(UrlCommand::Settings { tab: Some("measuring".into()) })
        );
        assert_eq!(parse_url("pixlsnapz://settings?tab="), Some(UrlCommand::Settings { tab: None }));
        assert_eq!(parse_url("pixlsnapz://launch-rockets"), None);
        assert_eq!(parse_url("https://example.com"), None);
        assert_eq!(
            parse_url("pixlsnapz://measure?set=%C3%BC%2"),
            Some(UrlCommand::Measure { set: Some("ü%2".into()) })
        );
        // A non-ASCII character right after a % is left alone, not split.
        assert_eq!(parse_url("pixlsnapz://measure?set=%ü"), Some(UrlCommand::Measure { set: Some("%ü".into()) }));
    }

    #[test]
    fn annotate_urls() {
        assert_eq!(
            cleanshotz_annotate_url("/Users/me/Library/Caches/PixlSnapz/PixlSnapz 2026-11-01 at 18.05.34.png"),
            "cleanshotz://open-annotate?filepath=/Users/me/Library/Caches/PixlSnapz/PixlSnapz%202026-11-01%20at%2018.05.34.png"
        );
        assert_eq!(cleanshotz_annotate_url("/a&b=c+ü"), "cleanshotz://open-annotate?filepath=/a%26b%3Dc%2B%C3%BC");
    }
}
