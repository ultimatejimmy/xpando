//! Rich text (`html:`) support: a sanitiser for the HTML subset xpando writes, plain-text
//! conversion, and format-preserving edits of an entry's `html:` body.
//!
//! See `docs/xpando/RICHTEXT.md` for why this subset was chosen. In short, espanso pastes the
//! `html:` value verbatim through the clipboard, so the value must be small, predictable HTML.
//!
//! The editing functions work on the raw file text like [`crate::edit`]: only the lines of
//! the body field are rewritten, everything else stays byte for byte, and the result is
//! re-parsed and compared with the original before it is returned. The few line helpers here
//! mirror the private ones in `edit.rs` (which this module must not modify); they can be
//! merged once those are made `pub(crate)`.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_norway::Value;

use super::{make_preview, parse_match_text, scan_entries, EntrySpan, Phrase};

/// Prefix of the error returned when a conversion would lose formatting and the caller did
/// not allow it. The rest of the message lists what would be lost.
pub const LOSSY_PREFIX: &str = "lossy:";

const UNSUPPORTED_FILE: &str = "This file's layout is not one xpando can edit safely (for \
    example a flow-style list). Edit it in a text editor.";
const UNSUPPORTED_ENTRY: &str = "This entry's layout is not one xpando can edit safely. \
    Edit it in a text editor.";

// ---------------------------------------------------------------------------------------
// HTML parsing (tolerant, no external crate)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Elem(Elem),
}

#[derive(Debug, Clone)]
struct Elem {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Elem {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

const VOID: &[&str] = &[
    "br", "hr", "img", "input", "meta", "link", "area", "base", "col", "embed", "source", "track",
    "wbr", "param",
];

/// Elements whose content is not text at all: dropped together with their content.
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title", "noscript"];

/// Elements dropped with their content (they have no text worth keeping).
const DROP_WITH_CONTENT: &[&str] = &[
    "head", "iframe", "object", "embed", "template", "svg", "math", "select", "option", "button",
    "input", "video", "audio", "canvas", "frame", "frameset", "applet", "datalist", "dialog",
    "map",
];

const MAX_DEPTH: usize = 64;
const MAX_INPUT: usize = 4 * 1024 * 1024;

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "trade" => '\u{2122}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "bull" => '\u{2022}',
        "middot" => '\u{b7}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "deg" => '\u{b0}',
        "times" => '\u{d7}',
        "euro" => '\u{20ac}',
        "pound" => '\u{a3}',
        "yen" => '\u{a5}',
        "cent" => '\u{a2}',
        "sect" => '\u{a7}',
        "para" => '\u{b6}',
        "larr" => '\u{2190}',
        "rarr" => '\u{2192}',
        _ => return None,
    })
}

/// Decodes character references. Unknown ones are kept as literal text.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let semi = rest
            .char_indices()
            .take(12)
            .find(|&(_, c)| c == ';')
            .map(|(i, _)| i);
        let decoded = semi.and_then(|end| {
            let name = &rest[1..end];
            let ch = if let Some(num) = name.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                // Invalid or control references decode to nothing instead of an error.
                Some(
                    code.and_then(char::from_u32)
                        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r')),
                )
            } else {
                named_entity(name).map(Some)
            };
            ch.map(|c| (c, end + 1))
        });
        match decoded {
            Some((c, used)) => {
                if let Some(c) = c {
                    out.push(c);
                }
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b':' || c == b'_'
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn find(&self, pat: &str) -> Option<usize> {
        self.s[self.i..].find(pat).map(|p| self.i + p)
    }

    /// Parses a start tag at `self.i` (pointing at `<`). Returns `None` if it never ends.
    fn start_tag(&mut self) -> Option<(String, Vec<(String, String)>)> {
        let b = self.b;
        let mut i = self.i + 1;
        let ns = i;
        while i < b.len() && is_name_char(b[i]) {
            i += 1;
        }
        let name = self.s[ns..i].to_ascii_lowercase();
        let mut attrs = Vec::new();
        loop {
            while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
                i += 1;
            }
            if i >= b.len() {
                return None;
            }
            if b[i] == b'>' {
                self.i = i + 1;
                return Some((name, attrs));
            }
            let as_ = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/')
            {
                i += 1;
            }
            let key = self.s[as_..i].to_ascii_lowercase();
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut value = String::new();
            if i < b.len() && b[i] == b'=' {
                i += 1;
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                    let q = b[i];
                    let vs = i + 1;
                    let end = b[vs..].iter().position(|&c| c == q)? + vs;
                    value = decode_entities(&self.s[vs..end]);
                    i = end + 1;
                } else {
                    let vs = i;
                    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                        i += 1;
                    }
                    value = decode_entities(&self.s[vs..i]);
                }
            }
            if i == as_ {
                i += 1; // stray character, make progress
            } else if !key.is_empty() && !attrs.iter().any(|(k, _): &(String, String)| *k == key) {
                attrs.push((key, value));
            }
        }
    }
}

fn parse_html(input: &str) -> (Vec<Node>, bool, Vec<String>) {
    let mut dropped: Vec<String> = Vec::new();
    let truncated = input.len() > MAX_INPUT;
    let mut end = input.len().min(MAX_INPUT);
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    let s = &input[..end];
    let mut p = Parser {
        s,
        b: s.as_bytes(),
        i: 0,
    };
    let mut stack: Vec<Elem> = vec![Elem {
        name: String::new(),
        attrs: Vec::new(),
        children: Vec::new(),
    }];

    fn close_top(stack: &mut Vec<Elem>) {
        if stack.len() > 1 {
            let e = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(Node::Elem(e));
        }
    }
    fn push_text(stack: &mut [Elem], t: &str) {
        if t.is_empty() {
            return;
        }
        let top = stack.last_mut().unwrap();
        let t = decode_entities(t);
        if let Some(Node::Text(prev)) = top.children.last_mut() {
            prev.push_str(&t);
        } else {
            top.children.push(Node::Text(t));
        }
    }

    while p.i < p.b.len() {
        if p.b[p.i] != b'<' {
            let next = p.find("<").unwrap_or(p.b.len());
            push_text(&mut stack, &s[p.i..next]);
            p.i = next;
            continue;
        }
        let nxt = p.b.get(p.i + 1).copied().unwrap_or(0);
        if s[p.i..].starts_with("<!--") {
            p.i = p.find("-->").map_or(p.b.len(), |e| e + 3);
        } else if nxt == b'!' || nxt == b'?' {
            p.i = p.find(">").map_or(p.b.len(), |e| e + 1);
        } else if nxt == b'/' && p.b.get(p.i + 2).is_some_and(u8::is_ascii_alphabetic) {
            let mut j = p.i + 2;
            while j < p.b.len() && is_name_char(p.b[j]) {
                j += 1;
            }
            let name = s[p.i + 2..j].to_ascii_lowercase();
            p.i = s[j..].find('>').map_or(p.b.len(), |e| j + e + 1);
            if let Some(pos) = stack.iter().rposition(|e| e.name == name) {
                if pos > 0 {
                    while stack.len() > pos {
                        close_top(&mut stack);
                    }
                }
            }
        } else if nxt.is_ascii_alphabetic() {
            let Some((name, attrs)) = p.start_tag() else {
                break; // unterminated tag: drop the rest
            };
            if RAW_TEXT.contains(&name.as_str()) {
                let close = format!("</{name}");
                let lower = s[p.i..].to_ascii_lowercase();
                p.i = match lower.find(&close) {
                    Some(e) => s[p.i + e..]
                        .find('>')
                        .map_or(p.b.len(), |g| p.i + e + g + 1),
                    None => p.b.len(),
                };
                // Remember something was dropped so the UI can tell the user.
                if !dropped.contains(&name) {
                    dropped.push(name);
                }
                continue;
            }
            let top_name = stack.last().unwrap().name.clone();
            let block_start = matches!(
                name.as_str(),
                "p" | "div"
                    | "ul"
                    | "ol"
                    | "li"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "blockquote"
                    | "pre"
                    | "hr"
                    | "table"
                    | "section"
                    | "article"
            );
            if (top_name == "p" && block_start) || (name == "li" && top_name == "li") {
                close_top(&mut stack);
            }
            let elem = Elem {
                name: name.clone(),
                attrs,
                children: Vec::new(),
            };
            if VOID.contains(&name.as_str()) {
                stack.last_mut().unwrap().children.push(Node::Elem(elem));
            } else if stack.len() < MAX_DEPTH {
                stack.push(elem);
            }
        } else {
            push_text(&mut stack, "<");
            p.i += 1;
        }
    }
    while stack.len() > 1 {
        close_top(&mut stack);
    }
    let root = stack.pop().unwrap();
    (root.children, truncated, dropped)
}

// ---------------------------------------------------------------------------------------
// Document model
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Marks {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunKind {
    Text(String),
    Br,
    Img {
        src: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    kind: RunKind,
    marks: Marks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    Para(Vec<Run>),
    Heading(u8, Vec<Run>),
    Quote(Vec<Block>),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Code(String),
    Hr,
}

#[derive(Default)]
struct Cx {
    dropped: Vec<String>,
}

impl Cx {
    fn note(&mut self, what: &str) {
        if !self.dropped.iter().any(|d| d == what) {
            self.dropped.push(what.to_string());
        }
    }
}

fn is_html_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Collapses HTML whitespace and removes characters that must never reach a file.
fn collapse(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut last_space = false;
    for c in t.chars() {
        let c = match c {
            c if is_html_space(c) => ' ',
            '\u{85}' | '\u{2028}' | '\u{2029}' => ' ',
            '\u{feff}' => continue,
            c if c.is_control() => continue,
            c => c,
        };
        if c == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(c);
    }
    out
}

fn clean_pre(t: &str) -> String {
    t.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' | '\t' => Some(c),
            '\r' | '\u{85}' | '\u{2028}' | '\u{2029}' => Some('\n'),
            '\u{feff}' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

fn raw_text(nodes: &[Node], out: &mut String) {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Elem(e) if e.name == "br" => out.push('\n'),
            Node::Elem(e) => {
                raw_text(&e.children, out);
                if matches!(e.name.as_str(), "div" | "p") && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
}

/// A link address xpando keeps: `http`, `https`, `mailto` or `tel`. Whitespace and control
/// characters (which browsers ignore inside a scheme) are removed first.
pub fn safe_href(raw: &str) -> Option<String> {
    let href: String = raw
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect();
    let lower = href.to_ascii_lowercase();
    let ok = ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|p| lower.starts_with(p) && lower.len() > p.len());
    ok.then_some(href)
}

fn image_src(raw: &str) -> Option<String> {
    let src = safe_href(raw)?;
    let lower = src.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then_some(src)
}

fn dimension(v: Option<&str>) -> Option<u32> {
    let v = v?.trim();
    (!v.is_empty() && v.len() <= 4 && v.bytes().all(|b| b.is_ascii_digit()))
        .then(|| v.parse().ok())
        .flatten()
        .filter(|n| *n > 0)
}

fn tidy(runs: &mut Vec<Run>) {
    loop {
        let before = runs.clone();
        runs.retain(|r| !matches!(&r.kind, RunKind::Text(t) if t.is_empty()));
        for i in 0..runs.len() {
            let prev_ends_space =
                i > 0 && matches!(&runs[i - 1].kind, RunKind::Text(t) if t.ends_with(' '));
            let prev_is_br = i > 0 && runs[i - 1].kind == RunKind::Br;
            let at_start = i == 0;
            if let RunKind::Text(t) = &mut runs[i].kind {
                if (prev_ends_space || prev_is_br || at_start) && t.starts_with(' ') {
                    t.remove(0);
                }
            }
        }
        for i in 0..runs.len() {
            let next_is_br = runs.get(i + 1).is_some_and(|r| r.kind == RunKind::Br);
            let last = i + 1 == runs.len();
            if let RunKind::Text(t) = &mut runs[i].kind {
                if (next_is_br || last) && t.ends_with(' ') {
                    t.pop();
                }
            }
        }
        while runs.last().is_some_and(|r| r.kind == RunKind::Br) {
            runs.pop();
        }
        runs.retain(|r| !matches!(&r.kind, RunKind::Text(t) if t.is_empty()));
        if *runs == before {
            break;
        }
    }
}

fn flush(pend: &mut Vec<Run>, out: &mut Vec<Block>, force: bool) {
    let mut runs = std::mem::take(pend);
    tidy(&mut runs);
    if !runs.is_empty() || force {
        out.push(Block::Para(runs));
    }
}

fn build_blocks(nodes: &[Node], marks: &Marks, cx: &mut Cx) -> Vec<Block> {
    let mut out = Vec::new();
    let mut pend = Vec::new();
    collect(nodes, marks, cx, &mut out, &mut pend);
    flush(&mut pend, &mut out, false);
    out
}

fn inline_runs(nodes: &[Node], marks: &Marks, cx: &mut Cx) -> Vec<Run> {
    let blocks = build_blocks(nodes, marks, cx);
    let mut runs: Vec<Run> = Vec::new();
    for b in blocks {
        match b {
            Block::Para(r) | Block::Heading(_, r) => {
                if !runs.is_empty() {
                    runs.push(Run {
                        kind: RunKind::Br,
                        marks: Marks::default(),
                    });
                }
                runs.extend(r);
            }
            _ => cx.note("blocks nested inside headings"),
        }
    }
    tidy(&mut runs);
    runs
}

fn collect(nodes: &[Node], marks: &Marks, cx: &mut Cx, out: &mut Vec<Block>, pend: &mut Vec<Run>) {
    for n in nodes {
        match n {
            Node::Text(t) => {
                let t = collapse(t);
                if !t.is_empty() {
                    pend.push(Run {
                        kind: RunKind::Text(t),
                        marks: marks.clone(),
                    });
                }
            }
            Node::Elem(e) => collect_elem(e, marks, cx, out, pend),
        }
    }
}

fn collect_elem(e: &Elem, marks: &Marks, cx: &mut Cx, out: &mut Vec<Block>, pend: &mut Vec<Run>) {
    if e.attr("style").is_some_and(|s| !s.trim().is_empty()) || e.name == "font" {
        cx.note("colours, fonts and sizes");
    }
    let inline = |m: Marks, cx: &mut Cx, out: &mut Vec<Block>, pend: &mut Vec<Run>| {
        collect(&e.children, &m, cx, out, pend);
    };
    match e.name.as_str() {
        "strong" | "b" => inline(
            Marks {
                bold: true,
                ..marks.clone()
            },
            cx,
            out,
            pend,
        ),
        "em" | "i" => inline(
            Marks {
                italic: true,
                ..marks.clone()
            },
            cx,
            out,
            pend,
        ),
        "u" | "ins" => inline(
            Marks {
                underline: true,
                ..marks.clone()
            },
            cx,
            out,
            pend,
        ),
        "s" | "strike" | "del" => inline(
            Marks {
                strike: true,
                ..marks.clone()
            },
            cx,
            out,
            pend,
        ),
        "code" | "kbd" | "samp" | "tt" => inline(
            Marks {
                code: true,
                ..marks.clone()
            },
            cx,
            out,
            pend,
        ),
        "a" => match e.attr("href").and_then(safe_href) {
            Some(href) => inline(
                Marks {
                    link: marks.link.clone().or(Some(href)),
                    ..marks.clone()
                },
                cx,
                out,
                pend,
            ),
            None => {
                if e.attr("href").is_some() {
                    cx.note("links that are not http, https, mailto or tel");
                }
                inline(marks.clone(), cx, out, pend);
            }
        },
        "br" => pend.push(Run {
            kind: RunKind::Br,
            marks: Marks::default(),
        }),
        "img" => match e.attr("src").and_then(image_src) {
            Some(src) => pend.push(Run {
                kind: RunKind::Img {
                    src,
                    alt: collapse(e.attr("alt").unwrap_or("")),
                    width: dimension(e.attr("width")),
                    height: dimension(e.attr("height")),
                },
                marks: Marks {
                    link: marks.link.clone(),
                    ..Marks::default()
                },
            }),
            None => cx.note("images that are not at an http or https address"),
        },
        "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "aside" | "nav"
        | "figure" | "figcaption" | "address" | "details" | "summary" | "form" | "fieldset"
        | "center" | "td" | "th" | "dd" | "dt" | "dl" => {
            flush(pend, out, false);
            let inner = build_blocks(&e.children, marks, cx);
            if inner.is_empty() && e.name == "p" {
                out.push(Block::Para(Vec::new()));
            } else {
                out.extend(inner);
            }
        }
        "table" | "thead" | "tbody" | "tfoot" | "tr" | "caption" => {
            cx.note("tables (their cells became paragraphs)");
            flush(pend, out, false);
            let inner = build_blocks(&e.children, marks, cx);
            out.extend(inner);
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            flush(pend, out, false);
            let level = e.name.as_bytes()[1] - b'0';
            if level > 3 {
                cx.note("headings below level 3 (shown as level 3)");
            }
            let runs = inline_runs(&e.children, marks, cx);
            if runs.is_empty() {
                // an empty heading carries nothing worth keeping
            } else {
                out.push(Block::Heading(level.min(3), runs));
            }
        }
        "ul" | "ol" => {
            flush(pend, out, false);
            let mut items: Vec<Vec<Block>> = Vec::new();
            for c in &e.children {
                let item = match c {
                    Node::Elem(li) if li.name == "li" => build_blocks(&li.children, marks, cx),
                    Node::Text(t) if collapse(t).trim().is_empty() => continue,
                    other => build_blocks(std::slice::from_ref(other), marks, cx),
                };
                items.push(if item.is_empty() {
                    vec![Block::Para(Vec::new())]
                } else {
                    item
                });
            }
            if !items.is_empty() {
                out.push(Block::List {
                    ordered: e.name == "ol",
                    items,
                });
            }
        }
        "li" => {
            // A stray list item outside a list: keep it as a paragraph.
            flush(pend, out, false);
            out.extend(build_blocks(&e.children, marks, cx));
        }
        "blockquote" => {
            flush(pend, out, false);
            let inner = build_blocks(&e.children, marks, cx);
            let empty = |b: &Block| matches!(b, Block::Para(r) if r.is_empty());
            if !inner.iter().all(empty) {
                out.push(Block::Quote(inner));
            }
        }
        "pre" => {
            flush(pend, out, false);
            let mut raw = String::new();
            raw_text(&e.children, &mut raw);
            let text = clean_pre(&raw).trim_matches('\n').to_string();
            if !text.is_empty() {
                out.push(Block::Code(text));
            }
        }
        "hr" => {
            flush(pend, out, false);
            out.push(Block::Hr);
        }
        name if DROP_WITH_CONTENT.contains(&name) => cx.note(&format!("<{name}> content")),
        _ => inline(marks.clone(), cx, out, pend),
    }
}

fn parse_blocks(input: &str, cx: &mut Cx) -> Vec<Block> {
    let (nodes, truncated, dropped) = parse_html(input);
    for name in dropped {
        cx.note(&format!("<{name}> content"));
    }
    if truncated {
        cx.note("text beyond 4 MB");
    }
    let mut blocks = build_blocks(&nodes, &Marks::default(), cx);
    while blocks.len() > 1 && matches!(blocks.last(), Some(Block::Para(r)) if r.is_empty()) {
        blocks.pop();
    }
    if matches!(blocks.as_slice(), [Block::Para(r)] if r.is_empty()) {
        blocks.clear();
    }
    blocks
}

// ---------------------------------------------------------------------------------------
// Serialising
// ---------------------------------------------------------------------------------------

fn esc_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
    out
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

const TAGS: [&str; 6] = ["a", "strong", "em", "u", "s", "code"];

fn mark_key(m: &Marks, level: usize) -> Option<&str> {
    match level {
        0 => m.link.as_deref(),
        1 => m.bold.then_some(""),
        2 => m.italic.then_some(""),
        3 => m.underline.then_some(""),
        4 => m.strike.then_some(""),
        _ => m.code.then_some(""),
    }
}

fn emit_runs(runs: &[Run], level: usize) -> String {
    if level == TAGS.len() {
        let mut out = String::new();
        for r in runs {
            match &r.kind {
                RunKind::Text(t) => out.push_str(&esc_text(t)),
                RunKind::Br => out.push_str("<br>"),
                RunKind::Img {
                    src,
                    alt,
                    width,
                    height,
                } => {
                    out.push_str(&format!("<img src=\"{}\"", esc_attr(src)));
                    if !alt.is_empty() {
                        out.push_str(&format!(" alt=\"{}\"", esc_attr(alt)));
                    }
                    if let Some(w) = width {
                        out.push_str(&format!(" width=\"{w}\""));
                    }
                    if let Some(h) = height {
                        out.push_str(&format!(" height=\"{h}\""));
                    }
                    out.push('>');
                }
            }
        }
        return out;
    }
    let mut out = String::new();
    let mut i = 0;
    while i < runs.len() {
        let key = mark_key(&runs[i].marks, level);
        let mut j = i + 1;
        while j < runs.len() && mark_key(&runs[j].marks, level) == key {
            j += 1;
        }
        let inner = emit_runs(&runs[i..j], level + 1);
        match key {
            None => out.push_str(&inner),
            Some(href) if level == 0 => {
                out.push_str(&format!("<a href=\"{}\">{inner}</a>", esc_attr(href)));
            }
            Some(_) => out.push_str(&format!("<{0}>{inner}</{0}>", TAGS[level])),
        }
        i = j;
    }
    out
}

/// Content of a container that holds one paragraph: written without the `<p>` when possible.
fn emit_container(blocks: &[Block]) -> String {
    match blocks {
        [Block::Para(r)] if !r.is_empty() => emit_runs(r, 0),
        [Block::Para(r), rest @ ..] if !r.is_empty() && rest.iter().all(is_list) => {
            format!("{}{}", emit_runs(r, 0), emit_all(rest, ""))
        }
        [Block::Para(_)] => String::new(),
        _ => emit_all(blocks, ""),
    }
}

fn is_list(b: &Block) -> bool {
    matches!(b, Block::List { .. })
}

fn emit_block(b: &Block) -> String {
    match b {
        Block::Para(r) if r.is_empty() => "<p><br></p>".to_string(),
        Block::Para(r) => format!("<p>{}</p>", emit_runs(r, 0)),
        Block::Heading(n, r) => format!("<h{n}>{}</h{n}>", emit_runs(r, 0)),
        Block::Quote(inner) => format!("<blockquote>{}</blockquote>", emit_container(inner)),
        Block::List { ordered, items } => {
            let tag = if *ordered { "ol" } else { "ul" };
            let lis: String = items
                .iter()
                .map(|i| format!("<li>{}</li>", emit_container(i)))
                .collect();
            format!("<{tag}>{lis}</{tag}>")
        }
        Block::Code(t) => format!("<pre><code>{}</code></pre>", esc_text(t)),
        Block::Hr => "<hr>".to_string(),
    }
}

fn emit_all(blocks: &[Block], sep: &str) -> String {
    blocks.iter().map(emit_block).collect::<Vec<_>>().join(sep)
}

fn emit_document(blocks: &[Block]) -> String {
    match blocks {
        [] => String::new(),
        [Block::Para(r)] => emit_runs(r, 0),
        _ => emit_all(blocks, "\n"),
    }
}

// ---------------------------------------------------------------------------------------
// Public sanitiser and text conversion
// ---------------------------------------------------------------------------------------

/// Result of [`sanitize_html`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sanitized {
    /// Canonical HTML in the allowed subset.
    pub html: String,
    /// What was removed or simplified, as short phrases for the UI.
    pub dropped: Vec<String>,
}

/// Reduces `input` to the HTML subset xpando writes: `p`, `br`, `strong`, `em`, `u`, `s`,
/// `code`, `a[href]`, `ul`/`ol`/`li`, `h1`-`h3`, `blockquote`, `pre>code`, `hr` and
/// `img[src|alt|width|height]`. Scripts, styles, event handlers, unsafe link schemes and
/// everything else are dropped. The output is canonical: sanitising it again changes nothing.
///
/// A body that is one paragraph is written without the `<p>` wrapper, because espanso pastes
/// the value as is and a wrapper adds a line break in many applications. `{{placeholders}}`
/// are ordinary text and pass through untouched.
pub fn sanitize_html(input: &str) -> Sanitized {
    let mut cx = Cx::default();
    let blocks = parse_blocks(input, &mut cx);
    Sanitized {
        html: emit_document(&blocks),
        dropped: cx.dropped,
    }
}

fn runs_text(runs: &[Run]) -> String {
    let mut out = String::new();
    for r in runs {
        match &r.kind {
            RunKind::Text(t) => out.push_str(&t.replace('\u{a0}', " ")),
            RunKind::Br => out.push('\n'),
            RunKind::Img { alt, .. } => out.push_str(alt),
        }
    }
    out
}

fn prefix_lines(text: &str, first: &str, rest: &str) -> String {
    text.split('\n')
        .enumerate()
        .map(|(i, l)| {
            let p = if i == 0 { first } else { rest };
            if l.is_empty() {
                p.trim_end().to_string()
            } else {
                format!("{p}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn blocks_text(blocks: &[Block], sep: &str) -> String {
    blocks
        .iter()
        .map(|b| match b {
            Block::Para(r) | Block::Heading(_, r) => runs_text(r),
            Block::Quote(inner) => prefix_lines(&blocks_text(inner, "\n\n"), "> ", "> "),
            Block::List { ordered, items } => items
                .iter()
                .enumerate()
                .map(|(n, item)| {
                    let marker = if *ordered {
                        format!("{}. ", n + 1)
                    } else {
                        "- ".to_string()
                    };
                    let pad = " ".repeat(marker.len());
                    prefix_lines(&blocks_text(item, "\n"), &marker, &pad)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Block::Code(t) => t.clone(),
            Block::Hr => "---".to_string(),
        })
        .collect::<Vec<_>>()
        .join(sep)
}

fn features(blocks: &[Block], out: &mut BTreeSet<&'static str>) {
    let runs = |rs: &[Run], out: &mut BTreeSet<&'static str>| {
        for r in rs {
            if r.marks.bold {
                out.insert("bold");
            }
            if r.marks.italic {
                out.insert("italic");
            }
            if r.marks.underline {
                out.insert("underline");
            }
            if r.marks.strike {
                out.insert("strikethrough");
            }
            if r.marks.code {
                out.insert("code style");
            }
            if r.marks.link.is_some() {
                out.insert("links");
            }
            if matches!(r.kind, RunKind::Img { .. }) {
                out.insert("images");
            }
        }
    };
    for b in blocks {
        match b {
            Block::Para(r) => runs(r, out),
            Block::Heading(_, r) => {
                out.insert("headings");
                runs(r, out);
            }
            Block::Quote(inner) => {
                out.insert("quotes");
                features(inner, out);
            }
            Block::List { items, .. } => {
                out.insert("lists");
                for i in items {
                    features(i, out);
                }
            }
            Block::Code(_) => {
                out.insert("code blocks");
            }
            Block::Hr => {
                out.insert("horizontal rules");
            }
        }
    }
}

/// A plain-text version of `html`: what a text-only target would receive. Paragraphs are
/// separated by a blank line, `<br>` is a line break, lists get `- ` or `1. ` markers.
/// Mirrors what espanso's own fallback (html2text) produces closely enough for previews.
pub fn html_to_text(html: &str) -> String {
    let mut cx = Cx::default();
    blocks_text(&parse_blocks(html, &mut cx), "\n\n")
}

/// Result of [`html_to_plain`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlainConversion {
    pub text: String,
    /// What the conversion throws away (formatting, links, ...); empty if nothing.
    pub lost: Vec<String>,
}

/// Converts an `html:` body to the text of a plain `replace:` body and lists what is lost.
pub fn html_to_plain(html: &str) -> PlainConversion {
    let mut cx = Cx::default();
    let blocks = parse_blocks(html, &mut cx);
    let mut f = BTreeSet::new();
    features(&blocks, &mut f);
    let mut lost: Vec<String> = f.into_iter().map(String::from).collect();
    lost.extend(cx.dropped);
    PlainConversion {
        text: blocks_text(&blocks, "\n\n"),
        lost,
    }
}

/// Result of [`plain_to_html`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HtmlConversion {
    pub html: String,
    /// What the conversion changes (tabs, trailing line breaks, ...); empty if nothing.
    pub lost: Vec<String>,
}

/// Converts the text of a `replace:` body to an `html:` body. A blank line starts a new
/// paragraph, a single line break becomes `<br>`, and spaces that HTML would collapse are
/// kept as non-breaking spaces. `{{placeholders}}` are kept as they are.
pub fn plain_to_html(text: &str) -> Result<HtmlConversion, String> {
    if text.chars().any(|c| {
        (c.is_control() && !matches!(c, '\n' | '\t' | '\r')) || matches!(c, '\u{feff}' | '\u{85}')
    }) {
        return Err("The text contains control characters that cannot be written as HTML.".into());
    }
    let norm = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lost: Vec<String> = Vec::new();
    if norm.contains('\t') {
        lost.push("tabs (written as four spaces)".into());
    }
    if norm.starts_with('\n') || norm.trim_end_matches([' ', '\t']).ends_with('\n') {
        lost.push("leading or trailing line breaks".into());
    }
    let mut paras: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut blank_run = 0;
    for line in norm.split('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            continue;
        }
        if blank_run > 0 && !cur.is_empty() {
            if blank_run > 1 {
                lost.push("runs of several blank lines (shown as one)".into());
            }
            paras.push(std::mem::take(&mut cur));
        }
        blank_run = 0;
        cur.push(line.replace('\t', "    "));
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    lost.dedup();

    let blocks: Vec<Block> = paras
        .into_iter()
        .map(|lines| {
            let mut runs = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    runs.push(Run {
                        kind: RunKind::Br,
                        marks: Marks::default(),
                    });
                }
                runs.push(Run {
                    kind: RunKind::Text(keep_spaces(line)),
                    marks: Marks::default(),
                });
            }
            Block::Para(runs)
        })
        .collect();
    let html = emit_document(&blocks);
    Ok(HtmlConversion { html, lost })
}

/// Spaces HTML would drop (at the ends of a line or next to another space) become U+00A0.
fn keep_spaces(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            let edge = i == 0 || i + 1 == chars.len();
            let doubled = chars.get(i + 1) == Some(&' ') || (i > 0 && chars[i - 1] == ' ');
            if edge || doubled {
                out.push('\u{a0}');
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Plain-text preview of an `html:` body for lists (whitespace collapsed, shortened).
pub fn html_preview(html: &str) -> String {
    make_preview(&html_to_text(html))
}

// ---------------------------------------------------------------------------------------
// Line helpers (mirror edit.rs; see module docs)
// ---------------------------------------------------------------------------------------

fn text_of(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

fn indent(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

fn split_lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(String::from).collect()
}

fn detect_eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

fn finish(mut lines: Vec<String>, eol: &str, ends_nl: bool) -> String {
    let n = lines.len();
    for (i, l) in lines.iter_mut().enumerate() {
        if i + 1 < n && !l.ends_with('\n') {
            l.push_str(eol);
        }
    }
    let mut out = lines.concat();
    if ends_nl {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
    } else if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

/// A YAML double-quoted scalar for `s`.
fn dq(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if c.is_control() || c == '\u{feff}' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn block_safe(core: &str) -> bool {
    let first = core.chars().next();
    matches!(first, Some(c) if !c.is_whitespace())
        && !core.chars().any(|c| {
            (c.is_control() && c != '\n' && c != '\t')
                || matches!(c, '\u{feff}' | '\u{85}' | '\u{2028}' | '\u{2029}')
        })
}

/// Lines of `key: value`; continuation lines carry absolute indentation. A value with line
/// breaks becomes a `|-` block scalar when that reads back identically, otherwise a
/// double-quoted scalar with escapes.
fn render_value(key: &str, value: &str, k: usize, step: usize, eol: &str) -> Vec<String> {
    if value.contains('\n') && block_safe(value) && !value.ends_with('\n') {
        let pad = " ".repeat(k + step);
        let mut out = vec![format!("{key}: |-{eol}")];
        for line in value.split('\n') {
            if line.is_empty() {
                out.push(eol.to_string());
            } else {
                out.push(format!("{pad}{line}{eol}"));
            }
        }
        return out;
    }
    vec![format!("{key}: {}{eol}", dq(value))]
}

fn entry_count(text: &str) -> Result<usize, String> {
    let doc: Value =
        serde_norway::from_str(text).map_err(|e| format!("The file is not valid YAML: {e}"))?;
    match doc {
        Value::Null => Ok(0),
        Value::Mapping(m) => match m.get("matches") {
            None | Some(Value::Null) => Ok(0),
            Some(Value::Sequence(s)) => Ok(s.len()),
            Some(_) => Err("`matches` is not a list.".into()),
        },
        _ => Err("The file is not a YAML mapping.".into()),
    }
}

struct Field {
    key: String,
    own: Vec<String>,
    tail: Vec<String>,
}

struct EntryText {
    prefix: String,
    k: usize,
    fields: Vec<Field>,
}

fn field_key(rest: &str) -> Option<String> {
    let end = rest.find(':')?;
    let key = &rest[..end];
    let after = &rest[end + 1..];
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !key.starts_with(|c: char| c.is_ascii_digit() || c == '-');
    (valid && (after.is_empty() || after.starts_with([' ', '\t']))).then(|| key.to_string())
}

fn split_entry(lines: &[String]) -> Result<EntryText, String> {
    let first = text_of(&lines[0]);
    let d = indent(first);
    let after_indent = &first[d..];
    let rest = after_indent.strip_prefix('-').ok_or(UNSUPPORTED_ENTRY)?;
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let k = d + 1 + spaces;
    if first.len() <= k
        || first[k..].trim().is_empty()
        || first[k..].starts_with('#')
        || first[..d].contains('\t')
    {
        return Err(UNSUPPORTED_ENTRY.into());
    }
    let mut segs: Vec<(String, Vec<String>)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = text_of(l);
        let starts_field = if i == 0 {
            true
        } else {
            let tt = t.trim();
            let ind = indent(t);
            !tt.is_empty()
                && !tt.starts_with('#')
                && ind <= k
                && !(ind == k && (tt == "-" || tt.starts_with("- ")))
        };
        if starts_field {
            if i > 0 && (indent(t) != k || t[..k].contains('\t')) {
                return Err(UNSUPPORTED_ENTRY.into());
            }
            let key = field_key(&t[k..]).ok_or(UNSUPPORTED_ENTRY)?;
            segs.push((key, vec![l[k..].to_string()]));
        } else {
            segs.last_mut().ok_or(UNSUPPORTED_ENTRY)?.1.push(l.clone());
        }
    }
    let fields = segs
        .into_iter()
        .map(|(key, mut own)| {
            let mut tail = Vec::new();
            while own.len() > 1 {
                let t = text_of(own.last().unwrap());
                let tt = t.trim();
                if tt.is_empty() || (tt.starts_with('#') && indent(t) <= k) {
                    tail.push(own.pop().unwrap());
                } else {
                    break;
                }
            }
            tail.reverse();
            Field { key, own, tail }
        })
        .collect();
    Ok(EntryText {
        prefix: first[..k].to_string(),
        k,
        fields,
    })
}

impl EntryText {
    fn assemble(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut first = true;
        for f in &self.fields {
            for (i, l) in f.own.iter().enumerate() {
                if i == 0 {
                    let pre = if first {
                        self.prefix.clone()
                    } else {
                        " ".repeat(self.k)
                    };
                    out.push(format!("{pre}{l}"));
                    first = false;
                } else {
                    out.push(l.clone());
                }
            }
            out.extend(f.tail.iter().cloned());
        }
        out
    }

    fn step(&self) -> usize {
        self.fields
            .iter()
            .flat_map(|f| f.own.iter().skip(1))
            .map(|l| text_of(l))
            .filter(|t| !t.trim().is_empty() && indent(t) > self.k)
            .map(|t| indent(t) - self.k)
            .next()
            .unwrap_or(2)
            .clamp(1, 8)
    }
}

/// One entry of a file, split into lines and fields.
struct Located {
    lines: Vec<String>,
    eol: &'static str,
    ends_nl: bool,
    span: EntrySpan,
    body_end: usize,
    entry: EntryText,
}

const BODY_KEYS: [&str; 5] = ["replace", "html", "markdown", "image_path", "form"];

fn locate(text: &str, index: usize) -> Result<Located, String> {
    let count = entry_count(text)?;
    if index >= count {
        return Err(format!("There is no entry #{} in this file.", index + 1));
    }
    let spans = scan_entries(text)
        .filter(|s| s.len() == count)
        .ok_or(UNSUPPORTED_FILE)?;
    let lines = split_lines(text);
    let span = spans[index].clone();
    let d = indent(text_of(&lines[span.start_line]));
    let mut e = span.end_line;
    while e > span.start_line + 1 {
        let t = text_of(&lines[e - 1]);
        let tt = t.trim();
        if tt.is_empty() || (tt.starts_with('#') && indent(t) <= d) {
            e -= 1;
        } else {
            break;
        }
    }
    let entry = split_entry(&lines[span.start_line..e])?;
    Ok(Located {
        eol: detect_eol(text),
        ends_nl: text.is_empty() || text.ends_with('\n'),
        lines,
        span,
        body_end: e,
        entry,
    })
}

impl Located {
    /// Index of the single body field, which must be named `want`.
    fn body_field(&self, want: &str) -> Result<usize, String> {
        let pos: Vec<usize> = (0..self.entry.fields.len())
            .filter(|&i| BODY_KEYS.contains(&self.entry.fields[i].key.as_str()))
            .collect();
        match pos.as_slice() {
            [p] if self.entry.fields[*p].key == want => Ok(*p),
            [p] => Err(format!(
                "This phrase uses `{}`, not `{want}`.",
                self.entry.fields[*p].key
            )),
            _ => Err(UNSUPPORTED_ENTRY.into()),
        }
    }

    fn rebuild(&self) -> String {
        let mut out: Vec<String> = self.lines[..self.span.start_line].to_vec();
        out.extend(self.entry.assemble());
        out.extend(self.lines[self.body_end..].iter().cloned());
        finish(out, self.eol, self.ends_nl)
    }
}

// ---------------------------------------------------------------------------------------
// Safety net
// ---------------------------------------------------------------------------------------

fn without_line(p: &Phrase) -> Phrase {
    Phrase {
        line: None,
        ..p.clone()
    }
}

fn fail(what: &str) -> Result<(), String> {
    Err(format!(
        "Safety check failed ({what}); nothing was changed. Please report this."
    ))
}

type Pairs = Vec<(Value, Value)>;

fn entry_pairs(doc: &Value, index: usize) -> Option<(Pairs, Pairs)> {
    let map = doc.as_mapping()?;
    let others = map
        .iter()
        .filter(|(k, _)| k.as_str() != Some("matches"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let seq = map.get("matches")?.as_sequence()?;
    let entry = seq.get(index)?.as_mapping()?;
    Some((
        others,
        entry.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    ))
}

/// Re-parses `new` and checks that only the body field of entry `index` changed: it must now
/// be `new_key: expected`, and every other field, entry and top-level key must be identical.
fn verify_body(
    old: &str,
    new: &str,
    index: usize,
    old_key: &str,
    new_key: &str,
    expected: &str,
) -> Result<(), String> {
    let Ok(old_doc) = serde_norway::from_str::<Value>(old) else {
        return fail("the original is not valid YAML");
    };
    let Ok(new_doc) = serde_norway::from_str::<Value>(new) else {
        return fail("the result is not valid YAML");
    };
    let (Some((old_top, old_entry)), Some((new_top, new_entry))) =
        (entry_pairs(&old_doc, index), entry_pairs(&new_doc, index))
    else {
        return fail("the entry could not be compared");
    };
    if old_top != new_top {
        return fail("a top-level key changed");
    }
    let old_n = entry_count(old)?;
    if entry_count(new) != Ok(old_n) {
        return fail("entry count differs");
    }
    if scan_entries(new).map(|s| s.len()) != Some(old_n) {
        return fail("entry layout not recognised");
    }
    let strip = |pairs: &Pairs, key: &str| -> Pairs {
        pairs
            .iter()
            .filter(|(k, _)| k.as_str() != Some(key))
            .cloned()
            .collect()
    };
    if strip(&old_entry, old_key) != strip(&new_entry, new_key) {
        return fail("another field of the phrase changed");
    }
    let new_body = new_entry
        .iter()
        .find(|(k, _)| k.as_str() == Some(new_key))
        .map(|(_, v)| v);
    if new_body != Some(&Value::String(expected.to_string())) {
        return fail("the body differs from the request");
    }
    let (old_p, old_w) = parse_match_text(old);
    let (new_p, new_w) = parse_match_text(new);
    if old_w != new_w {
        return fail("warnings differ");
    }
    let rest = |ps: &[Phrase]| -> Vec<Phrase> {
        ps.iter()
            .filter(|p| p.index != index)
            .map(without_line)
            .collect()
    };
    if rest(&old_p) != rest(&new_p) {
        return fail("another phrase changed");
    }
    Ok(())
}

fn body_string(text: &str, index: usize, key: &str) -> Result<String, String> {
    let doc: Value = serde_norway::from_str(text).map_err(|e| e.to_string())?;
    doc.get("matches")
        .and_then(Value::as_sequence)
        .and_then(|s| s.get(index))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(key))
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| format!("The `{key}` value of this phrase is not text."))
}

// ---------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------

/// Replaces the `html:` body of entry `index` with `html` after sanitising it.
///
/// If the sanitised result means the same as the current body (both reduce to the same
/// canonical HTML) the input is returned unchanged, so merely opening and saving a phrase
/// never rewrites it. Only the body lines change; the rest of the file stays byte for byte.
pub fn set_html_body(text: &str, index: usize, html: &str) -> Result<String, String> {
    let loc = locate(text, index)?;
    let pos = loc.body_field("html")?;
    let old = body_string(text, index, "html")?;
    let new = sanitize_html(html).html;
    if new == sanitize_html(&old).html {
        return Ok(text.to_string());
    }
    let mut loc = loc;
    let (k, step, eol) = (loc.entry.k, loc.entry.step(), loc.eol);
    loc.entry.fields[pos].own = render_value("html", &new, k, step, eol);
    let new_text = loc.rebuild();
    verify_body(text, &new_text, index, "html", "html", &new)?;
    Ok(new_text)
}

fn lossy_error(lost: &[String], what: &str) -> String {
    format!("{LOSSY_PREFIX}{what} {}", lost.join(", "))
}

/// Turns the plain `replace:` body of entry `index` into an `html:` body (the key is renamed
/// and the text converted, see [`plain_to_html`]). Fails with a [`LOSSY_PREFIX`] error when
/// the text would change and `allow_lossy` is false.
pub fn convert_to_html(text: &str, index: usize, allow_lossy: bool) -> Result<String, String> {
    let mut loc = locate(text, index)?;
    let pos = loc.body_field("replace")?;
    let old = body_string(text, index, "replace")?;
    let conv = plain_to_html(&old)?;
    let mut lost = conv.lost.clone();
    let trailing = old.len() - old.trim_end_matches('\n').len();
    if trailing > 0 {
        lost.push("the line break at the end".into());
    }
    if !lost.is_empty() && !allow_lossy {
        return Err(lossy_error(&lost, "Converting to rich text changes:"));
    }
    let (k, step, eol) = (loc.entry.k, loc.entry.step(), loc.eol);
    loc.entry.fields[pos].key = "html".into();
    loc.entry.fields[pos].own = render_value("html", &conv.html, k, step, eol);
    let new_text = loc.rebuild();
    verify_body(text, &new_text, index, "replace", "html", &conv.html)?;
    Ok(new_text)
}

/// Turns the `html:` body of entry `index` into a plain `replace:` body. Fails with a
/// [`LOSSY_PREFIX`] error when formatting would be lost and `allow_lossy` is false.
pub fn convert_to_plain(text: &str, index: usize, allow_lossy: bool) -> Result<String, String> {
    let mut loc = locate(text, index)?;
    let pos = loc.body_field("html")?;
    let old = body_string(text, index, "html")?;
    let conv = html_to_plain(&old);
    if !conv.lost.is_empty() && !allow_lossy {
        return Err(lossy_error(&conv.lost, "Converting to plain text loses:"));
    }
    let (k, step, eol) = (loc.entry.k, loc.entry.step(), loc.eol);
    loc.entry.fields[pos].key = "replace".into();
    loc.entry.fields[pos].own = render_value("replace", &conv.text, k, step, eol);
    let new_text = loc.rebuild();
    verify_body(text, &new_text, index, "html", "replace", &conv.text)?;
    Ok(new_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(html: &str) -> String {
        sanitize_html(html).html
    }

    // ---- sanitiser ---------------------------------------------------------------

    #[test]
    fn plain_and_inline_bodies_stay_inline() {
        assert_eq!(s("hello"), "hello");
        assert_eq!(s("${<b>rs</b>}"), "${<strong>rs</strong>}");
        assert_eq!(s("<p>one paragraph</p>"), "one paragraph");
        assert_eq!(s(""), "");
        assert_eq!(s("   \n  "), "");
    }

    #[test]
    fn multiple_blocks_are_wrapped_and_separated_by_newlines() {
        assert_eq!(s("<p>a</p><p>b</p>"), "<p>a</p>\n<p>b</p>");
        assert_eq!(s("a<p>b</p>"), "<p>a</p>\n<p>b</p>");
        assert_eq!(s("<div>a</div><div>b</div>"), "<p>a</p>\n<p>b</p>");
    }

    #[test]
    fn tag_aliases_are_normalised() {
        assert_eq!(
            s("<b>b</b><i>i</i><strike>s</strike><del>d</del><ins>u</ins>"),
            "<strong>b</strong><em>i</em><s>sd</s><u>u</u>"
        );
    }

    #[test]
    fn mark_nesting_is_canonical() {
        let a = s("<em><strong>x</strong></em>");
        let b = s("<strong><em>x</em></strong>");
        assert_eq!(a, b);
        assert_eq!(a, "<strong><em>x</em></strong>");
        assert_eq!(
            s("<strong>a</strong><strong>b</strong>"),
            "<strong>ab</strong>"
        );
        assert_eq!(
            s("<a href=\"https://x.com\"><strong>a</strong> b</a>"),
            "<a href=\"https://x.com\"><strong>a</strong> b</a>"
        );
    }

    #[test]
    fn scripts_handlers_and_styles_are_dropped() {
        let r =
            sanitize_html("a<script>alert(1)</script>b<style>p{}</style><b onclick=\"x()\">c</b>");
        assert_eq!(r.html, "ab<strong>c</strong>");
        assert!(r.dropped.iter().any(|d| d.contains("script")));
        assert_eq!(
            s("<img src=x onerror=alert(1)>"),
            "",
            "relative image sources are dropped"
        );
        assert_eq!(s("<iframe src=\"https://e.com\">hi</iframe>x"), "x");
        assert_eq!(s("<svg><script>1</script></svg>y"), "y");
        assert_eq!(s("<p style=\"color:red\" onmouseover=\"x\">t</p>"), "t");
        assert_eq!(s("<!-- c --><!DOCTYPE html>t"), "t");
    }

    #[test]
    fn unsafe_links_are_unwrapped() {
        assert_eq!(s("<a href=\"javascript:alert(1)\">x</a>"), "x");
        assert_eq!(s("<a href=\" JaVa\tScRiPt:alert(1)\">x</a>"), "x");
        assert_eq!(s("<a href=\"data:text/html,<b>\">x</a>"), "x");
        assert_eq!(s("<a href=\"vbscript:1\">x</a>"), "x");
        assert_eq!(s("<a href=\"/relative\">x</a>"), "x");
        assert_eq!(s("<a href=\"{{url}}\">x</a>"), "x");
        assert_eq!(s("<a name=\"top\">x</a>"), "x");
        assert_eq!(
            s("<a href='HTTPS://a.com/?q=1&amp;r=\"2\"' target=\"_blank\" rel=\"x\">x</a>"),
            "<a href=\"HTTPS://a.com/?q=1&amp;r=&quot;2&quot;\">x</a>"
        );
        assert_eq!(
            s("<a href=\"mailto:a@b.com\">m</a><a href=\"tel:+1555\">t</a>"),
            "<a href=\"mailto:a@b.com\">m</a><a href=\"tel:+1555\">t</a>"
        );
        assert_eq!(s("<a href=\"https://\">x</a>"), "x");
    }

    #[test]
    fn link_with_placeholder_in_path_is_kept() {
        assert_eq!(
            s("<a href=\"https://x.com/{{id}}\">go</a>"),
            "<a href=\"https://x.com/{{id}}\">go</a>"
        );
    }

    #[test]
    fn images_keep_only_http_sources() {
        assert_eq!(
            s("<img src=\"https://x.com/a.png\" alt=\"A &amp; B\" width=\"10\" height=\"abc\" onerror=\"x\">"),
            "<img src=\"https://x.com/a.png\" alt=\"A &amp; B\" width=\"10\">"
        );
        assert_eq!(s("<img src=\"data:image/png;base64,AAAA\">"), "");
        assert_eq!(s("<img src=\"file:///c:/a.png\">"), "");
        assert_eq!(s("<img src=\"javascript:alert(1)\">"), "");
        let r = sanitize_html("<img src=\"data:image/png;base64,AAAA\">");
        assert!(!r.dropped.is_empty());
    }

    #[test]
    fn headings_lists_quotes_code_hr() {
        assert_eq!(
            s("<h1>T</h1><h2>U</h2><h4>V</h4>"),
            "<h1>T</h1>\n<h2>U</h2>\n<h3>V</h3>"
        );
        assert_eq!(
            s("<ul><li>a</li><li>b</li></ul>"),
            "<ul><li>a</li><li>b</li></ul>"
        );
        assert_eq!(
            s("<ol>\n  <li><p>a</p></li>\n  <li>b<ul><li>c</li></ul></li>\n</ol>"),
            "<ol><li>a</li><li>b<ul><li>c</li></ul></li></ol>"
        );
        assert_eq!(s("<ul><li>a<li>b</ul>"), "<ul><li>a</li><li>b</li></ul>");
        assert_eq!(
            s("<blockquote><p>q1</p><p>q2</p></blockquote>"),
            "<blockquote><p>q1</p><p>q2</p></blockquote>"
        );
        assert_eq!(
            s("<blockquote>q</blockquote>"),
            "<blockquote>q</blockquote>"
        );
        assert_eq!(
            s("<pre>\n  a &lt; b\n\tc</pre>"),
            "<pre><code>  a &lt; b\n\tc</code></pre>"
        );
        assert_eq!(
            s("<pre><code>x\ny</code></pre>"),
            "<pre><code>x\ny</code></pre>"
        );
        assert_eq!(s("a<hr>b"), "<p>a</p>\n<hr>\n<p>b</p>");
    }

    #[test]
    fn whitespace_collapses_like_a_browser() {
        assert_eq!(s("a   b\n\tc"), "a b c");
        assert_eq!(s("<b>a </b> b"), "<strong>a </strong>b");
        assert_eq!(s("a &nbsp;b"), "a &nbsp;b");
        assert_eq!(s("<p> a </p><p>b</p>"), "<p>a</p>\n<p>b</p>");
        assert_eq!(s("a<br> b"), "a<br>b");
        assert_eq!(s("a<br>"), "a");
        assert_eq!(s("<p><br></p><p>x</p>"), "<p><br></p>\n<p>x</p>");
        assert_eq!(s("<p>x</p><p></p><p><br></p>"), "x");
    }

    #[test]
    fn text_is_escaped_and_entities_decoded() {
        assert_eq!(s("a &lt;b&gt; &amp; c"), "a &lt;b&gt; &amp; c");
        assert_eq!(s("1 < 2 && 3 > 2"), "1 &lt; 2 &amp;&amp; 3 &gt; 2");
        assert_eq!(
            s("&copy; &#169; &#xA9; &unknown; &#0;"),
            "\u{a9} \u{a9} \u{a9} &amp;unknown;"
        );
        assert_eq!(s("\"quoted\" 'x'"), "\"quoted\" 'x'");
        assert_eq!(s("<<b>>x"), "&lt;<strong>&gt;x</strong>");
    }

    #[test]
    fn placeholders_pass_through_untouched() {
        assert_eq!(s("Hi {{name}}, {{ date }}!"), "Hi {{name}}, {{ date }}!");
        assert_eq!(
            s("<p><b>{{form1.name}}</b></p><p>{{clipboard}}</p>"),
            "<p><strong>{{form1.name}}</strong></p>\n<p>{{clipboard}}</p>"
        );
        assert_eq!(
            s("<a href=\"https://x.com\">{{x}}</a>"),
            "<a href=\"https://x.com\">{{x}}</a>"
        );
    }

    #[test]
    fn broken_markup_is_survivable() {
        assert_eq!(s("<b>unclosed"), "<strong>unclosed</strong>");
        assert_eq!(s("</b>stray"), "stray");
        assert_eq!(s("<b><i>x</b></i>"), "<strong><em>x</em></strong>");
        assert_eq!(s("<p>a<p>b"), "<p>a</p>\n<p>b</p>");
        assert_eq!(s("text<"), "text&lt;");
        assert_eq!(s("<b"), "");
        assert_eq!(s("<a href=\"x"), "");
        assert_eq!(s("<script>never closed"), "");
        assert_eq!(s("a<br/>b<br />c"), "a<br>b<br>c");
        let deep = "<b>".repeat(500) + "x";
        assert_eq!(s(&deep), "<strong>x</strong>");
    }

    #[test]
    fn control_characters_never_survive() {
        assert_eq!(s("a\u{0}b\u{7}c\u{feff}d"), "abcd");
        assert_eq!(s("a\u{2028}b"), "a b");
        assert_eq!(
            s("<pre>a\u{0}b\r\nc</pre>"),
            "<pre><code>ab\nc</code></pre>"
        );
    }

    #[test]
    fn tables_and_styles_are_flattened_with_a_note() {
        let r = sanitize_html(
            "<table><tr><td>a</td><td>b</td></tr></table><span style=\"color:red\">c</span>",
        );
        assert_eq!(r.html, "<p>a</p>\n<p>b</p>\n<p>c</p>");
        assert!(r.dropped.iter().any(|d| d.contains("tables")));
        assert!(r.dropped.iter().any(|d| d.contains("colours")));
    }

    #[test]
    fn sanitising_is_idempotent() {
        let samples = [
            "${<b>rs</b>}",
            "<p>a</p><p></p><p>b <i>c <u>d</u></i></p>",
            "<ul><li>a<ul><li>b</li></ul></li><li><p>x</p><p>y</p></li></ul>",
            "<blockquote><p>a</p><ul><li>x</li></ul></blockquote>",
            "<h2><b>t</b> <a href=\"https://a.b\">l</a></h2>text<br>more<br><br>x",
            "<pre>  code\n    more</pre><hr><img src=\"https://a.b/c.png\" width=\"3\">",
            "<table><tr><td>1</td></tr></table>",
            "a &nbsp; b &nbsp;",
            "<strong>a</strong> <strong>b</strong><em> c</em>",
            "<li>stray</li>",
            "<ul>text<li>a</li></ul>",
            "<p>x<br></p><p><br>y</p>",
            "<a href=\"https://a.b\"><img src=\"https://a.b/i.png\" alt=\"i\"></a>",
        ];
        for sample in samples {
            let once = s(sample);
            assert_eq!(s(&once), once, "not idempotent for {sample:?}");
        }
    }

    #[test]
    fn idempotent_on_generated_input() {
        // Small deterministic generator over the interesting tags.
        let parts = [
            "<p>",
            "</p>",
            "<b>",
            "</b>",
            "<i>",
            "</i>",
            "<u>",
            "<ul>",
            "</ul>",
            "<li>",
            "</li>",
            "<ol>",
            "</ol>",
            "<br>",
            "<hr>",
            "<h1>",
            "</h1>",
            "<blockquote>",
            "</blockquote>",
            "<pre>",
            "</pre>",
            "<a href=\"https://a.b\">",
            "</a>",
            "x",
            " y ",
            "&nbsp;",
            "{{v}}",
            "<div>",
            "</div>",
            "<code>",
            "</code>",
            "<s>",
            "</s>",
        ];
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..3000 {
            let mut doc = String::new();
            for _ in 0..(seed % 14 + 1) {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                doc.push_str(parts[((seed >> 33) as usize) % parts.len()]);
            }
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let once = s(&doc);
            assert_eq!(s(&once), once, "not idempotent for {doc:?} -> {once:?}");
            assert!(!once.contains("<script"));
        }
    }

    #[test]
    fn output_only_contains_allowed_tags() {
        let nasty =
            "<form action=x><input name=a><button>b</button></form><object data=x></object>\
                     <embed src=x><link rel=stylesheet href=x><meta http-equiv=refresh content=0>\
                     <base href=x><video src=x></video><marquee onstart=x>m</marquee>";
        assert_eq!(s(nasty), "m");
    }

    // ---- text conversion --------------------------------------------------------

    #[test]
    fn html_text_fallback() {
        assert_eq!(html_to_text("<p>a</p><p>b<br>c</p>"), "a\n\nb\nc");
        assert_eq!(
            html_to_text(
                "<ul><li>x</li><li>y<ul><li>z</li></ul></li></ul><ol><li>1</li><li>2</li></ol>"
            ),
            "- x\n- y\n  - z\n\n1. 1\n2. 2"
        );
        assert_eq!(html_to_text("<blockquote>q</blockquote>"), "> q");
        assert_eq!(html_to_text("a&nbsp;b"), "a b");
        assert_eq!(html_to_text("<a href=\"https://x.y\">link</a>"), "link");
        assert_eq!(
            html_preview("<p>Hello <b>world</b></p><p>two</p>"),
            "Hello world two"
        );
    }

    #[test]
    fn html_to_plain_reports_losses() {
        let c = html_to_plain("<p>a</p><p>b<br>c</p>");
        assert_eq!(c.text, "a\n\nb\nc");
        assert!(c.lost.is_empty());
        let c = html_to_plain("<b>x</b> <a href=\"https://a.b\">y</a><ul><li>z</li></ul>");
        assert_eq!(c.lost, vec!["bold", "links", "lists"]);
        let c = html_to_plain("<script>x</script>t");
        assert_eq!(c.lost.len(), 1);
    }

    #[test]
    fn plain_to_html_basics() {
        let c = plain_to_html("one line").unwrap();
        assert_eq!(c.html, "one line");
        assert!(c.lost.is_empty());
        assert_eq!(plain_to_html("a\nb").unwrap().html, "a<br>b");
        assert_eq!(plain_to_html("a\n\nb").unwrap().html, "<p>a</p>\n<p>b</p>");
        assert_eq!(
            plain_to_html("a\r\nb\r\n\r\nc").unwrap().html,
            "<p>a<br>b</p>\n<p>c</p>"
        );
        assert_eq!(plain_to_html("x < y & z").unwrap().html, "x &lt; y &amp; z");
        assert_eq!(
            plain_to_html("<wr:html a=\"b\"/>").unwrap().html,
            "&lt;wr:html a=\"b\"/&gt;"
        );
        assert_eq!(plain_to_html("Hi {{name}}").unwrap().html, "Hi {{name}}");
        assert_eq!(plain_to_html("a  b").unwrap().html, "a&nbsp;&nbsp;b");
        assert_eq!(plain_to_html("  lead").unwrap().html, "&nbsp;&nbsp;lead");
        assert_eq!(plain_to_html("").unwrap().html, "");
        assert!(plain_to_html("a\u{7}b").is_err());
    }

    #[test]
    fn plain_to_html_reports_changes() {
        assert!(!plain_to_html("a\tb").unwrap().lost.is_empty());
        assert!(!plain_to_html("a\n\n\n\nb").unwrap().lost.is_empty());
        assert!(!plain_to_html("\nlead").unwrap().lost.is_empty());
        assert!(plain_to_html("a\n\nb").unwrap().lost.is_empty());
    }

    #[test]
    fn plain_html_plain_round_trip() {
        for t in [
            "simple",
            "two\nlines",
            "para\n\nsecond",
            "a  double  space",
            "  indented\n    more",
            "{{a}} & <b> \"q\" 'x' é ✓",
            "trailing space ",
            "x\n\ny\nz\n\nw",
        ] {
            let h = plain_to_html(t).unwrap();
            assert!(h.lost.is_empty(), "{t:?}");
            assert_eq!(s(&h.html), h.html, "html canonical for {t:?}");
            let back = html_to_plain(&h.html);
            assert!(back.lost.is_empty());
            assert_eq!(back.text, t, "round trip of {t:?} via {:?}", h.html);
        }
    }

    // ---- file edits -------------------------------------------------------------

    const FILE: &str = "# my phrases\nmatches:\n  # first\n  - trigger: \":a\"\n    replace: \"plain\"\n\n  # bold one\n  - trigger: \":b\"\n    html: \"${<b>rs</b>}\"\n    label: \"L\"\n    word: true\n\n  - trigger: \":c\"\n    html: |\n      <p>x</p>\n      <p>y</p>\n    vars:\n      - name: v\n        type: echo\n        params:\n          echo: hi\n  - trigger: \":d\"\n    replace: \"last\"\nglobal_vars:\n  - name: g\n    type: echo\n    params:\n      echo: g\n";

    #[test]
    fn html_phrases_get_a_text_preview() {
        let p = parse_match_text(FILE).0;
        assert_eq!(p[1].preview, "${rs}");
        assert_eq!(p[1].content, "${<b>rs</b>}");
        assert_eq!(p[2].preview, "x y");
    }

    #[test]
    fn set_body_changes_only_that_field() {
        let out = set_html_body(FILE, 1, "<p>new</p><p>two</p>").unwrap();
        let expected = FILE.replace(
            "    html: \"${<b>rs</b>}\"\n",
            "    html: |-\n      <p>new</p>\n      <p>two</p>\n",
        );
        assert_eq!(out, expected);
        // single paragraph: quoted scalar
        let out = set_html_body(FILE, 1, "<p>one <i>line</i></p>").unwrap();
        assert!(out.contains("    html: \"one <em>line</em>\"\n    label: \"L\"\n    word: true\n"));
        // every other line identical
        let (a, b): (Vec<&str>, Vec<&str>) = (FILE.lines().collect(), out.lines().collect());
        assert_eq!(a[..8], b[..8]);
    }

    #[test]
    fn set_body_noop_is_byte_identical() {
        for i in [1, 2] {
            let p = &parse_match_text(FILE).0[i];
            assert_eq!(set_html_body(FILE, i, &p.content).unwrap(), FILE);
            // equivalent markup (editor round trip) is also a no-op
            assert_eq!(set_html_body(FILE, i, &s(&p.content)).unwrap(), FILE);
        }
        assert_eq!(
            set_html_body(FILE, 1, "<p>${<b>rs</b>}</p>").unwrap(),
            FILE,
            "wrapping in <p> means the same"
        );
    }

    #[test]
    fn set_body_keeps_vars_comments_and_tail() {
        let out = set_html_body(FILE, 2, "<p>z</p>").unwrap();
        assert!(out.contains("    html: \"z\"\n    vars:\n      - name: v\n"));
        assert!(out.contains("  # bold one\n"));
        assert!(out
            .ends_with("global_vars:\n  - name: g\n    type: echo\n    params:\n      echo: g\n"));
    }

    #[test]
    fn set_body_sanitises_before_writing() {
        let out = set_html_body(FILE, 1, "<b>x</b><script>alert(1)</script>").unwrap();
        assert!(out.contains("html: \"<strong>x</strong>\""));
        assert!(!out.contains("script"));
    }

    #[test]
    fn set_body_escapes_quotes_and_special_text() {
        let out = set_html_body(FILE, 1, "say \"hi\" \\ <b>x</b>").unwrap();
        assert!(out.contains("html: \"say \\\"hi\\\" \\\\ <strong>x</strong>\""));
        let (p, w) = parse_match_text(&out);
        assert!(w.is_empty());
        assert_eq!(p[1].content, "say \"hi\" \\ <strong>x</strong>");
    }

    #[test]
    fn set_body_handles_unicode_and_leading_markup() {
        let out = set_html_body(FILE, 1, "<p>caf\u{e9} \u{2713}</p><p>\u{1f600}</p>").unwrap();
        let (p, _) = parse_match_text(&out);
        assert_eq!(p[1].content, "<p>caf\u{e9} \u{2713}</p>\n<p>\u{1f600}</p>");
    }

    #[test]
    fn set_body_keeps_crlf_and_no_final_newline() {
        let crlf = FILE.replace('\n', "\r\n");
        let out = set_html_body(&crlf, 1, "<p>a</p><p>b</p>").unwrap();
        assert!(out.contains("html: |-\r\n      <p>a</p>\r\n      <p>b</p>\r\n    label"));
        assert!(!out.replace("\r\n", "").contains('\n'));
        let no_nl = "matches:\n  - trigger: a\n    html: \"x <b>y</b>\"";
        let out = set_html_body(no_nl, 0, "z").unwrap();
        assert_eq!(out, "matches:\n  - trigger: a\n    html: \"z\"");
    }

    #[test]
    fn set_body_in_last_entry_with_following_keys() {
        let t = "matches:\n  - trigger: a\n    html: \"<b>x</b>\"\n\n\nglobal_vars: []\n";
        let out = set_html_body(t, 0, "<i>y</i>").unwrap();
        assert_eq!(
            out,
            "matches:\n  - trigger: a\n    html: \"<em>y</em>\"\n\n\nglobal_vars: []\n"
        );
    }

    #[test]
    fn set_body_refuses_other_kinds_and_bad_indexes() {
        assert!(set_html_body(FILE, 0, "x")
            .unwrap_err()
            .contains("not `html`"));
        assert!(set_html_body(FILE, 9, "x")
            .unwrap_err()
            .contains("no entry"));
        let flow = "matches: [{trigger: a, html: x}]\n";
        assert!(set_html_body(flow, 0, "y").is_err());
        let both = "matches:\n  - trigger: a\n    html: x\n    replace: y\n";
        assert!(set_html_body(both, 0, "z").is_err());
        let md = "matches:\n  - trigger: a\n    markdown: x\n";
        assert!(set_html_body(md, 0, "z").unwrap_err().contains("markdown"));
        assert!(set_html_body("matches: [", 0, "z").is_err());
    }

    #[test]
    fn set_body_in_deeply_indented_file() {
        let t = "matches:\n    - trigger: a\n      html: |\n          <b>x</b>\n          <i>y</i>\n    - trigger: b\n      replace: q\n";
        let out = set_html_body(t, 0, "<p>1</p><p>2</p>").unwrap();
        assert_eq!(
            out,
            "matches:\n    - trigger: a\n      html: |-\n          <p>1</p>\n          <p>2</p>\n    - trigger: b\n      replace: q\n"
        );
    }

    #[test]
    fn set_body_with_inline_comments_and_blank_tail() {
        let t = "matches:\n  - trigger: a\n    html: \"<b>x</b>\"  # why\n\n  # next\n  - trigger: b\n    replace: q\n";
        let out = set_html_body(t, 0, "<i>y</i>").unwrap();
        assert_eq!(
            out,
            "matches:\n  - trigger: a\n    html: \"<em>y</em>\"\n\n  # next\n  - trigger: b\n    replace: q\n"
        );
    }

    #[test]
    fn convert_plain_to_html_and_back() {
        let t = "matches:\n  - trigger: a\n    replace: \"Hello {{n}}\"\n    label: L\n    vars:\n      - name: n\n        type: echo\n        params:\n          echo: x\n";
        let h = convert_to_html(t, 0, false).unwrap();
        assert_eq!(
            h,
            t.replace("replace: \"Hello {{n}}\"", "html: \"Hello {{n}}\"")
        );
        let back = convert_to_plain(&h, 0, false).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn convert_multiline_block_scalars() {
        let t = "matches:\n  - trigger: a\n    replace: |-\n      one\n      two\n\n      three\n  - trigger: b\n    replace: z\n";
        let h = convert_to_html(t, 0, false).unwrap();
        assert!(h.contains("html: |-\n      <p>one<br>two</p>\n      <p>three</p>\n  - trigger: b"));
        let back = convert_to_plain(&h, 0, false).unwrap();
        assert_eq!(
            back.replace("one\n      two\n\n      three", "X"),
            t.replace("one\n      two\n\n      three", "X")
        );
        let (p, _) = parse_match_text(&back);
        assert_eq!(p[0].content, "one\ntwo\n\nthree");
        assert_eq!(p[0].kind, "replace");
    }

    #[test]
    fn convert_to_html_refuses_lossy_unless_allowed() {
        let t = "matches:\n  - trigger: a\n    replace: |\n      keep\n";
        let err = convert_to_html(t, 0, false).unwrap_err();
        assert!(err.starts_with(LOSSY_PREFIX), "{err}");
        assert!(err.contains("line break at the end"));
        let h = convert_to_html(t, 0, true).unwrap();
        assert!(h.contains("html: \"keep\""));
        let tabs = "matches:\n  - trigger: a\n    replace: \"a\\tb\"\n";
        assert!(convert_to_html(tabs, 0, false)
            .unwrap_err()
            .starts_with(LOSSY_PREFIX));
        assert!(convert_to_html(tabs, 0, true).is_ok());
    }

    #[test]
    fn convert_to_plain_refuses_lossy_unless_allowed() {
        let t = "matches:\n  - trigger: a\n    html: \"${<b>rs</b>}\"\n";
        let err = convert_to_plain(t, 0, false).unwrap_err();
        assert!(
            err.starts_with(LOSSY_PREFIX) && err.contains("bold"),
            "{err}"
        );
        let out = convert_to_plain(t, 0, true).unwrap();
        assert_eq!(out, "matches:\n  - trigger: a\n    replace: \"${rs}\"\n");
        // unformatted html converts silently
        let plain = "matches:\n  - trigger: a\n    html: \"just text\"\n";
        assert_eq!(
            convert_to_plain(plain, 0, false).unwrap(),
            "matches:\n  - trigger: a\n    replace: \"just text\"\n"
        );
    }

    #[test]
    fn convert_keeps_other_entries_and_comments() {
        let out = convert_to_html(FILE, 0, false).unwrap();
        assert_eq!(out, FILE.replace("replace: \"plain\"", "html: \"plain\""));
        let out = convert_to_html(FILE, 3, false).unwrap();
        assert_eq!(out, FILE.replace("replace: \"last\"", "html: \"last\""));
        assert!(convert_to_html(FILE, 1, false).is_err(), "already html");
        assert!(convert_to_plain(FILE, 0, true).is_err(), "already plain");
    }

    /// The sequences the app's save commands run, one `edit_file` closure each.
    #[test]
    fn app_save_sequences_compose() {
        use crate::edit::{add_phrase, update_phrase, CommentChange, NewPhrase, PhraseEdit};
        let edit = |triggers: &[&str], body: Option<&str>| PhraseEdit {
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            label: Some("L".into()),
            body: body.map(String::from),
            comment: CommentChange::Keep,
            options: None,
        };

        // plain -> rich: edit fields, convert, then set the rich body
        let t = update_phrase(FILE, 0, &edit(&[":a2"], None)).unwrap();
        let t = convert_to_html(&t, 0, true).unwrap();
        let t = set_html_body(&t, 0, "<p>x</p><p><b>y</b></p>").unwrap();
        let (p, w) = parse_match_text(&t);
        assert!(w.is_empty());
        assert_eq!(
            (p[0].kind.as_str(), p[0].triggers[0].as_str()),
            ("html", ":a2")
        );
        assert_eq!(p[0].content, "<p>x</p>\n<p><strong>y</strong></p>");
        assert_eq!(p[0].label.as_deref(), Some("L"));
        assert!(t.contains("  # first\n"), "comment kept");

        // rich phrase: edit fields only, body untouched
        let t = update_phrase(FILE, 1, &edit(&[":b2"], None)).unwrap();
        assert!(t.contains("html: \"${<b>rs</b>}\""));

        // rich -> plain, then the plain text the user typed
        let t = convert_to_plain(FILE, 1, true).unwrap();
        let t = update_phrase(&t, 1, &edit(&[":b"], Some("typed text"))).unwrap();
        let (p, _) = parse_match_text(&t);
        assert_eq!(
            (p[1].kind.as_str(), p[1].content.as_str()),
            ("replace", "typed text")
        );
        assert!(t.contains("word: true"), "other fields stay");

        // new rich phrase: add as plain text, convert, set the body
        let added = add_phrase(
            FILE,
            &NewPhrase {
                triggers: vec![":new".into()],
                label: None,
                body: html_to_text("<p>a</p><p>b</p>"),
                comment: None,
                options: Default::default(),
            },
        )
        .unwrap();
        let index = parse_match_text(&added)
            .0
            .iter()
            .map(|p| p.index)
            .max()
            .unwrap();
        let t = convert_to_html(&added, index, true).unwrap();
        let t = set_html_body(&t, index, "<p>a</p><p><i>b</i></p>").unwrap();
        let p = parse_match_text(&t).0;
        assert_eq!(p[index].kind, "html");
        assert_eq!(p[index].content, "<p>a</p>\n<p><em>b</em></p>");
    }

    #[test]
    fn convert_special_text_is_escaped() {
        let t = "matches:\n  - trigger: a\n    replace: \"<b>x</b> & \\\"y\\\"\"\n";
        let h = convert_to_html(t, 0, false).unwrap();
        let (p, w) = parse_match_text(&h);
        assert!(w.is_empty());
        assert_eq!(p[0].content, "&lt;b&gt;x&lt;/b&gt; &amp; \"y\"");
        assert_eq!(convert_to_plain(&h, 0, false).unwrap(), t);
    }

    /// Dry run against real files: opening and saving every `html:` phrase unchanged must be
    /// byte-identical. Reads only. Run with `cargo test -- --ignored real_files`.
    #[test]
    #[ignore = "reads the user's espanso config"]
    fn real_files_noop_save_is_identical() {
        let dir = std::env::var("XPANDO_TEST_CONFIG")
            .map(std::path::PathBuf::from)
            .or_else(|_| {
                std::env::var("APPDATA").map(|a| std::path::PathBuf::from(a).join("espanso"))
            })
            .expect("no config dir");
        let mut seen = 0;
        for file in crate::discover_match_files(&dir).unwrap() {
            let text = std::fs::read_to_string(&file).unwrap();
            for p in parse_match_text(&text)
                .0
                .iter()
                .filter(|p| p.kind == "html")
            {
                let same = set_html_body(&text, p.index, &p.content).unwrap();
                assert_eq!(same, text, "{} entry {}", file.display(), p.index);
                let editor_form = s(&p.content);
                assert_eq!(set_html_body(&text, p.index, &editor_form).unwrap(), text);
                seen += 1;
            }
        }
        eprintln!("checked {seen} html phrases");
    }
}
