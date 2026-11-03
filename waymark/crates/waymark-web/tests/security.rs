//! Hostile document content cannot execute.
//!
//! Titles cross the API as JSON text; bodies are Markdown rendered by
//! pulldown-cmark and sanitized by ammonia. The assertions are on the actual
//! bytes the service returns, not on the renderer's configuration.

mod support;

use support::{assets, fixture, get, item, seed, start};

const HOSTILE_TITLE: &str = "<script>alert('title')</script><img src=x onerror=alert('title')>";

const HOSTILE_BODY: &str = "# Heading\n\n\
<script>alert('body')</script>\n\
<img src=x onerror=alert('img')>\n\
<a href=\"javascript:alert('raw-link')\">raw link</a>\n\
[click](javascript:alert('md-link'))\n\
![image](javascript:alert('image'))\n\
<iframe src=\"https://evil.example\"></iframe>\n\
<object data=\"https://evil.example\"></object>\n\
<style>body { display: none; }</style>\n\
<svg onload=alert('svg')></svg>\n\n\
[ordinary](https://example.com/document)\n";

#[tokio::test(flavor = "multi_thread")]
async fn hostile_titles_bodies_and_links_cannot_execute() {
    let fixture = fixture();
    let mut document = item("wm-000000000001", "bug", HOSTILE_TITLE);
    document.body = HOSTILE_BODY.to_owned();
    seed(&fixture.project, &document);
    let assets = assets();
    let running = start(&fixture, assets.path()).await;

    let detail = get(running.addr(), "/api/item/wm-000000000001").json();
    assert_eq!(detail["ok"], true);

    // The title is data, never markup: it comes back exactly as written.
    assert_eq!(detail["data"]["title"], HOSTILE_TITLE);

    let html = detail["data"]["body_html"].as_str().unwrap();
    let lowered = html.to_ascii_lowercase();
    // No `javascript:` URL survives, in a link or in escaped text.
    assert!(
        !lowered.contains("javascript:"),
        "rendered body must not contain a javascript: URL: {html}"
    );
    // Only real markup is checked for scripts and event-handler attributes:
    // escaped text such as `&lt;img onerror=…&gt;` is inert data, not a tag.
    for tag in tags(html) {
        for element in ["script", "style", "iframe", "object", "embed", "svg"] {
            assert!(
                !tag.starts_with(&format!("<{element}"))
                    && !tag.starts_with(&format!("</{element}")),
                "rendered body must not contain a <{element}> tag: {html}"
            );
        }
        for attribute in ["onerror", "onload", "onclick", "onmouseover"] {
            assert!(
                !tag.contains(attribute),
                "rendered body must not contain a {attribute} attribute: {html}"
            );
        }
    }
    // A harmless link survives, with the sanitizer's rel hardening.
    assert!(lowered.contains("href=\"https://example.com/document\""));
    assert!(lowered.contains("rel=\"noopener noreferrer\""));

    running.shutdown().await.unwrap();
}

/// Every actual markup tag in `html` (escaped `&lt;` is not a tag).
fn tags(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        found.push(rest[start..start + end + 1].to_ascii_lowercase());
        rest = &rest[start + end + 1..];
    }
    found
}
