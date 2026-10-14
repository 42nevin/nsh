//! Maps `bundle.js:LINE:COL` locations in JS stack traces back to the app
//! sources with the bundle's source map.

use oxc_sourcemap::SourceMap;
use yf_js::BUNDLE_NAME;

pub(crate) struct Symbolicator {
    json: Option<std::sync::Arc<str>>,
    /// Parsed on first use: errors are rare, parsing is not free.
    map: Option<Option<SourceMap<'static>>>,
}

impl Symbolicator {
    pub(crate) fn new(json: Option<std::sync::Arc<str>>) -> Self {
        Self { json, map: None }
    }

    /// Rewrites every `bundle.js:L:C` in `text` to `source:L:C`.
    pub(crate) fn rewrite(&mut self, text: &str) -> String {
        let Some(json) = &self.json else {
            return text.to_owned();
        };
        let map = self.map.get_or_insert_with(|| {
            SourceMap::from_json_string(json)
                .ok()
                .map(SourceMap::into_owned)
        });
        let Some(map) = map else {
            return text.to_owned();
        };
        let table = map.generate_lookup_table();
        let needle = format!("{BUNDLE_NAME}:");
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(&needle) {
            out.push_str(&rest[..at]);
            let after = &rest[at + needle.len()..];
            match parse_position(after) {
                Some((line, col, used)) => {
                    // Stack positions are 1-based; source maps are 0-based.
                    let token =
                        map.lookup_token(&table, line.saturating_sub(1), col.saturating_sub(1));
                    match token.and_then(|t| Some((map.get_source(t.get_source_id()?)?, t))) {
                        Some((source, token)) => out.push_str(&format!(
                            "{}:{}:{}",
                            source.trim_start_matches("./"),
                            token.get_src_line() + 1,
                            token.get_src_col() + 1
                        )),
                        None => out.push_str(&rest[at..at + needle.len() + used]),
                    }
                    rest = &after[used..];
                }
                None => {
                    out.push_str(&needle);
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// Parses `LINE:COL` at the start of `s`; returns both and the bytes used.
fn parse_position(s: &str) -> Option<(u32, u32, usize)> {
    let line_len = s.bytes().take_while(u8::is_ascii_digit).count();
    let line = s[..line_len].parse().ok()?;
    let rest = s[line_len..].strip_prefix(':')?;
    let col_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    let col = rest[..col_len].parse().ok()?;
    Some((line, col, line_len + 1 + col_len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positions() {
        assert_eq!(parse_position("12:7)"), Some((12, 7, 4)));
        assert_eq!(parse_position("12)"), None);
    }

    #[test]
    fn leaves_text_without_map_alone() {
        let mut s = Symbolicator::new(None);
        assert_eq!(s.rewrite("at f (bundle.js:1:2)"), "at f (bundle.js:1:2)");
    }
}
