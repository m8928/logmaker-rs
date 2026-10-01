//! Log format templates and scenario override references.
//!
//! A log format is literal text with `<maker_name>` tokens (StringTemplate
//! style, as in the Java edition). A token is `<`, an identifier
//! (`[A-Za-z_][A-Za-z0-9_-]*`, optional surrounding spaces) and `>`; anything
//! else, such as `<134>` or `a < b`, stays literal. `\<` writes a literal `<`.
//!
//! Scenario step overrides may reference shared variables Velocity-style:
//! `$name`, `${name}`, and the quiet forms `$!name` / `$!{name}` that render
//! nothing when the variable is undefined.

use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Text(String),
    Token(usize),
}

/// A parsed log format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTemplate {
    segments: Vec<Segment>,
    /// Distinct token names in order of first appearance.
    names: Vec<String>,
    literal_len: usize,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Whether `name` can be written as a `<name>` token.
pub fn is_token_name(name: &str) -> bool {
    name.bytes().next().is_some_and(is_ident_start) && name.bytes().all(is_ident_continue)
}

/// Parses `<name>` starting at `start` (which holds `<`); returns the name and
/// the index after `>`.
fn token_at(s: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let skip_spaces = |mut i: usize| {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        i
    };
    let name_start = skip_spaces(start + 1);
    if !bytes.get(name_start).copied().is_some_and(is_ident_start) {
        return None;
    }
    let mut name_end = name_start + 1;
    while name_end < bytes.len() && is_ident_continue(bytes[name_end]) {
        name_end += 1;
    }
    let close = skip_spaces(name_end);
    (bytes.get(close) == Some(&b'>')).then(|| (&s[name_start..name_end], close + 1))
}

impl LogTemplate {
    pub fn parse(format: &str) -> Self {
        let mut segments = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut text = String::new();
        let mut literal_len = 0;
        let bytes = format.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if bytes.get(i + 1) == Some(&b'<') => {
                    text.push('<');
                    i += 2;
                    continue;
                }
                b'<' => {
                    if let Some((name, next)) = token_at(format, i) {
                        if !text.is_empty() {
                            literal_len += text.len();
                            segments.push(Segment::Text(std::mem::take(&mut text)));
                        }
                        let index = names.iter().position(|n| n == name).unwrap_or_else(|| {
                            names.push(name.to_owned());
                            names.len() - 1
                        });
                        segments.push(Segment::Token(index));
                        i = next;
                        continue;
                    }
                }
                _ => {}
            }
            let ch = format[i..].chars().next().expect("index is on a char boundary");
            text.push(ch);
            i += ch.len_utf8();
        }
        if !text.is_empty() {
            literal_len += text.len();
            segments.push(Segment::Text(text));
        }
        Self {
            segments,
            names,
            literal_len,
        }
    }

    /// Distinct maker names referenced by the template.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Renders with one value per entry of [`LogTemplate::names`]; a name used
    /// several times gets the same value everywhere.
    pub fn render<S: AsRef<str>>(&self, values: &[S]) -> String {
        self.render_with(|i| Some(values[i].as_ref()))
    }

    /// Renders with optional values; missing ones keep their `<name>` token.
    pub fn render_with<'a>(&self, value: impl Fn(usize) -> Option<&'a str>) -> String {
        let mut out = String::with_capacity(self.literal_len + 16 * self.names.len());
        for segment in &self.segments {
            match segment {
                Segment::Text(text) => out.push_str(text),
                Segment::Token(i) => match value(*i) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push('<');
                        out.push_str(&self.names[*i]);
                        out.push('>');
                    }
                },
            }
        }
        out
    }
}

/// Replaces `$name`, `${name}`, `$!name` and `$!{name}` with values from
/// `vars`. Undefined references stay as written, except the quiet forms which
/// render nothing.
pub fn render_references(value: &str, vars: &IndexMap<String, String>) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let quiet = bytes.get(j) == Some(&b'!');
        if quiet {
            j += 1;
        }
        let reference = if bytes.get(j) == Some(&b'{') {
            value[j + 1..].find('}').and_then(|len| {
                let name = &value[j + 1..j + 1 + len];
                let valid = name.bytes().next().is_some_and(is_ident_start) && name.bytes().all(is_ident_continue);
                valid.then_some((name, j + 2 + len))
            })
        } else if bytes.get(j).copied().is_some_and(is_ident_start) {
            let mut end = j + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            Some((&value[j..end], end))
        } else {
            None
        };
        let Some((name, end)) = reference else {
            i += 1;
            continue;
        };
        out.push_str(&value[copied..i]);
        match vars.get(name) {
            Some(v) => out.push_str(v),
            None if quiet => {}
            None => out.push_str(&value[i..end]),
        }
        i = end;
        copied = end;
    }
    out.push_str(&value[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(format: &str, pairs: &[(&str, &str)]) -> String {
        let template = LogTemplate::parse(format);
        let values: Vec<&str> = template
            .names()
            .iter()
            .map(|n| pairs.iter().find(|(k, _)| k == n).map(|(_, v)| *v).unwrap())
            .collect();
        template.render(&values)
    }

    #[test]
    fn substitutes_tokens() {
        assert_eq!(
            render(
                "<ip> - - [<date>] \"GET /\" <code>",
                &[("ip", "1.2.3.4"), ("date", "D"), ("code", "200")]
            ),
            "1.2.3.4 - - [D] \"GET /\" 200"
        );
        assert_eq!(
            render("{\"src\":\"< src_ip >\"}", &[("src_ip", "x")]),
            "{\"src\":\"x\"}"
        );
        assert_eq!(render("<a-b>/<a_b>", &[("a-b", "1"), ("a_b", "2")]), "1/2");
    }

    #[test]
    fn repeated_tokens_share_one_name() {
        let template = LogTemplate::parse("<ip> <ip> <host>");
        assert_eq!(template.names(), ["ip", "host"]);
        assert_eq!(template.render(&["1", "h"]), "1 1 h");
    }

    #[test]
    fn non_token_brackets_stay_literal() {
        let template = LogTemplate::parse("<134>a < b <> <1x> \\<ip> 한글 <x");
        assert!(template.names().is_empty());
        assert_eq!(template.render::<&str>(&[]), "<134>a < b <> <1x> <ip> 한글 <x");
    }

    #[test]
    fn missing_values_keep_tokens() {
        let template = LogTemplate::parse("<a>-<b>");
        assert_eq!(template.render_with(|i| (i == 0).then_some("A")), "A-<b>");
    }

    #[test]
    fn renders_velocity_style_references() {
        let vars: IndexMap<String, String> = [("src_ip", "10.0.0.1"), ("user", "kim")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        assert_eq!(render_references("${src_ip}", &vars), "10.0.0.1");
        assert_eq!(render_references("$user@$src_ip:80", &vars), "kim@10.0.0.1:80");
        assert_eq!(
            render_references("$missing ${missing} $!missing $!{missing}.", &vars),
            "$missing ${missing}  ."
        );
        assert_eq!(
            render_references("cost $5 and $ {user} #if", &vars),
            "cost $5 and $ {user} #if"
        );
        assert_eq!(render_references("${user", &vars), "${user");
    }
}
