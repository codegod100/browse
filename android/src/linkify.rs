//! Detect bare `http(s)://` URLs in plain text and render them as clickable links.

use eframe::egui::{self, CursorIcon, OpenUrl, RichText, Sense};
use vidya::Theme;

/// A run of plain text or a bare URL within a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    Text(&'a str),
    Url(&'a str),
}

/// Split `text` into plain runs and bare `http://` / `https://` URLs.
pub fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some((start, end)) = next_url(text, cursor) {
        if start > cursor {
            out.push(Segment::Text(&text[cursor..start]));
        }
        out.push(Segment::Url(&text[start..end]));
        cursor = end;
    }
    if cursor < text.len() {
        out.push(Segment::Text(&text[cursor..]));
    }
    out
}

/// True when `text` itself is a single openable URL (optionally surrounded by whitespace).
pub fn is_url(text: &str) -> bool {
    let t = text.trim();
    matches!(segments(t).as_slice(), [Segment::Url(u)] if *u == t)
}

/// Render a single clickable URL with accent styling.
pub fn link(ui: &mut egui::Ui, th: &Theme, url: &str) {
    let response = ui
        .add(
            egui::Label::new(
                RichText::new(url)
                    .size(th.type_scale.body)
                    .color(th.palette.accent)
                    .underline(),
            )
            .sense(Sense::click())
            .wrap(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(url.to_owned()));
    }
}

/// Like [`link`], but caption-sized for secondary/dim lines.
pub fn dim_link(ui: &mut egui::Ui, th: &Theme, url: &str) {
    let response = ui
        .add(
            egui::Label::new(
                RichText::new(url)
                    .size(th.type_scale.caption)
                    .color(th.palette.accent)
                    .underline(),
            )
            .sense(Sense::click())
            .wrap(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(url.to_owned()));
    }
}

/// Body text that auto-links any bare URLs. Falls back to a plain label when none.
pub fn body(ui: &mut egui::Ui, th: &Theme, text: &str) {
    render_mixed(ui, th, text, th.type_scale.body, th.palette.text, false);
}

/// Dim/caption text that auto-links any bare URLs.
pub fn dim(ui: &mut egui::Ui, th: &Theme, text: &str) {
    render_mixed(
        ui,
        th,
        text,
        th.type_scale.caption,
        th.palette.text_secondary,
        true,
    );
}

fn render_mixed(
    ui: &mut egui::Ui,
    th: &Theme,
    text: &str,
    size: f32,
    color: egui::Color32,
    dim_links: bool,
) {
    let parts = segments(text);
    if !parts.iter().any(|s| matches!(s, Segment::Url(_))) {
        ui.add(
            egui::Label::new(RichText::new(text).size(size).color(color)).wrap(),
        );
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for part in parts {
            match part {
                Segment::Text(t) if t.is_empty() => {}
                Segment::Text(t) => {
                    ui.add(
                        egui::Label::new(RichText::new(t).size(size).color(color)).wrap(),
                    );
                }
                Segment::Url(url) => {
                    if dim_links {
                        dim_link(ui, th, url);
                    } else {
                        link(ui, th, url);
                    }
                }
            }
        }
    });
}

fn next_url(text: &str, from: usize) -> Option<(usize, usize)> {
    let rest = text.get(from..)?;
    let https = rest.find("https://").map(|i| (i, 8usize));
    let http = rest.find("http://").map(|i| (i, 7usize));
    let (rel, scheme_len) = match (https, http) {
        (Some((i, l)), Some((j, k))) => {
            if i <= j {
                (i, l)
            } else {
                (j, k)
            }
        }
        (Some(pair), None) | (None, Some(pair)) => pair,
        (None, None) => return None,
    };

    let start = from + rel;
    let mut end = start + scheme_len;
    // Need at least one host character after the scheme.
    if end >= text.len() || !is_url_body_char(text[end..].chars().next()?) {
        return next_url(text, start + scheme_len);
    }

    while end < text.len() {
        let c = text[end..].chars().next()?;
        if is_url_body_char(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }

    end = trim_trailing_punct(text, start, end);
    if end <= start + scheme_len {
        return next_url(text, start + scheme_len);
    }
    Some((start, end))
}

fn is_url_body_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '[' | ']' | '@' | '!' | '$' | '&'
                | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%'
        )
}

fn trim_trailing_punct(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let c = text[..end].chars().last().unwrap();
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"') {
            end -= c.len_utf8();
            continue;
        }
        if c == ')' {
            let slice = &text[start..end];
            let opens = slice.chars().filter(|&ch| ch == '(').count();
            let closes = slice.chars().filter(|&ch| ch == ')').count();
            if closes > opens {
                end -= c.len_utf8();
                continue;
            }
        }
        if c == ']' {
            let slice = &text[start..end];
            let opens = slice.chars().filter(|&ch| ch == '[').count();
            let closes = slice.chars().filter(|&ch| ch == ']').count();
            if closes > opens {
                end -= c.len_utf8();
                continue;
            }
        }
        break;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_bare_https() {
        let parts = segments("see https://example.com/a for more");
        assert_eq!(
            parts,
            vec![
                Segment::Text("see "),
                Segment::Url("https://example.com/a"),
                Segment::Text(" for more"),
            ]
        );
    }

    #[test]
    fn trims_trailing_punctuation() {
        let parts = segments("log https://ci.example/run/1.");
        assert_eq!(
            parts,
            vec![
                Segment::Text("log "),
                Segment::Url("https://ci.example/run/1"),
                Segment::Text("."),
            ]
        );
    }

    #[test]
    fn whole_string_url() {
        assert!(is_url("https://boxci.boxd.sh/runs/abc"));
        assert!(!is_url("log https://boxci.boxd.sh/runs/abc"));
    }

    #[test]
    fn keeps_balanced_parens() {
        let parts = segments("https://example.com/foo_(bar)");
        assert_eq!(parts, vec![Segment::Url("https://example.com/foo_(bar)")]);
    }
}
