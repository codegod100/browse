//! Render markdown into Vidya text widgets (headings, paragraphs, lists, code, links).

use eframe::egui::{self, CursorIcon, FontFamily, OpenUrl, RichText, Sense};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};
use vidya::{dim_label, title, title_2, Theme};

use crate::linkify;

pub fn render(ui: &mut egui::Ui, th: &Theme, markdown: &str) {
    if markdown.is_empty() || markdown == "(no README)" {
        dim_label(ui, th, "(no README)");
        return;
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(markdown, options);
    let mut inline = String::new();
    let mut list_depth: usize = 0;
    let mut in_code_block = false;
    let mut code_buf = String::new();
    let mut heading_level: Option<u8> = None;
    let mut link_url: Option<String> = None;
    let mut link_text = String::new();

    let flush_inline = |ui: &mut egui::Ui,
                        th: &Theme,
                        inline: &mut String,
                        heading_level: &mut Option<u8>,
                        list_depth: usize| {
        let text = inline.trim();
        if text.is_empty() {
            inline.clear();
            return;
        }
        let prefix = if list_depth > 0 {
            format!("{}• ", "  ".repeat(list_depth.saturating_sub(1)))
        } else {
            String::new()
        };
        let line = format!("{prefix}{text}");
        match *heading_level {
            // Headings keep title styling; still autolink bare URLs inside them.
            Some(1) => {
                if linkify::segments(&line)
                    .iter()
                    .any(|s| matches!(s, linkify::Segment::Url(_)))
                {
                    linkify::body(ui, th, &line);
                } else {
                    title(ui, th, &line);
                }
            }
            Some(_) => {
                if linkify::segments(&line)
                    .iter()
                    .any(|s| matches!(s, linkify::Segment::Url(_)))
                {
                    linkify::body(ui, th, &line);
                } else {
                    title_2(ui, th, &line);
                }
            }
            None => linkify::body(ui, th, &line),
        }
        ui.add_space(th.spacing.xs);
        inline.clear();
        *heading_level = None;
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
                heading_level = Some(match level {
                    pulldown_cmark::HeadingLevel::H1 => 1,
                    pulldown_cmark::HeadingLevel::H2 => 2,
                    _ => 3,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
                ui.add_space(th.spacing.sm);
            }
            Event::Start(Tag::Item) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
            }
            Event::End(TagEnd::Item) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
            }
            Event::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                ui.add_space(th.spacing.xs);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
                in_code_block = true;
                code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let code = code_buf.trim_end();
                if !code.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(code)
                                .font(egui::FontId::new(th.type_scale.body, FontFamily::Monospace))
                                .color(th.palette.text_secondary),
                        )
                        .wrap(),
                    );
                    ui.add_space(th.spacing.sm);
                }
                code_buf.clear();
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
                link_url = Some(cow_to_string(dest_url));
                link_text.clear();
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = link_url.take() {
                    let label = if link_text.trim().is_empty() {
                        url.clone()
                    } else {
                        link_text.trim().to_string()
                    };
                    if label == url {
                        linkify::link(ui, th, &url);
                    } else {
                        let response = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(label)
                                        .size(th.type_scale.body)
                                        .color(th.palette.accent)
                                        .underline(),
                                )
                                .sense(Sense::click())
                                .wrap(),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text(&url);
                        if response.clicked() {
                            ui.ctx().open_url(OpenUrl::new_tab(url));
                        }
                    }
                    ui.add_space(th.spacing.xs);
                }
                link_text.clear();
            }
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough) => {}
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {}
            Event::Start(Tag::BlockQuote(_)) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
            }
            Event::Code(c) => {
                if in_code_block {
                    code_buf.push_str(&c);
                } else if link_url.is_some() {
                    link_text.push('`');
                    link_text.push_str(&c);
                    link_text.push('`');
                } else {
                    inline.push('`');
                    inline.push_str(&c);
                    inline.push('`');
                }
            }
            Event::Text(t) => {
                if in_code_block {
                    code_buf.push_str(&t);
                } else if link_url.is_some() {
                    link_text.push_str(&t);
                } else {
                    inline.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else if link_url.is_some() {
                    link_text.push(' ');
                } else {
                    inline.push(' ');
                }
            }
            Event::Rule => {
                flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
                ui.separator();
                ui.add_space(th.spacing.sm);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let stripped: String = html
                    .chars()
                    .filter(|c| *c != '<' && *c != '>')
                    .collect();
                if !stripped.trim().is_empty() && !in_code_block {
                    if link_url.is_some() {
                        link_text.push_str(stripped.trim());
                    } else {
                        inline.push_str(stripped.trim());
                    }
                }
            }
            _ => {}
        }
    }
    flush_inline(ui, th, &mut inline, &mut heading_level, list_depth);
}

fn cow_to_string(c: CowStr<'_>) -> String {
    c.into_string()
}
