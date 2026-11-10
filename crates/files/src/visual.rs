//! C43: visual Markdown editing only where the round trip is exact (FRD-0001 R37).
//!
//! The source is split into blocks that keep their exact bytes. Editable blocks are ATX
//! headings, plain paragraphs and the body of fenced code; everything else (lists, quotes,
//! tables, HTML, front matter, setext headings, indented code, reference definitions) is a
//! preserved block shown read-only and written back unchanged. An edit replaces only its
//! block's text and is refused when it would change the block's kind. Visual mode is offered
//! only when `serialize(parse(source)) == source` byte for byte.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Code,
    Blank,
    /// Syntax outside the visual subset: kept byte for byte.
    Preserved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub kind: BlockKind,
    /// Exact bytes before the editable text (`## `, the opening fence line).
    pub prefix: String,
    /// The editable text (single line for headings, lines joined for paragraphs).
    pub text: String,
    /// Exact bytes after the text (line ending, closing fence line).
    pub suffix: String,
    pub level: u8,
    pub editable: bool,
}
impl Block {
    fn raw(&self) -> String {
        format!("{}{}{}", self.prefix, self.text, self.suffix)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualDocument {
    pub blocks: Vec<Block>,
    pub exact: bool,
    pub editable_blocks: usize,
    pub preserved_blocks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum EditError {
    NotEditable { index: usize },
    NoSuchBlock { index: usize },
    WouldChangeKind { index: usize },
    NotExact,
}
impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::NotEditable { index } => write!(f, "Block {index} is kept as source and cannot be edited visually."),
            EditError::NoSuchBlock { index } => write!(f, "Block {index} does not exist; reload the document."),
            EditError::WouldChangeKind { index } => write!(f, "This edit would turn block {index} into different Markdown; edit it in Source mode."),
            EditError::NotExact => write!(f, "This document does not round-trip exactly; use Source mode."),
        }
    }
}

fn lines_with_endings(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn ending(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

fn atx(body: &str) -> Option<(u8, usize)> {
    let hashes = body.chars().take_while(|c| *c == '#').count();
    let rest = &body[hashes..];
    ((1..=6).contains(&hashes) && rest.starts_with(' ') && !rest.trim_end().ends_with('#') && !rest.trim().is_empty())
        .then_some((hashes as u8, hashes + 1))
}

fn starts_block(body: &str) -> bool {
    let t = body.trim_start();
    let indent = body.len() - t.len();
    indent >= 4
        || t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with("```")
        || t.starts_with("~~~")
        || t.starts_with('<')
        || t.starts_with('|')
        || t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("+ ")
        || t.starts_with("---")
        || t.starts_with("***")
        || t.starts_with("___")
        || t.starts_with("===")
        || (t.starts_with('[') && t.contains("]:"))
        || {
            let digits = t.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") "))
        }
}

pub fn parse(source: &str) -> VisualDocument {
    let lines = lines_with_endings(source);
    let mut blocks = Vec::new();
    let mut i = 0;
    // Front matter is always preserved.
    if lines.first().is_some_and(|l| ending(l).0 == "---")
        && let Some(end) = lines.iter().skip(1).position(|l| ending(l).0 == "---")
    {
        blocks.push(Block { kind: BlockKind::Preserved, prefix: lines[..end + 2].concat(), text: String::new(), suffix: String::new(), level: 0, editable: false });
        i = end + 2;
    }
    while i < lines.len() {
        let (body, end) = ending(lines[i]);
        let trimmed = body.trim_start();
        if body.trim().is_empty() {
            blocks.push(Block { kind: BlockKind::Blank, prefix: lines[i].to_owned(), text: String::new(), suffix: String::new(), level: 0, editable: false });
            i += 1;
            continue;
        }
        if body.len() == trimmed.len()
            && let Some((level, start)) = atx(body)
        {
            blocks.push(Block { kind: BlockKind::Heading, prefix: body[..start].to_owned(), text: body[start..].to_owned(), suffix: end.to_owned(), level, editable: true });
            i += 1;
            continue;
        }
        if body.len() == trimmed.len() && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            let fence: String = trimmed.chars().take_while(|c| *c == '`' || *c == '~').collect();
            let close = lines.iter().enumerate().skip(i + 1).find(|(_, l)| {
                let b = ending(l).0;
                b.starts_with(&fence) && b.trim_end().chars().all(|c| c == fence.chars().next().unwrap_or('`'))
            });
            match close {
                Some((j, _)) => {
                    let inner = lines[i + 1..j].concat();
                    blocks.push(Block { kind: BlockKind::Code, prefix: lines[i].to_owned(), text: inner, suffix: lines[j].to_owned(), level: 0, editable: !fence.is_empty() });
                    i = j + 1;
                }
                None => {
                    blocks.push(Block { kind: BlockKind::Preserved, prefix: lines[i..].concat(), text: String::new(), suffix: String::new(), level: 0, editable: false });
                    i = lines.len();
                }
            }
            continue;
        }
        if !starts_block(body) {
            // Paragraph: consecutive lines that do not start another block. A following
            // `===`/`---` line would make it a setext heading: then the whole run is preserved.
            let mut j = i;
            while j < lines.len() {
                let b = ending(lines[j]).0;
                if b.trim().is_empty() || (j > i && starts_block(b)) {
                    break;
                }
                j += 1;
            }
            let setext = lines.get(j).is_some_and(|l| {
                let b = ending(l).0.trim();
                !b.is_empty() && (b.chars().all(|c| c == '=') || b.chars().all(|c| c == '-'))
            });
            let run = lines[i..j].concat();
            let (text_part, tail) = {
                let body = run.trim_end_matches(['\n', '\r']);
                (body.to_owned(), run[body.len()..].to_owned())
            };
            let hard_break = text_part.lines().any(|l| l.ends_with("  ") || l.ends_with('\\'));
            if setext || hard_break {
                let until = if setext { j + 1 } else { j };
                blocks.push(Block { kind: BlockKind::Preserved, prefix: lines[i..until].concat(), text: String::new(), suffix: String::new(), level: 0, editable: false });
                i = until;
            } else {
                blocks.push(Block { kind: BlockKind::Paragraph, prefix: String::new(), text: text_part, suffix: tail, level: 0, editable: true });
                i = j;
            }
            continue;
        }
        // Any other construct: preserve the run up to the next blank line.
        let mut j = i + 1;
        while j < lines.len() && !ending(lines[j]).0.trim().is_empty() {
            j += 1;
        }
        blocks.push(Block { kind: BlockKind::Preserved, prefix: lines[i..j].concat(), text: String::new(), suffix: String::new(), level: 0, editable: false });
        i = j;
    }
    let exact = serialize(&blocks) == source;
    VisualDocument {
        editable_blocks: blocks.iter().filter(|b| b.editable).count(),
        preserved_blocks: blocks.iter().filter(|b| b.kind == BlockKind::Preserved).count(),
        exact,
        blocks,
    }
}

pub fn serialize(blocks: &[Block]) -> String {
    blocks.iter().map(Block::raw).collect()
}

/// Visual mode is available for this source.
pub fn available(source: &str) -> bool {
    let doc = parse(source);
    doc.exact && doc.editable_blocks > 0
}

/// Applies text edits to editable blocks and returns the new source.
pub fn apply(source: &str, edits: &[(usize, String)]) -> Result<String, EditError> {
    let mut doc = parse(source);
    if !doc.exact {
        return Err(EditError::NotExact);
    }
    for (index, text) in edits {
        let block = doc.blocks.get(*index).ok_or(EditError::NoSuchBlock { index: *index })?;
        if !block.editable {
            return Err(EditError::NotEditable { index: *index });
        }
        let line_ending = if source.contains("\r\n") { "\r\n" } else { "\n" };
        let candidate = match block.kind {
            BlockKind::Heading => {
                if text.contains(['\n', '\r']) || text.trim().is_empty() || text.trim_end().ends_with('#') {
                    return Err(EditError::WouldChangeKind { index: *index });
                }
                text.clone()
            }
            BlockKind::Paragraph => {
                let normalized = text.replace("\r\n", "\n");
                let lines: Vec<&str> = normalized.split('\n').collect();
                if lines.iter().any(|l| l.trim().is_empty() || starts_block(l) || l.ends_with("  ") || l.ends_with('\\')) {
                    return Err(EditError::WouldChangeKind { index: *index });
                }
                lines.join(line_ending)
            }
            BlockKind::Code => {
                let fence: String = block.prefix.trim_start().chars().take_while(|c| *c == '`' || *c == '~').collect();
                let mut body = text.replace("\r\n", "\n");
                if !body.is_empty() && !body.ends_with('\n') {
                    body.push('\n');
                }
                if body.lines().any(|l| l.trim_start().starts_with(&fence)) {
                    return Err(EditError::WouldChangeKind { index: *index });
                }
                body.replace('\n', line_ending)
            }
            _ => return Err(EditError::NotEditable { index: *index }),
        };
        doc.blocks[*index].text = candidate;
    }
    let result = serialize(&doc.blocks);
    // The result must itself round-trip and keep every preserved block byte for byte.
    let check = parse(&result);
    let preserved_before: Vec<String> = doc.blocks.iter().filter(|b| b.kind == BlockKind::Preserved).map(Block::raw).collect();
    let preserved_after: Vec<String> = check.blocks.iter().filter(|b| b.kind == BlockKind::Preserved).map(Block::raw).collect();
    if !check.exact || preserved_before != preserved_after || check.blocks.len() != doc.blocks.len() {
        return Err(EditError::WouldChangeKind { index: edits.first().map_or(0, |e| e.0) });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &[&str] = &[
        "# Title\n\nA paragraph\nwith two lines.\n\n## Next\n",
        "# Title\r\n\r\nWindows endings\r\nkept.\r\n",
        "No trailing newline",
        "---\ntitle: x\n---\n# After front matter\n",
        "- list item\n- second\n\nParagraph after.\n",
        "1. one\n2) two\n",
        "> quote\n> more\n\ntext\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
        "<div align=center>\n<img src=x>\n</div>\n",
        "Setext heading\n==============\n\nbody\n",
        "Line with hard break  \nnext\n",
        "    indented code\n\nafter\n",
        "[ref]: https://example.com\n\nSee [x][ref].\n",
        "```rust\nfn main() {}\n```\n\n~~~\ntilde\n~~~\n",
        "```\nunterminated fence\nstill code",
        "# Closed heading #\n",
        "Text with **bold**, `code`, [link](a.md) and <span>html</span>.\n",
        "\n\n\nleading blanks\n\n\n",
        "### \n",
        "Emoji 🦀 and città\n",
    ];

    #[test]
    fn every_corpus_case_round_trips_byte_for_byte() {
        for source in CORPUS {
            let doc = parse(source);
            assert!(doc.exact, "not exact: {source:?} -> {:?}", serialize(&doc.blocks));
        }
    }

    #[test]
    fn unsupported_syntax_is_preserved_and_not_editable() {
        for (source, preserved) in [
            ("- list item\n- second\n\nParagraph after.\n", 1),
            ("> quote\n\ntext\n", 1),
            ("| a | b |\n|---|---|\n", 1),
            ("Setext heading\n===\n\nbody\n", 1),
            ("Line with hard break  \nnext\n", 1),
            ("# Closed heading #\n", 1),
            ("---\ntitle: x\n---\n# H\n", 1),
        ] {
            let doc = parse(source);
            assert_eq!(doc.preserved_blocks, preserved, "{source:?}: {:?}", doc.blocks);
            for (index, block) in doc.blocks.iter().enumerate() {
                if block.kind == BlockKind::Preserved {
                    assert_eq!(apply(source, &[(index, "x".into())]), Err(EditError::NotEditable { index }));
                }
            }
        }
    }

    #[test]
    fn edits_change_only_their_block_and_keep_everything_else() {
        let source = "# Title\n\nOld text.\n\n- keep this list\n- exactly\n\n```js\nlet a = 1;\n```\n";
        let doc = parse(source);
        let heading = doc.blocks.iter().position(|b| b.kind == BlockKind::Heading).unwrap();
        let paragraph = doc.blocks.iter().position(|b| b.kind == BlockKind::Paragraph).unwrap();
        let code = doc.blocks.iter().position(|b| b.kind == BlockKind::Code).unwrap();
        let edited = apply(source, &[(heading, "New title".into()), (paragraph, "New text\non two lines.".into()), (code, "let a = 2;".into())]).unwrap();
        assert_eq!(edited, "# New title\n\nNew text\non two lines.\n\n- keep this list\n- exactly\n\n```js\nlet a = 2;\n```\n");
        let crlf = "Hello\r\nworld\r\n";
        assert_eq!(apply(crlf, &[(0, "Hi\nthere".into())]).unwrap(), "Hi\r\nthere\r\n");
    }

    #[test]
    fn edits_that_would_change_the_kind_are_refused() {
        let source = "# Title\n\nText.\n\n```\ncode\n```\n";
        for (index, text) in [(0, "two\nlines"), (0, "ends with #"), (0, " "), (2, "- now a list"), (2, "para\n\nsplit"), (2, "# heading"), (2, "hard break  "), (4, "```\nbreak out")] {
            assert!(matches!(apply(source, &[(index, text.into())]), Err(EditError::WouldChangeKind { .. })), "{index} {text:?}");
        }
        assert_eq!(apply(source, &[(99, "x".into())]), Err(EditError::NoSuchBlock { index: 99 }));
    }

    #[test]
    fn availability_requires_an_exact_round_trip_and_an_editable_block() {
        assert!(available("# A\n\ntext\n"));
        assert!(!available("- only a list\n"));
        assert!(!available(""));
    }
}
