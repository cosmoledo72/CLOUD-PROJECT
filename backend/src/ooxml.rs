//! Placeholder substitution in Office Open XML documents (DOCX, XLSX).
//!
//! Placeholders look like `{{project.code}}` (spaces inside the braces are
//! allowed). Word and Excel frequently split typed text into several runs
//! (`<w:t>` / `<t>` nodes) because of spell-check marks, revision IDs or
//! formatting changes, so a placeholder may be spread over several text
//! nodes. The engine therefore:
//!
//! 1. collects every text node of a part in document order;
//! 2. concatenates their (XML-unescaped) contents;
//! 3. finds placeholders in the concatenated text;
//! 4. writes each replacement into the node where the placeholder starts and
//!    removes the consumed characters from the following nodes.
//!
//! Formatting of the first run of a placeholder is kept for the inserted value.

use std::{
    collections::{BTreeSet, HashMap},
    io::{Cursor, Read, Write},
    sync::LazyLock,
};

use anyhow::{bail, Context, Result};
use regex::Regex;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Document flavour; decides which parts and text nodes are processed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Docx,
    Xlsx,
}

impl Kind {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "docx" => Some(Kind::Docx),
            "xlsx" => Some(Kind::Xlsx),
            _ => None,
        }
    }

    /// Main part that must exist for the file to be a valid document of this kind.
    fn main_part(self) -> &'static str {
        match self {
            Kind::Docx => "word/document.xml",
            Kind::Xlsx => "xl/workbook.xml",
        }
    }

    /// Parts whose text is processed.
    fn is_text_part(self, name: &str) -> bool {
        match self {
            Kind::Docx => {
                name == "word/document.xml"
                    || name == "word/footnotes.xml"
                    || name == "word/endnotes.xml"
                    || (name.starts_with("word/header") && name.ends_with(".xml"))
                    || (name.starts_with("word/footer") && name.ends_with(".xml"))
            }
            Kind::Xlsx => name == "xl/sharedStrings.xml" || (name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml")),
        }
    }

    fn text_node_re(self) -> &'static Regex {
        static DOCX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)(<w:t(?:\s[^>]*)?>)(.*?)</w:t>").unwrap());
        static XLSX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)(<t(?:\s[^>]*)?>)(.*?)</t>").unwrap());
        match self {
            Kind::Docx => &DOCX,
            Kind::Xlsx => &XLSX,
        }
    }
}

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([A-Za-z][A-Za-z0-9_.]{0,80})\s*\}\}").unwrap());

/// Maximum uncompressed size of a single XML part we are willing to process
/// (protection against zip bombs).
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

struct TextNode {
    /// Byte range of the opening tag in the XML.
    open: std::ops::Range<usize>,
    /// Byte range of the node content in the XML.
    content: std::ops::Range<usize>,
    /// Unescaped text.
    text: String,
}

fn text_nodes(kind: Kind, xml: &str) -> Vec<TextNode> {
    kind.text_node_re()
        .captures_iter(xml)
        .map(|c| {
            let open = c.get(1).expect("group 1");
            let content = c.get(2).expect("group 2");
            TextNode { open: open.range(), content: content.range(), text: unescape(content.as_str()) }
        })
        .collect()
}

/// Placeholder keys (normalised to lowercase) used in one XML part.
fn part_placeholders(kind: Kind, xml: &str, out: &mut BTreeSet<String>) {
    let joined: String = text_nodes(kind, xml).into_iter().map(|n| n.text).collect();
    for c in PLACEHOLDER.captures_iter(&joined) {
        out.insert(c[1].to_lowercase());
    }
}

/// Replace placeholders in one XML part. Unknown keys are replaced with `""`
/// and reported in `missing`.
fn substitute_part(kind: Kind, xml: &str, values: &HashMap<String, String>, missing: &mut BTreeSet<String>) -> Option<String> {
    let nodes = text_nodes(kind, xml);
    if nodes.is_empty() {
        return None;
    }
    // Concatenate texts, remembering which node owns each byte.
    let mut joined = String::new();
    let mut owner: Vec<usize> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        joined.push_str(&n.text);
        owner.extend(std::iter::repeat_n(i, n.text.len()));
    }
    if !PLACEHOLDER.is_match(&joined) {
        return None;
    }

    // Rebuild every node's text.
    let mut out: Vec<String> = vec![String::new(); nodes.len()];
    let mut pos = 0;
    for m in PLACEHOLDER.captures_iter(&joined) {
        let whole = m.get(0).expect("match");
        // Copy text before the placeholder to its owners (char by char keeps UTF-8 boundaries).
        for (off, ch) in joined[pos..whole.start()].char_indices() {
            out[owner[pos + off]].push(ch);
        }
        let key = m[1].to_lowercase();
        let value = match values.get(&key) {
            Some(v) if !v.is_empty() => v.clone(),
            Some(_) => {
                missing.insert(key);
                String::new()
            }
            None => {
                missing.insert(key);
                String::new()
            }
        };
        out[owner[whole.start()]].push_str(&value);
        pos = whole.end();
    }
    for (off, ch) in joined[pos..].char_indices() {
        out[owner[pos + off]].push(ch);
    }

    // Write changed nodes back, from the end so byte offsets stay valid.
    let mut xml = xml.to_string();
    for (i, n) in nodes.iter().enumerate().rev() {
        if out[i] == n.text {
            continue;
        }
        let mut content = escape(&out[i]);
        if kind == Kind::Docx && content.contains('\n') {
            // Line breaks inside a Word run.
            content = content.replace("\r\n", "\n").replace('\n', "</w:t><w:br/><w:t xml:space=\"preserve\">");
        }
        xml.replace_range(n.content.clone(), &content);
        let open = &xml[n.open.clone()];
        if !open.contains("xml:space") {
            let tag = if kind == Kind::Docx { "<w:t xml:space=\"preserve\">" } else { "<t xml:space=\"preserve\">" };
            xml.replace_range(n.open.clone(), tag);
        }
    }
    Some(xml)
}

fn open_archive(data: &[u8], kind: Kind) -> Result<ZipArchive<Cursor<&[u8]>>> {
    let archive = ZipArchive::new(Cursor::new(data)).context("not a valid Office document (zip)")?;
    if archive.index_for_name(kind.main_part()).is_none() {
        bail!("the file is not a valid .{} document", if kind == Kind::Docx { "docx" } else { "xlsx" });
    }
    Ok(archive)
}

fn read_part(archive: &mut ZipArchive<Cursor<&[u8]>>, index: usize) -> Result<(String, Option<String>)> {
    let mut file = archive.by_index(index)?;
    let name = file.name().to_string();
    if file.size() > MAX_PART_BYTES {
        bail!("document part {name} is too large");
    }
    let mut buf = String::new();
    file.by_ref().take(MAX_PART_BYTES).read_to_string(&mut buf).with_context(|| format!("read {name}"))?;
    Ok((name, Some(buf)))
}

/// Validate a template and list its placeholder keys.
pub fn scan(data: &[u8], kind: Kind) -> Result<Vec<String>> {
    let mut archive = open_archive(data, kind)?;
    let mut keys = BTreeSet::new();
    for i in 0..archive.len() {
        let name = archive.by_index_raw(i)?.name().to_string();
        if kind.is_text_part(&name) {
            let (_, xml) = read_part(&mut archive, i)?;
            part_placeholders(kind, xml.as_deref().unwrap_or_default(), &mut keys);
        }
    }
    Ok(keys.into_iter().collect())
}

/// Produce a filled-in copy of a template. Returns the document and the keys
/// that had no value.
pub fn render(data: &[u8], kind: Kind, values: &HashMap<String, String>) -> Result<(Vec<u8>, Vec<String>)> {
    let mut archive = open_archive(data, kind)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::with_capacity(data.len() + 4096)));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut missing = BTreeSet::new();

    for i in 0..archive.len() {
        let name = archive.by_index_raw(i)?.name().to_string();
        if kind.is_text_part(&name) {
            let (_, xml) = read_part(&mut archive, i)?;
            let xml = xml.unwrap_or_default();
            let new_xml = substitute_part(kind, &xml, values, &mut missing);
            writer.start_file(name.as_str(), options)?;
            writer.write_all(new_xml.as_deref().unwrap_or(&xml).as_bytes())?;
        } else {
            writer.raw_copy_file(archive.by_index_raw(i)?)?;
        }
    }
    let bytes = writer.finish()?.into_inner();
    Ok((bytes, missing.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn replaces_placeholder_inside_one_run() {
        let xml = r#"<w:p><w:r><w:t>Шифр: {{project.code}}</w:t></w:r></w:p>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Docx, xml, &vals(&[("project.code", "125-24-ТО")]), &mut missing).unwrap();
        assert!(out.contains("Шифр: 125-24-ТО"), "{out}");
        assert!(missing.is_empty());
    }

    #[test]
    fn replaces_placeholder_split_across_runs() {
        let xml = r#"<w:p><w:r><w:t>Объект: {{</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>project.</w:t></w:r><w:r><w:t xml:space="preserve">object_name}} (корп.)</w:t></w:r></w:p>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Docx, xml, &vals(&[("project.object_name", "Здание & <склад>")]), &mut missing).unwrap();
        assert!(out.contains("Объект: Здание &amp; &lt;склад&gt;"), "{out}");
        assert!(out.contains(r#"<w:t xml:space="preserve"></w:t>"#), "middle run emptied: {out}");
        assert!(out.contains(" (корп.)"), "{out}");
        assert!(!out.contains("{{") && !out.contains("}}"));
    }

    #[test]
    fn several_placeholders_and_spaces() {
        let xml = r#"<w:t>{{ date }} — {{project.client}}/{{PROJECT.CODE}}</w:t>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Docx, xml, &vals(&[("date", "10.11.2026"), ("project.code", "X1")]), &mut missing).unwrap();
        assert!(out.contains("10.11.2026 — /X1"), "{out}");
        assert_eq!(missing.into_iter().collect::<Vec<_>>(), vec!["project.client".to_string()]);
    }

    #[test]
    fn multiline_value_becomes_line_breaks() {
        let xml = r#"<w:r><w:t>{{project.object_address}}</w:t></w:r>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Docx, xml, &vals(&[("project.object_address", "г. Москва\nул. Ленина, 1")]), &mut missing).unwrap();
        assert!(out.contains("г. Москва</w:t><w:br/><w:t xml:space=\"preserve\">ул. Ленина, 1"), "{out}");
    }

    #[test]
    fn does_not_touch_other_tags() {
        let xml = r#"<w:tbl><w:tc><w:tab/><w:t>{{year}}</w:t></w:tc></w:tbl>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Docx, xml, &vals(&[("year", "2026")]), &mut missing).unwrap();
        assert_eq!(out, r#"<w:tbl><w:tc><w:tab/><w:t xml:space="preserve">2026</w:t></w:tc></w:tbl>"#);
    }

    #[test]
    fn untouched_part_returns_none() {
        let mut missing = BTreeSet::new();
        assert!(substitute_part(Kind::Docx, "<w:t>plain {text}</w:t>", &HashMap::new(), &mut missing).is_none());
    }

    #[test]
    fn xlsx_shared_strings() {
        let xml = r#"<sst><si><t>{{project.code}}</t></si><si><r><t>Заказчик: {{proj</t></r><r><t>ect.client}}</t></r></si></sst>"#;
        let mut missing = BTreeSet::new();
        let out = substitute_part(Kind::Xlsx, xml, &vals(&[("project.code", "A-1"), ("project.client", "ООО")]), &mut missing).unwrap();
        assert!(out.contains(">A-1</t>") && out.contains("Заказчик: ООО"), "{out}");
    }

    #[test]
    fn roundtrip_real_docx() {
        // Build a minimal DOCX in memory.
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let o = SimpleFileOptions::default();
        w.start_file("[Content_Types].xml", o).unwrap();
        w.write_all(b"<Types/>").unwrap();
        w.start_file("word/document.xml", o).unwrap();
        w.write_all(r#"<w:document><w:body><w:p><w:r><w:t>{{project.name}}</w:t></w:r></w:p></w:body></w:document>"#.as_bytes()).unwrap();
        w.start_file("word/footer1.xml", o).unwrap();
        w.write_all(r#"<w:ftr><w:p><w:r><w:t>{{year}}</w:t></w:r></w:p></w:ftr>"#.as_bytes()).unwrap();
        let docx = w.finish().unwrap().into_inner();

        assert_eq!(scan(&docx, Kind::Docx).unwrap(), vec!["project.name", "year"]);
        let (out, missing) = render(&docx, Kind::Docx, &vals(&[("project.name", "Корпус 12")])).unwrap();
        assert_eq!(missing, vec!["year"]);
        let mut a = ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
        let mut s = String::new();
        a.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
        assert!(s.contains("Корпус 12"));
        assert!(scan(b"not a zip", Kind::Docx).is_err());
    }
}
