//! Entity and numeric character references
//!
//! `&#123;`, `&#xAF;`, `&quot;`
//!
//! <https://spec.commonmark.org/0.30/#entity-and-numeric-character-references>
use once_cell::sync::Lazy;
use regex::Regex;

use crate::common::utils::{decode_numeric_entity, get_entity_from_str};
use crate::parser::inline::{InlineRule, InlineState, TextSpecial};
use crate::{MarkdownParser, Node};

pub fn add(md: &mut MarkdownParser) {
    md.inline.add_rule::<EntityScanner>();
}

/// The inline entity grammar — the single source of truth for the `&…;`
/// spans the scanner consumes. Everything that has to recognise those spans
/// is built from these two pattern strings, so the grammar cannot drift
/// (issue #234): `common::utils::decode_entity_at_start` reuses the anchored
/// `Regex` objects below for the `RawValueMap` lock-step in `supramark.rs`,
/// and `common::utils::unescape_all` (fence info strings, link
/// destinations/titles) composes the same bodies into its unanchored
/// alternating pattern. A grammar copy that accepts more mis-models a
/// literal `&…;` as decoded and sinks the whole text run to plain text,
/// silently dropping every `$math$` / `[^footnote]` in it. Do NOT copy
/// these strings — import them.
pub(crate) const DIGITAL_ENTITY_PATTERN: &str = r"&#((?:x[a-f0-9]{1,6}|[0-9]{1,7}));";
pub(crate) const NAMED_ENTITY_PATTERN: &str = r"&([a-z][a-z0-9]{1,31});";

pub(crate) static DIGITAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("(?i)^{DIGITAL_ENTITY_PATTERN}")).unwrap());

pub(crate) static NAMED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("(?i)^{NAMED_ENTITY_PATTERN}")).unwrap());

#[doc(hidden)]
pub struct EntityScanner;

impl EntityScanner {
    fn parse_digital_entity(state: &mut InlineState) -> Option<(Node, usize)> {
        let capture = DIGITAL_RE.captures(&state.src[state.pos..])?;
        let entity_len = capture[0].len();
        let content_str: String =
            decode_numeric_entity(capture.get(1).unwrap().as_str()).into();

        let markup_str = capture[0].to_owned();

        let node = Node::new(TextSpecial {
            content: content_str,
            markup: markup_str,
            info: "entity",
        });
        Some((node, entity_len))
    }

    fn parse_named_entity(state: &mut InlineState) -> Option<(Node, usize)> {
        let capture = NAMED_RE.captures(&state.src[state.pos..])?;
        let str = get_entity_from_str(&capture[0])?;
        let entity_len = capture[0].len();
        let markup_str = capture[0].to_owned();
        let content_str = (*str).to_owned();

        let node = Node::new(TextSpecial {
            content: content_str,
            markup: markup_str,
            info: "entity",
        });
        Some((node, entity_len))
    }
}

impl InlineRule for EntityScanner {
    const MARKER: char = '&';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '&' {
            return None;
        }

        if let Some('#') = chars.next() {
            Self::parse_digital_entity(state)
        } else {
            Self::parse_named_entity(state)
        }
    }
}
