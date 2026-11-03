//! Safe Markdown rendering.
//!
//! The body is parsed by `pulldown-cmark` and then sanitized by `ammonia` before
//! it leaves the process. Titles are never rendered here: they cross the API as
//! JSON strings and the frontend must insert them as text.
//!
//! The sanitizer is the security boundary. Raw HTML in a document reaches
//! `ammonia` (pulldown-cmark passes it through as HTML events) and is stripped of
//! scripts, event-handler attributes, `javascript:` URLs, and every other unsafe
//! construct it knows about. We do not try to patch Markdown by hand.

use pulldown_cmark::{html, Options, Parser};

/// Render Markdown to sanitized HTML.
pub(crate) fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(markdown, options);
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);

    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .clean(&rendered)
        .to_string()
}
