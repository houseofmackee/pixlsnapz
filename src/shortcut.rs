//! Keyboard shortcuts written as text, such as "shift+cmd+1".

use std::fmt;

/// Carbon modifier bits (`cmdKey`, `shiftKey`, `optionKey`, `controlKey`).
pub const CMD: u32 = 0x0100;
pub const SHIFT: u32 = 0x0200;
pub const OPTION: u32 = 0x0800;
pub const CONTROL: u32 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// Virtual key code (kVK_*).
    pub key_code: u16,
    /// Carbon modifier bits.
    pub modifiers: u32,
}

/// Key names and their ANSI virtual key codes.
const KEYS: &[(&str, u16)] = &[
    ("a", 0x00),
    ("s", 0x01),
    ("d", 0x02),
    ("f", 0x03),
    ("h", 0x04),
    ("g", 0x05),
    ("z", 0x06),
    ("x", 0x07),
    ("c", 0x08),
    ("v", 0x09),
    ("b", 0x0B),
    ("q", 0x0C),
    ("w", 0x0D),
    ("e", 0x0E),
    ("r", 0x0F),
    ("y", 0x10),
    ("t", 0x11),
    ("1", 0x12),
    ("2", 0x13),
    ("3", 0x14),
    ("4", 0x15),
    ("6", 0x16),
    ("5", 0x17),
    ("=", 0x18),
    ("9", 0x19),
    ("7", 0x1A),
    ("-", 0x1B),
    ("8", 0x1C),
    ("0", 0x1D),
    ("]", 0x1E),
    ("o", 0x1F),
    ("u", 0x20),
    ("[", 0x21),
    ("i", 0x22),
    ("p", 0x23),
    ("l", 0x25),
    ("j", 0x26),
    ("'", 0x27),
    ("k", 0x28),
    (";", 0x29),
    ("\\", 0x2A),
    (",", 0x2B),
    ("/", 0x2C),
    ("n", 0x2D),
    ("m", 0x2E),
    (".", 0x2F),
    ("`", 0x32),
    ("space", 0x31),
    ("f1", 0x7A),
    ("f2", 0x78),
    ("f3", 0x63),
    ("f4", 0x76),
    ("f5", 0x60),
    ("f6", 0x61),
    ("f7", 0x62),
    ("f8", 0x64),
    ("f9", 0x65),
    ("f10", 0x6D),
    ("f11", 0x67),
    ("f12", 0x6F),
];

impl Shortcut {
    pub fn parse(text: &str) -> Result<Shortcut, String> {
        let mut modifiers = 0;
        let mut key = None;
        for part in text.split('+').map(|p| p.trim().to_ascii_lowercase()) {
            match part.as_str() {
                "cmd" | "command" | "⌘" => modifiers |= CMD,
                "shift" | "⇧" => modifiers |= SHIFT,
                "opt" | "option" | "alt" | "⌥" => modifiers |= OPTION,
                "ctrl" | "control" | "⌃" => modifiers |= CONTROL,
                "" => return Err(format!("\"{text}\" has an empty part")),
                name => {
                    if key.is_some() {
                        return Err(format!("\"{text}\" has more than one key"));
                    }
                    let code = KEYS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c);
                    key = Some(code.ok_or_else(|| format!("unknown key \"{name}\" in \"{text}\""))?);
                }
            }
        }
        let key_code = key.ok_or_else(|| format!("\"{text}\" has no key"))?;
        if modifiers == 0 && !(0x60..=0x7A).contains(&key_code) {
            return Err(format!("\"{text}\" needs a modifier such as cmd"));
        }
        Ok(Shortcut { key_code, modifiers })
    }

    /// The menu key equivalent: the key as a lowercase string.
    pub fn key_name(&self) -> &'static str {
        KEYS.iter().find(|(_, c)| *c == self.key_code).map_or("", |(n, _)| n)
    }
}

impl fmt::Display for Shortcut {
    /// macOS style, for example "⇧⌘1".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (bit, symbol) in [(CONTROL, "⌃"), (OPTION, "⌥"), (SHIFT, "⇧"), (CMD, "⌘")] {
            if self.modifiers & bit != 0 {
                f.write_str(symbol)?;
            }
        }
        let name = self.key_name();
        if name == "space" { f.write_str("Space") } else { f.write_str(&name.to_uppercase()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let s = Shortcut::parse("shift+cmd+1").unwrap();
        assert_eq!(s, Shortcut { key_code: 0x12, modifiers: SHIFT | CMD });
        assert_eq!(s.to_string(), "⇧⌘1");
        let s = Shortcut::parse(" Ctrl + Option + M ").unwrap();
        assert_eq!(s.modifiers, CONTROL | OPTION);
        assert_eq!(s.to_string(), "⌃⌥M");
        assert_eq!(Shortcut::parse("f5").unwrap().key_code, 0x60);
    }

    #[test]
    fn rejects_bad_shortcuts() {
        assert!(Shortcut::parse("cmd+shift").is_err());
        assert!(Shortcut::parse("cmd+a+b").is_err());
        assert!(Shortcut::parse("cmd+nope").is_err());
        assert!(Shortcut::parse("a").is_err());
        assert!(Shortcut::parse("cmd++").is_err());
    }
}
