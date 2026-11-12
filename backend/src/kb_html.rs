//! HTML of knowledge base pages: the editor's output is cleaned to a fixed
//! set of tags, attributes and inline styles (colors, alignment, column
//! widths); images may only point to files of the knowledge base. Also the
//! plain text of a page (search, snippets) and the one-time conversion of
//! Markdown pages.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Tags the editor produces.
const TAGS: &[&str] = &[
    "p", "br", "h1", "h2", "h3", "strong", "b", "em", "i", "u", "s", "del", "span", "mark", "code", "pre", "blockquote",
    "hr", "ul", "ol", "li", "label", "input", "div", "a", "img", "table", "colgroup", "col", "thead", "tbody", "tr", "th",
    "td", "sub", "sup",
];

/// CSS properties kept in `style`.
const STYLE_PROPS: &[&str] = &["color", "background-color", "text-align", "width", "min-width"];

fn clean_style(v: &str) -> String {
    v.split(';')
        .filter_map(|decl| {
            let (prop, val) = decl.split_once(':')?;
            let prop = prop.trim().to_ascii_lowercase();
            let val = val.trim();
            let safe = !val.is_empty()
                && val.len() <= 40
                && val.chars().all(|c| c.is_ascii_alphanumeric() || " #(),.%-".contains(c))
                && !val.to_ascii_lowercase().contains("url");
            (STYLE_PROPS.contains(&prop.as_str()) && safe).then(|| format!("{prop}: {val}"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn builder() -> &'static ammonia::Builder<'static> {
    static B: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    B.get_or_init(|| {
        let mut b = ammonia::Builder::empty();
        b.tags(TAGS.iter().copied().collect::<HashSet<_>>())
            .generic_attributes(["style", "class"].into_iter().collect())
            .generic_attribute_prefixes(["data-"].into_iter().collect())
            .add_tag_attributes("a", &["href", "title", "target"])
            .add_tag_attributes("img", &["src", "alt", "title", "width", "height"])
            .add_tag_attributes("input", &["type", "checked", "disabled"])
            .add_tag_attributes("td", &["colspan", "rowspan", "colwidth"])
            .add_tag_attributes("th", &["colspan", "rowspan", "colwidth"])
            .add_tag_attributes("ol", &["start", "type"])
            .url_schemes(["http", "https", "mailto", "tel"].into_iter().collect())
            .url_relative(ammonia::UrlRelative::PassThrough)
            .link_rel(Some("noopener noreferrer"))
            .attribute_filter(|el, attr, val| match (el, attr) {
                (_, "style") => {
                    let s = clean_style(val);
                    (!s.is_empty()).then(|| s.into())
                }
                // Pictures only from the knowledge base's own storage.
                ("img", "src") => val.starts_with("/api/kb/files/").then(|| val.into()),
                ("input", "type") => (val == "checkbox").then(|| val.into()),
                ("a", "target") => (val == "_blank").then(|| val.into()),
                (_, "class") => val
                    .split_whitespace()
                    .all(|c| c.len() < 40 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
                    .then(|| val.into()),
                _ => Some(val.into()),
            });
        b
    })
}

/// Cleaned HTML of a page.
pub fn sanitize(html: &str) -> String {
    builder().clean(html).to_string()
}

/// Markdown (pages written before the visual editor) to cleaned HTML.
pub fn from_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    // «breaks: true» of the old viewer: single line breaks were kept.
    let md = md.replace("\r\n", "\n");
    let parser = Parser::new_ext(&md, opts).map(|e| match e {
        pulldown_cmark::Event::SoftBreak => pulldown_cmark::Event::HardBreak,
        e => e,
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    sanitize(&tasks_to_editor(&out))
}

/// GitHub task list items (`<li><input type="checkbox" disabled checked>`)
/// to the editor's task list markup.
fn tasks_to_editor(html: &str) -> String {
    if !html.contains("type=\"checkbox\"") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    for (i, part) in html.split("<li><input ").enumerate() {
        if i == 0 {
            out.push_str(part);
            continue;
        }
        let checked = part.split('>').next().is_some_and(|a| a.contains("checked"));
        let rest = part.split_once('>').map(|(_, r)| r).unwrap_or("");
        let (text, tail) = rest.split_once("</li>").unwrap_or((rest, ""));
        out.push_str(&format!(
            "<li data-type=\"taskItem\" data-checked=\"{checked}\"><label><input type=\"checkbox\"{}></label><div><p>{}</p></div></li>{tail}",
            if checked { " checked" } else { "" },
            text.trim()
        ));
    }
    // The list itself.
    out.replace("<ul>\n<li data-type=\"taskItem\"", "<ul data-type=\"taskList\">\n<li data-type=\"taskItem\"")
}

/// Plain text of a page: block ends become line breaks, entities decoded.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            if matches!(name.as_str(), "p" | "br" | "h1" | "h2" | "h3" | "li" | "tr" | "div" | "blockquote" | "pre" | "hr") {
                out.push('\n');
            } else if matches!(name.as_str(), "td" | "th") {
                out.push(' ');
            }
            // A file card is text too (its name is inside the link).
        } else if c == '&' {
            let mut ent = String::new();
            while let Some(&n) = chars.peek() {
                if n == ';' || ent.len() > 8 {
                    break;
                }
                ent.push(n);
                chars.next();
            }
            if chars.peek() == Some(&';') {
                chars.next();
            }
            out.push_str(match ent.as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "#39" | "apos" => "'",
                "nbsp" => " ",
                _ => " ",
            });
        } else {
            out.push(c);
        }
    }
    // Collapse blank lines and spaces.
    out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_editor_markup_and_drops_scripts() {
        let html = r##"<h2>Заголовок</h2><p>Текст <span style="color: #b42318; position: fixed">красный</span>
            <mark data-color="#fdecc8" style="background-color: #fdecc8; color: inherit">фон</mark>
            <a href="javascript:alert(1)">плохо</a> <a href="/kb?p=1">страница</a></p>
            <img src="https://evil/x.png"><img src="/api/kb/files/00000000-0000-0000-0000-000000000001">
            <script>alert(1)</script><p onclick="x()">клик</p><span style="background: url(x)">u</span>
            <ul data-type="taskList"><li data-type="taskItem" data-checked="true"><label><input type="checkbox" checked></label><div><p>пункт</p></div></li></ul>
            <table style="min-width: 75px"><tbody><tr><td colspan="2" colwidth="120">ячейка</td></tr></tbody></table>"##;
        let s = sanitize(html);
        assert!(s.contains("<h2>Заголовок</h2>"));
        assert!(s.contains(r##"style="color: #b42318""##), "{s}");
        assert!(!s.contains("position"));
        assert!(s.contains(r##"data-color="#fdecc8""##));
        assert!(!s.contains("javascript"));
        assert!(s.contains(r#"href="/kb?p=1""#));
        assert!(!s.contains("evil"));
        assert!(s.contains(r#"src="/api/kb/files/00000000-0000-0000-0000-000000000001""#));
        assert!(!s.contains("script") && !s.contains("onclick") && !s.contains("url("));
        assert!(s.contains(r#"<input type="checkbox" checked"#) && s.contains(r#"data-type="taskItem""#));
        assert!(s.contains(r#"colspan="2""#) && s.contains(r#"colwidth="120""#));
    }

    #[test]
    fn markdown_to_html_and_text() {
        let h = from_markdown("# Регламенты\n\nОбщие **правила**.\nВторая строка\n\n- [x] сделано\n- [ ] нет\n\n| А | Б |\n|---|---|\n| 1 | 2 |");
        assert!(h.contains("<h1>Регламенты</h1>") && h.contains("<strong>правила</strong>") && h.contains("<br>"), "{h}");
        assert!(h.contains(r#"<ul data-type="taskList">"#) && h.contains(r#"data-checked="true""#), "{h}");
        assert!(h.contains("<table>"));
        let t = to_text(&h);
        assert!(t.starts_with("Регламенты\nОбщие правила."), "{t}");
        assert!(t.contains("сделано") && !t.contains('<'));
        assert_eq!(to_text("<p>A &amp; B&nbsp;&lt;C&gt;</p>"), "A & B <C>");
    }
}
