use std::fmt::Write;
use wasm_bindgen::JsCast;
use web_sys::Element;

pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn token(input: &str) -> Option<(usize, String)> {
    for (delimiter, tag) in [("`", "code"), ("**", "strong")] {
        if let Some(rest) = input.strip_prefix(delimiter) {
            let end = rest.find(delimiter.chars().next()?)?;
            if end == 0 || !rest[end..].starts_with(delimiter) {
                return None;
            }
            return Some((
                2 * delimiter.len() + end,
                format!("<{tag}>{}</{tag}>", esc(&rest[..end])),
            ));
        }
    }
    if let Some(rest) = input.strip_prefix('#') {
        let bytes = rest.as_bytes();
        if bytes.len() >= 6
            && bytes[..6].iter().all(u8::is_ascii_hexdigit)
            && !bytes
                .get(6)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            return Some((7, format!(r#"<span class="swatch">{}</span>"#, &input[..7])));
        }
    }
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|s| input.starts_with(s))?;
    let rest = &input[scheme.len()..];
    let len = rest
        .find(|c: char| c.is_whitespace() || "<>\"'。、".contains(c))
        .unwrap_or(rest.len());
    if len == 0 {
        return None;
    }
    let end = scheme.len() + len;
    let link = esc(&input[..end]);
    Some((
        end,
        format!(
            r#"<a href="{link}" target="_blank" rel="noopener noreferrer nofollow">{link}</a>"#
        ),
    ))
}

fn inline(mut input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    while let Some(start) = input.find(['`', '*', '#', 'h']) {
        out.push_str(&esc(&input[..start]));
        input = &input[start..];
        if let Some((width, markup)) = token(input) {
            out.push_str(&markup);
            input = &input[width..];
        } else {
            out.push(char::from(input.as_bytes()[0]));
            input = &input[1..];
        }
    }
    out.push_str(&esc(input));
    out
}

pub(super) fn message_body(body: &str) -> String {
    let mut html = String::new();
    for line in body.split('\n') {
        if let Some(quote) = line.strip_prefix("> ") {
            let _ = write!(html, "<blockquote>{}</blockquote>", inline(quote));
        } else if line.is_empty() {
            html.push_str("<div><br></div>");
        } else {
            let _ = write!(html, "<div>{}</div>", inline(line));
        }
    }
    html
}

pub fn render_swatches(root: &Element) {
    let Ok(nodes) = root.query_selector_all(".swatch:not([data-tinted])") else {
        return;
    };
    for i in 0..nodes.length() {
        let Some(el) = nodes.get(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let color = el.text_content().unwrap_or_default();
        if color
            .strip_prefix('#')
            .is_some_and(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            let _ = el.set_attribute("style", &format!("--chip:{color}"));
            let _ = el.set_attribute("data-tinted", "true");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_content_and_accepts_only_supported_markup() {
        assert_eq!(
            message_body("**<森>**\n> `&` #ABC123!"),
            "<div><strong>&lt;森&gt;</strong></div><blockquote><code>&amp;</code> <span class=\"swatch\">#ABC123</span>!</blockquote>"
        );
        assert_eq!(
            inline("javascript:alert(1) #ABC1234"),
            "javascript:alert(1) #ABC1234"
        );
        assert!(
            inline("https://example.test/?a=1&b=2。")
                .contains("href=\"https://example.test/?a=1&amp;b=2\"")
        );
        assert_eq!(inline("α*日本語 #123<"), "α*日本語 #123&lt;");
    }
}
