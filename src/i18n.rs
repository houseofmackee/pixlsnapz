//! Localization. User-facing text is looked up by its English wording in
//! the app bundle's `Localizable.strings` (`macos/<language>.lproj`),
//! falling back to English when there is no translation (or no bundle, as
//! in tests and `cargo run`).
//!
//! Titles passed to the menu, Settings, and alert helpers are translated
//! where they are shown; mark any other user-facing literal with [`t!`],
//! and names that are translated later (enum names, tab titles) with
//! [`n!`]. `scripts/strings.py` collects all of them into
//! `macos/en.lproj/Localizable.strings`, the template for translators, and
//! checks that every translation has every string with the same
//! placeholders.
//!
//! Text assembled at runtime uses `t!` with named placeholders:
//! `t!("Units: {name}", name)` or `t!("Copied {text}", text = size)`. The
//! template is translated first and the values filled in after, so a
//! translation can move the placeholders. Measurements, colors, and
//! anything copied to the clipboard stay as they are: they are data.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use objc2_foundation::{NSBundle, NSString};

thread_local! {
    /// Translations loaded by [`use_strings_file`], used instead of the
    /// bundle's (tests have no app bundle).
    static OVERRIDE: RefCell<Option<HashMap<String, String>>> = const { RefCell::new(None) };
}

/// For tests: translate with this `Localizable.strings` file instead of the
/// app bundle, or (with `None`) not at all.
#[doc(hidden)]
pub fn use_strings_file(path: Option<&Path>) -> std::io::Result<()> {
    let table = match path {
        Some(path) => Some(parse_strings(&std::fs::read_to_string(path)?)),
        None => None,
    };
    OVERRIDE.with(|o| *o.borrow_mut() = table);
    Ok(())
}

/// The `"key" = "value";` pairs of a `.strings` file (comments skipped).
fn parse_strings(text: &str) -> HashMap<String, String> {
    let mut table = HashMap::new();
    let mut chars = text.chars().peekable();
    let mut strings = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some(other) => s.push(other),
                            None => {}
                        },
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                strings.push(s);
            }
            ';' => {
                if let [key, value] = strings.as_slice() {
                    table.insert(key.clone(), value.clone());
                }
                strings.clear();
            }
            _ => {}
        }
    }
    table
}

/// `text` in the user's language, if the bundle has a translation.
pub fn tr(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if let Some(found) = OVERRIDE.with(|o| o.borrow().as_ref().map(|t| t.get(text).cloned())) {
        return found.unwrap_or_else(|| text.to_string());
    }
    let key = NSString::from_str(text);
    NSBundle::mainBundle().localizedStringForKey_value_table(&key, Some(&key), None).to_string()
}

/// Replaces each `{name}` in `template` with its value; `{{` and `}}` are
/// literal braces. Unknown placeholders are left as they are.
pub fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        let value = tail.strip_prefix('{').and_then(|t| {
            let end = t.find('}')?;
            let (_, value) = values.iter().find(|(name, _)| *name == &t[..end])?;
            Some((value, end + 2))
        });
        match value {
            Some((value, len)) => {
                out.push_str(value);
                rest = &tail[len..];
            }
            None => {
                out.push_str(&tail[..1]);
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Marks a string literal for translation without translating it, for names
/// that are translated with [`tr`] where they are shown.
#[macro_export]
macro_rules! n {
    ($text:literal) => {
        $text
    };
}

/// Marks a user-facing string literal for translation and translates it.
/// Named placeholders take values after the template: `t!("{n} items", n)`
/// uses the variable `n`, `t!("{n} items", n = list.len())` an expression.
#[macro_export]
macro_rules! t {
    (@value $name:ident) => {
        $name
    };
    (@value $name:ident = $value:expr) => {
        $value
    };
    ($text:literal) => {
        $crate::i18n::tr($text)
    };
    ($text:literal, $($name:ident $(= $value:expr)?),+ $(,)?) => {{
        let values = [$((stringify!($name), $crate::t!(@value $name $(= $value)?).to_string())),+];
        #[cfg(debug_assertions)]
        for (name, _) in &values {
            debug_assert!($text.contains(&format!("{{{name}}}")), "t!: {:?} has no {{{name}}}", $text);
        }
        $crate::i18n::fill(&$crate::i18n::tr($text), &values)
    }};
}

#[cfg(test)]
mod tests {
    use super::fill;

    #[test]
    fn fills_named_placeholders() {
        let values = [("n", "3".to_string()), ("what", "items".to_string())];
        assert_eq!(fill("{n} {what}", &values), "3 items");
        assert_eq!(fill("{what}: {n}, {n}", &values), "items: 3, 3");
        assert_eq!(fill("{{n}} {n}", &values), "{n} 3");
        assert_eq!(fill("{other} {n}", &values), "{other} 3");
        assert_eq!(fill("open { brace", &values), "open { brace");
    }

    #[test]
    fn parses_strings_files() {
        let table =
            super::parse_strings("/* a comment; with \"quotes\" */\n\"A \\\"b\\\"\" = \"C\\nd\";\n\"x\" = \"y\";\n");
        assert_eq!(table.get("A \"b\"").map(String::as_str), Some("C\nd"));
        assert_eq!(table.get("x").map(String::as_str), Some("y"));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn macro_takes_variables_and_expressions() {
        let n = 2;
        assert_eq!(t!("{n} kept", n), "2 kept");
        assert_eq!(t!("{a} by {b}", a = 3 * 2, b = "x"), "6 by x");
        // Without a bundle, text stays English.
        assert_eq!(t!("Measure"), "Measure");
    }
}
