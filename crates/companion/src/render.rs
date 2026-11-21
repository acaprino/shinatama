//! Card rendering (FRD-0012 R41-R42): prose through the shared inert Markdown renderer, the
//! parsed block as data for chips, quiz, sources and risk markers. A parse failure never drops
//! the card: the prose is shown and the reasons are listed.
use crate::card::{CardKind, ParsedCard, parse};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedCard {
    pub html: String,
    pub truncated: bool,
    pub card: ParsedCard,
}

pub fn render(answer: &str, requested: Option<CardKind>) -> RenderedCard {
    let card = parse(answer, requested);
    let rendered = shinatama_files::markdown::render(&card.prose);
    RenderedCard {
        html: rendered.html,
        truncated: rendered.truncated,
        card,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_card_prose_renders_inert_and_the_block_is_data() {
        let answer = "# Card\n\n<img src=x onerror=alert(1)> [x](https://evil.example) ![p](https://t.example/p.gif)\n\n```mermaid\ngraph TD; A-->B\n```\n\n```shinatama-card\n{\"chips\":[\"Ports\"]}\n```";
        let card = render(answer, Some(CardKind::Learning));
        assert!(
            !card.html.contains("<img")
                && !card.html.contains("href=")
                && !card.html.contains("<script")
        );
        assert!(card.html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(
            card.html.contains("data-language=\"mermaid\""),
            "mermaid stays source"
        );
        assert!(!card.html.contains("shinatama-card"));
        assert_eq!(card.card.block.chips, ["Ports"]);
    }
}
