//! C42.2: Markdown rendered to a small, inert HTML subset (FRD-0001 R36, R41).
//!
//! Every input character is escaped first; the renderer emits only a fixed set of tags with
//! no attributes except a `data-path` on links that point inside the project. Raw HTML,
//! scripts, event handlers and styles stay visible text. Remote links are shown as text with
//! their address and never become requests; images are never loaded (their alt text and
//! source are shown). Fenced `mermaid` blocks stay code: there is no diagram renderer.
//! Input and output are bounded.
pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// A link target that stays inside the project: relative, no scheme, no `..`, no backslash.
pub fn local_target(target: &str) -> Option<String> {
    let path = target.split(['#', '?']).next().unwrap_or_default();
    let lower = target.to_ascii_lowercase();
    let schemed = lower.find(':').is_some_and(|i| lower[..i].chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')));
    (!path.is_empty()
        && !schemed
        && !target.starts_with(['/', '\\'])
        && !target.contains('\\')
        && !path.chars().any(|c| c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '`'))
        && !path.split('/').any(|p| p == ".."))
    .then(|| path.trim_start_matches("./").to_owned())
}

fn find_close(chars: &[char], from: usize, close: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == close)
}

fn inline(text: &str, out: &mut String) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut String| {
        escape(plain, out);
        plain.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() && "\\`*_[]()#!<>-".contains(chars[i + 1]) {
            plain.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`'
            && let Some(end) = find_close(&chars, i + 1, '`')
        {
            flush(&mut plain, out);
            out.push_str("<code>");
            escape(&chars[i + 1..end].iter().collect::<String>(), out);
            out.push_str("</code>");
            i = end + 1;
            continue;
        }
        if (c == '*' || c == '_') && i + 1 < chars.len() && chars[i + 1] == c {
            if let Some(end) = (i + 2..chars.len().saturating_sub(1)).find(|&j| chars[j] == c && chars[j + 1] == c) {
                flush(&mut plain, out);
                out.push_str("<strong>");
                inline(&chars[i + 2..end].iter().collect::<String>(), out);
                out.push_str("</strong>");
                i = end + 2;
                continue;
            }
        } else if (c == '*' || c == '_')
            && i + 1 < chars.len()
            && !chars[i + 1].is_whitespace()
            && let Some(end) = find_close(&chars, i + 1, c)
            && end > i + 1
        {
            flush(&mut plain, out);
            out.push_str("<em>");
            inline(&chars[i + 1..end].iter().collect::<String>(), out);
            out.push_str("</em>");
            i = end + 1;
            continue;
        }
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if (c == '[' || image)
            && let Some(label_end) = find_close(&chars, i + if image { 2 } else { 1 }, ']')
            && chars.get(label_end + 1) == Some(&'(')
            && let Some(target_end) = find_close(&chars, label_end + 2, ')')
        {
            flush(&mut plain, out);
            let label: String = chars[i + if image { 2 } else { 1 }..label_end].iter().collect();
            let target: String = chars[label_end + 2..target_end].iter().collect::<String>().trim().to_owned();
            if image {
                out.push_str("<span class=\"md-image\">[image: ");
                escape(&label, out);
                out.push_str(" · ");
                escape(&target, out);
                out.push_str(" · not loaded]</span>");
            } else if let Some(local) = local_target(&target) {
                out.push_str("<a data-path=\"");
                escape(&local, out);
                out.push_str("\">");
                inline(&label, out);
                out.push_str("</a>");
            } else {
                out.push_str("<span class=\"md-link\">");
                inline(&label, out);
                out.push_str(" (");
                escape(&target, out);
                out.push_str(")</span>");
            }
            i = target_end + 1;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut plain, out);
}

#[derive(PartialEq)]
enum Block {
    None,
    Paragraph,
    List(bool),
    Quote,
}

fn close(block: &mut Block, out: &mut String) {
    match block {
        Block::Paragraph => out.push_str("</p>\n"),
        Block::List(false) => out.push_str("</ul>\n"),
        Block::List(true) => out.push_str("</ol>\n"),
        Block::Quote => out.push_str("</blockquote>\n"),
        Block::None => {}
    }
    *block = Block::None;
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some((false, rest));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && digits < 10 && trimmed[digits..].starts_with(". ")).then(|| (true, &trimmed[digits + 2..]))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    pub truncated: bool,
}

pub fn render(source: &str) -> Rendered {
    let mut truncated = source.len() > MAX_INPUT_BYTES;
    let mut end = source.len().min(MAX_INPUT_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end * 2);
    let mut block = Block::None;
    let mut fence: Option<String> = None;
    for raw in source[..end].lines() {
        if out.len() > MAX_OUTPUT_BYTES {
            truncated = true;
            break;
        }
        let line = raw.trim_end_matches('\r');
        if let Some(marker) = &fence {
            if line.trim_start().starts_with(marker.as_str()) {
                out.push_str("</code></pre>\n");
                fence = None;
            } else {
                escape(line, &mut out);
                out.push('\n');
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            close(&mut block, &mut out);
            let marker: String = trimmed.chars().take_while(|c| *c == '`' || *c == '~').collect();
            let language: String = trimmed[marker.len()..].trim().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect();
            out.push_str("<pre><code");
            if !language.is_empty() {
                out.push_str(" data-language=\"");
                out.push_str(&language);
                out.push('"');
            }
            out.push('>');
            fence = Some(marker);
            continue;
        }
        if trimmed.is_empty() {
            close(&mut block, &mut out);
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            close(&mut block, &mut out);
            out.push_str(&format!("<h{level}>"));
            inline(trimmed[level..].trim(), &mut out);
            out.push_str(&format!("</h{level}>\n"));
            continue;
        }
        if matches!(trimmed, "---" | "***" | "___") {
            close(&mut block, &mut out);
            out.push_str("<hr>\n");
            continue;
        }
        if let Some((ordered, item)) = list_item(line) {
            if block != Block::List(ordered) {
                close(&mut block, &mut out);
                out.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
                block = Block::List(ordered);
            }
            out.push_str("<li>");
            inline(item, &mut out);
            out.push_str("</li>\n");
            continue;
        }
        if let Some(quoted) = trimmed.strip_prefix('>') {
            if block != Block::Quote {
                close(&mut block, &mut out);
                out.push_str("<blockquote>\n");
                block = Block::Quote;
            }
            out.push_str("<p>");
            inline(quoted.trim_start(), &mut out);
            out.push_str("</p>\n");
            continue;
        }
        if block != Block::Paragraph {
            close(&mut block, &mut out);
            out.push_str("<p>");
            block = Block::Paragraph;
        } else {
            out.push('\n');
        }
        inline(trimmed, &mut out);
    }
    if fence.is_some() {
        out.push_str("</code></pre>\n");
    }
    close(&mut block, &mut out);
    Rendered { html: out, truncated }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_common_subset() {
        let html = render("# Title\n\nSome *em* and **bold** and `x<y`.\n\n- a\n- b\n\n1. one\n\n> quoted\n\n```rust\nfn main() {}\n```\n").html;
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<em>em</em>") && html.contains("<strong>bold</strong>") && html.contains("<code>x&lt;y</code>"));
        assert!(html.contains("<ul>\n<li>a</li>\n<li>b</li>\n</ul>") && html.contains("<ol>\n<li>one</li>"));
        assert!(html.contains("<blockquote>") && html.contains("<pre><code data-language=\"rust\">fn main() {}\n</code></pre>"));
    }

    #[test]
    fn hostile_markdown_stays_inert() {
        let hostile = concat!(
            "<script>alert(1)</script>\n\n",
            "<img src=x onerror=alert(1)>\n\n",
            "[click](javascript:alert(1)) [web](https://evil.example/x) [file](file:///C:/Windows)\n\n",
            "![pixel](https://tracker.example/p.gif) ![local](img/a.png)\n\n",
            "[up](../../secrets.txt) [win](C:\\Windows\\system32) [ok](docs/guide.md#part)\n\n",
            "[\"><script>](x\" onmouseover=\"alert(1))\n\n",
            "```mermaid\ngraph TD; A-->B\n```\n",
        );
        let html = render(hostile).html;
        // Only escaped text may contain attribute-like words; no tag carries them.
        for tag in html.split('<').skip(1).map(|t| t.split('>').next().unwrap_or_default()) {
            let attributes: Vec<&str> = tag.split_whitespace().skip(1).collect();
            assert!(attributes.iter().all(|a| a.starts_with("data-path=") || a.starts_with("data-language=") || a.starts_with("class=")), "{tag}");
        }
        for forbidden in ["<script", "<img", "href=", "<iframe", "style="] {
            assert!(!html.contains(forbidden), "{forbidden}: {html}");
        }
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("click (javascript:alert(1") && html.contains("(https://evil.example/x)"));
        assert!(html.contains("[image: pixel · https://tracker.example/p.gif · not loaded]"));
        assert!(html.contains("<a data-path=\"docs/guide.md\">ok</a>"));
        assert!(!html.contains("data-path=\"../") && !html.contains("data-path=\"C:"));
        assert!(html.contains("<pre><code data-language=\"mermaid\">graph TD; A--&gt;B"));
        let tags: Vec<&str> = html.match_indices('<').map(|(i, _)| html[i + 1..].split(['>', ' ']).next().unwrap_or_default()).collect();
        let allowed = ["h1", "/h1", "p", "/p", "em", "/em", "strong", "/strong", "code", "/code", "pre", "/pre", "ul", "/ul", "ol", "/ol", "li", "/li", "blockquote", "/blockquote", "hr", "a", "/a", "span", "/span", "h2", "/h2", "h3", "/h3"];
        assert!(tags.iter().all(|t| allowed.contains(t)), "{tags:?}");
    }

    #[test]
    fn input_is_bounded_on_a_character_boundary() {
        let big = "é".repeat(MAX_INPUT_BYTES);
        let rendered = render(&big);
        assert!(rendered.truncated);
        assert!(std::str::from_utf8(rendered.html.as_bytes()).is_ok());
    }
}
