//! Random strings matching a regular expression.
//!
//! Generated log values are kept readable, like the Java edition's RgxGen:
//! `\w`, `\d` and `\s` are ASCII (also inside brackets, e.g. `[\w.-]`); `.` and
//! negated classes produce printable ASCII; control characters other than tab
//! are never generated from classes; anchors and word boundaries are ignored.
//! Explicit ranges such as `[가-힣]` are kept as written.

use regex_syntax::ast::{self, Ast, ClassAscii, ClassAsciiKind, ClassPerl, ClassPerlKind, ClassSet, ClassSetItem};
use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, Hir, HirKind};

/// Upper bound for unbounded repetitions (`*`, `+`, `{n,}`).
const MAX_REPEAT: u32 = 100;

pub fn compile(pattern: &str) -> Result<rand_regex::Regex, String> {
    let mut ast = ast::parse::Parser::new().parse(pattern).map_err(|e| e.to_string())?;
    asciify(&mut ast);
    let hir = regex_syntax::hir::translate::Translator::new()
        .translate(pattern, &ast)
        .map_err(|e| e.to_string())?;
    rand_regex::Regex::with_hir(narrow(hir), MAX_REPEAT).map_err(|e| e.to_string())
}

/// `\d`/`\w`/`\s` as the equivalent ASCII class (`[[:digit:]]`, ...).
fn ascii_class(perl: &ClassPerl) -> ClassAscii {
    ClassAscii {
        span: perl.span,
        kind: match perl.kind {
            ClassPerlKind::Digit => ClassAsciiKind::Digit,
            ClassPerlKind::Space => ClassAsciiKind::Space,
            ClassPerlKind::Word => ClassAsciiKind::Word,
        },
        negated: perl.negated,
    }
}

fn asciify(ast: &mut Ast) {
    match ast {
        Ast::ClassPerl(perl) => {
            let item = ClassSetItem::Ascii(ascii_class(perl));
            *ast = Ast::class_bracketed(ast::ClassBracketed {
                span: perl.span,
                negated: false,
                kind: ClassSet::Item(item),
            });
        }
        Ast::ClassBracketed(class) => asciify_set(&mut class.kind),
        Ast::Repetition(repetition) => asciify(&mut repetition.ast),
        Ast::Group(group) => asciify(&mut group.ast),
        Ast::Alternation(alternation) => alternation.asts.iter_mut().for_each(asciify),
        Ast::Concat(concat) => concat.asts.iter_mut().for_each(asciify),
        _ => {}
    }
}

fn asciify_set(set: &mut ClassSet) {
    match set {
        ClassSet::Item(item) => asciify_item(item),
        ClassSet::BinaryOp(op) => {
            asciify_set(&mut op.lhs);
            asciify_set(&mut op.rhs);
        }
    }
}

fn asciify_item(item: &mut ClassSetItem) {
    match item {
        ClassSetItem::Perl(perl) => *item = ClassSetItem::Ascii(ascii_class(perl)),
        ClassSetItem::Bracketed(class) => asciify_set(&mut class.kind),
        ClassSetItem::Union(union) => union.items.iter_mut().for_each(asciify_item),
        _ => {}
    }
}

fn ascii(ranges: &[(char, char)]) -> ClassUnicode {
    ClassUnicode::new(ranges.iter().map(|&(a, b)| ClassUnicodeRange::new(a, b)))
}

/// Narrows one class; keeps it unchanged if narrowing would leave it empty.
fn narrow_class(class: ClassUnicode) -> ClassUnicode {
    let mut narrowed = class.clone();
    // Classes reaching the last code point come from `.` or negation.
    if class.ranges().last().is_some_and(|r| r.end() == char::MAX) {
        narrowed.intersect(&ascii(&[(' ', '~')]));
    }
    narrowed.difference(&ascii(&[('\0', '\x08'), ('\n', '\x1f'), ('\x7f', '\x7f')]));
    if narrowed.ranges().is_empty() { class } else { narrowed }
}

fn narrow(hir: Hir) -> Hir {
    match hir.into_kind() {
        HirKind::Class(Class::Unicode(class)) => Hir::class(Class::Unicode(narrow_class(class))),
        HirKind::Class(class) => Hir::class(class),
        HirKind::Look(_) | HirKind::Empty => Hir::empty(),
        HirKind::Literal(literal) => Hir::literal(literal.0),
        HirKind::Repetition(mut repetition) => {
            repetition.sub = Box::new(narrow(*repetition.sub));
            Hir::repetition(repetition)
        }
        HirKind::Capture(mut capture) => {
            capture.sub = Box::new(narrow(*capture.sub));
            Hir::capture(capture)
        }
        HirKind::Concat(subs) => Hir::concat(subs.into_iter().map(narrow).collect()),
        HirKind::Alternation(subs) => Hir::alternation(subs.into_iter().map(narrow).collect()),
    }
}

#[cfg(test)]
mod tests {
    use rand::RngExt;

    use super::*;

    fn samples(pattern: &str) -> Vec<String> {
        let regex = compile(pattern).unwrap();
        let mut rng = rand::rng();
        (0..200).map(|_| rng.sample::<String, _>(&regex)).collect()
    }

    fn assert_all(pattern: &str, check: impl Fn(&str) -> bool) {
        for s in samples(pattern) {
            assert!(check(&s), "{pattern} produced {s:?}");
        }
    }

    fn printable(s: &str) -> bool {
        s.bytes().all(|b| (0x20..=0x7e).contains(&b))
    }

    fn digits3(s: &str) -> bool {
        s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())
    }

    #[test]
    fn shorthand_classes_are_ascii() {
        assert_all(r"\d{3}", digits3);
        assert_all(r"\w{5}", |s| {
            s.len() == 5 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
        assert_all(r".{20}", printable);
        assert_all(r"[^a-z]{20}", |s| {
            printable(s) && !s.bytes().any(|b| b.is_ascii_lowercase())
        });
        assert_all(r"\S\W\D.*", printable);
        assert_all(r"\s", |s| s == " " || s == "\t");
    }

    #[test]
    fn shorthand_classes_inside_brackets_are_ascii() {
        assert_all(r"[\w.-]{30}", |s| {
            s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        });
        assert_all(r"[\d.]{30}", |s| s.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
        assert_all(r"[^\d]{30}", |s| printable(s) && !s.bytes().any(|b| b.is_ascii_digit()));
        assert_all(r"[\s,]{30}", |s| s.bytes().all(|b| b"\t ,".contains(&b)));
        assert_all(r"[a-z&&[^aeiou]]{30}", |s| {
            s.bytes().all(|b| b.is_ascii_lowercase() && !b"aeiou".contains(&b))
        });
    }

    #[test]
    fn anchors_are_ignored() {
        assert_all(r"^\d{3}$", digits3);
        assert_all(r"\b\d{3}\b", digits3);
    }

    #[test]
    fn generates_structured_values() {
        assert_all(r"(\d{1,3}\.){3}\d{1,3}", |s| {
            let parts: Vec<_> = s.split('.').collect();
            parts.len() == 4
                && parts
                    .iter()
                    .all(|p| (1..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        });
        assert_all("(GET|POST) /api/v[12]", |s| {
            ["GET /api/v1", "GET /api/v2", "POST /api/v1", "POST /api/v2"].contains(&s)
        });
    }

    #[test]
    fn keeps_explicit_unicode_ranges() {
        assert_all("[가-힣]{2}", |s| {
            s.chars().count() == 2 && s.chars().all(|c| ('가'..='힣').contains(&c))
        });
    }

    #[test]
    fn rejects_invalid_patterns() {
        assert!(compile("(unclosed").is_err());
    }
}
