//! Reading EPUB 2 and EPUB 3 books: the container, the package document
//! (metadata, manifest, spine) and the table of contents, from the EPUB 3
//! navigation document or the EPUB 2 NCX.

use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

use roxmltree::{Document, Node, ParsingOptions};

/// A book read into memory.
pub struct Book {
    /// The title (`dc:title`), or the file name.
    pub title: String,
    /// The authors (`dc:creator`), joined.
    pub author: String,
    /// `2` or `3`, from the package's `version`.
    pub version: String,
    /// The language (`dc:language`), for the page's `lang`.
    pub language: String,
    /// The reading order: each item's path in the archive.
    pub spine: Vec<String>,
    /// The table of contents, flattened with depths.
    pub toc: Vec<TocEntry>,
    /// Every file of the archive by path, with its media type.
    files: HashMap<String, Arc<Vec<u8>>>,
    media_types: HashMap<String, String>,
}

/// One entry of the table of contents.
#[derive(Clone, Debug, PartialEq)]
pub struct TocEntry {
    /// The label shown.
    pub label: String,
    /// The spine index it opens, when its document is in the spine.
    pub chapter: Option<usize>,
    /// The fragment inside the document (`#id`, without `#`).
    pub fragment: Option<String>,
    /// 0 for top-level entries.
    pub depth: usize,
}

impl Book {
    /// Reads the book at `path`.
    pub fn open(path: &Path) -> Result<Book, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("파일을 읽을 수 없습니다: {e}"))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Book::from_bytes(bytes, &name)
    }

    /// Reads a book from the bytes of its archive; `name` is the title when
    /// the book has none.
    pub fn from_bytes(bytes: Vec<u8>, name: &str) -> Result<Book, String> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| format!("EPUB(ZIP) 파일이 아닙니다: {e}"))?;
        let mut files = HashMap::new();
        for i in 0..zip.len() {
            let Ok(mut entry) = zip.by_index(i) else { continue };
            if entry.is_dir() {
                continue;
            }
            let mut data = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut data).is_ok() {
                files.insert(entry.name().trim_start_matches('/').to_owned(), Arc::new(data));
            }
        }

        let container = text_of(&files, "META-INF/container.xml")
            .ok_or("META-INF/container.xml 이 없습니다")?;
        let container = parse_xml(&container).map_err(|e| format!("container.xml: {e}"))?;
        let opf_path = container
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .ok_or("container.xml 에 rootfile 이 없습니다")?
            .to_owned();
        let opf_text = text_of(&files, &opf_path).ok_or(format!("{opf_path} 이 없습니다"))?;
        let opf = parse_xml(&opf_text).map_err(|e| format!("{opf_path}: {e}"))?;
        let package = opf.root_element();
        let version = package.attribute("version").unwrap_or("2.0").to_owned();

        let meta = |name: &str| -> Vec<String> {
            package
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == name)
                .filter_map(|n| node_text(n))
                .collect()
        };
        let title = meta("title").into_iter().next().unwrap_or_else(|| name.to_owned());
        let author = meta("creator").join(", ");
        let language = meta("language").into_iter().next().unwrap_or_default();

        struct Item {
            href: String,
            media_type: String,
            properties: String,
        }
        let mut manifest: HashMap<String, Item> = HashMap::new();
        let mut media_types = HashMap::new();
        for item in package.descendants().filter(|n| n.has_tag_name("item")) {
            let (Some(id), Some(href)) = (item.attribute("id"), item.attribute("href")) else {
                continue;
            };
            let href = resolve(&opf_path, href);
            let media_type = item.attribute("media-type").unwrap_or("").to_owned();
            media_types.insert(href.clone(), media_type.clone());
            manifest.insert(
                id.to_owned(),
                Item {
                    href,
                    media_type,
                    properties: item.attribute("properties").unwrap_or("").to_owned(),
                },
            );
        }

        let spine_node = package.descendants().find(|n| n.has_tag_name("spine"));
        let mut spine: Vec<String> = spine_node
            .iter()
            .flat_map(|s| s.children().filter(|n| n.has_tag_name("itemref")))
            .filter_map(|r| manifest.get(r.attribute("idref")?))
            .filter(|item| files.contains_key(&item.href))
            .map(|item| item.href.clone())
            .collect();
        if spine.is_empty() {
            // A broken spine: read the manifest's documents in order.
            let mut docs: Vec<&Item> = manifest
                .values()
                .filter(|i| i.media_type.contains("html"))
                .collect();
            docs.sort_by(|a, b| a.href.cmp(&b.href));
            spine = docs.into_iter().map(|i| i.href.clone()).collect();
        }
        if spine.is_empty() {
            return Err("읽을 수 있는 본문이 없습니다".into());
        }
        let chapter_of: HashMap<&str, usize> =
            spine.iter().enumerate().map(|(i, p)| (p.as_str(), i)).collect();
        let entry = |label: String, target: &str, base: &str, depth: usize| {
            let (path, fragment) = split_fragment(&resolve(base, target));
            TocEntry {
                label,
                chapter: chapter_of.get(path.as_str()).copied(),
                fragment,
                depth,
            }
        };

        // EPUB 3: the navigation document's `toc` nav.
        let mut toc = Vec::new();
        if let Some(nav) = manifest
            .values()
            .find(|i| i.properties.split_whitespace().any(|p| p == "nav"))
            && let Some(text) = text_of(&files, &nav.href).map(|t| numeric_entities(&t).into_owned())
            && let Ok(doc) = parse_xml(&text)
        {
            let navs: Vec<Node> = doc.descendants().filter(|n| n.has_tag_name("nav")).collect();
            let toc_nav = navs
                .iter()
                .find(|n| n.attributes().any(|a| a.name() == "type" && a.value().contains("toc")))
                .or(navs.first());
            if let Some(list) = toc_nav.and_then(|n| n.children().find(|c| c.has_tag_name("ol"))) {
                walk_nav_list(list, 0, &mut |label, href, depth| {
                    toc.push(entry(label, href, &nav.href, depth));
                });
            }
        }
        // EPUB 2: the NCX the spine names (or any NCX).
        if toc.is_empty() {
            let ncx = spine_node
                .and_then(|s| s.attribute("toc"))
                .and_then(|id| manifest.get(id))
                .or_else(|| manifest.values().find(|i| i.media_type == "application/x-dtbncx+xml"));
            if let Some(ncx) = ncx
                && let Some(text) = text_of(&files, &ncx.href).map(|t| numeric_entities(&t).into_owned())
                && let Ok(doc) = parse_xml(&text)
                && let Some(map) = doc.descendants().find(|n| n.has_tag_name("navMap"))
            {
                walk_nav_points(map, 0, &mut |label, href, depth| {
                    toc.push(entry(label, href, &ncx.href, depth));
                });
            }
        }
        if toc.is_empty() {
            toc = spine
                .iter()
                .enumerate()
                .map(|(i, p)| TocEntry {
                    label: p.rsplit('/').next().unwrap_or(p).to_owned(),
                    chapter: Some(i),
                    fragment: None,
                    depth: 0,
                })
                .collect();
        }

        Ok(Book {
            title,
            author,
            version,
            language,
            spine,
            toc,
            files,
            media_types,
        })
    }

    /// The bytes of the file at `path`.
    pub fn file(&self, path: &str) -> Option<&Arc<Vec<u8>>> {
        self.files.get(path)
    }

    /// The file at `path` as UTF-8 text (a BOM dropped).
    pub fn text(&self, path: &str) -> Option<String> {
        text_of(&self.files, path)
    }

    /// The media type of `path`: the manifest's, else guessed from its
    /// extension.
    pub fn media_type(&self, path: &str) -> String {
        match self.media_types.get(path) {
            Some(t) if !t.is_empty() => t.clone(),
            _ => guess_media_type(path).to_owned(),
        }
    }

    /// The spine index of the document at `path`.
    pub fn chapter_of(&self, path: &str) -> Option<usize> {
        self.spine.iter().position(|p| p == path)
    }
}

/// `<li><a href>label</a><ol>…</ol></li>` of an EPUB 3 nav, depth first.
fn walk_nav_list(list: Node, depth: usize, f: &mut dyn FnMut(String, &str, usize)) {
    for li in list.children().filter(|n| n.has_tag_name("li")) {
        let link = li
            .children()
            .find(|c| c.has_tag_name("a") || c.has_tag_name("span"));
        if let Some(link) = link {
            let label = collapse(&all_text(link));
            if let Some(href) = link.attribute("href") {
                f(label, href, depth);
            }
        }
        if let Some(sub) = li.children().find(|c| c.has_tag_name("ol")) {
            walk_nav_list(sub, depth + 1, f);
        }
    }
}

/// `<navPoint><navLabel><text>…</text></navLabel><content src/>…` of an
/// NCX, depth first.
fn walk_nav_points(parent: Node, depth: usize, f: &mut dyn FnMut(String, &str, usize)) {
    for point in parent.children().filter(|n| n.has_tag_name("navPoint")) {
        let label = point
            .children()
            .find(|c| c.has_tag_name("navLabel"))
            .map(|l| collapse(&all_text(l)))
            .unwrap_or_default();
        if let Some(src) = point
            .children()
            .find(|c| c.has_tag_name("content"))
            .and_then(|c| c.attribute("src"))
        {
            f(label, src, depth);
        }
        walk_nav_points(point, depth + 1, f);
    }
}

fn all_text(node: Node) -> String {
    node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

fn node_text(node: Node) -> Option<String> {
    let text = collapse(&all_text(node));
    (!text.is_empty()).then_some(text)
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(files: &HashMap<String, Arc<Vec<u8>>>, path: &str) -> Option<String> {
    let bytes = files.get(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Parses XML with DTDs allowed (XHTML documents declare one); pass text
/// through [`numeric_entities`] first when it may use HTML's entities.
pub fn parse_xml(text: &str) -> Result<Document<'_>, roxmltree::Error> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(text, options)
}

/// `text` with HTML's named entities written as numeric character
/// references, which every XML parser reads.
pub fn numeric_entities(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('&') {
        return text.into();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .map(|i| i + 1);
        match end {
            Some(end) if end > 1 && rest[end..].starts_with(';') => {
                let name = &rest[1..end];
                match name {
                    "amp" | "lt" | "gt" | "quot" | "apos" => out.push_str(&rest[..=end]),
                    _ => match named_entity(name) {
                        Some(code) => out.push_str(&format!("&#{code};")),
                        None => out.push_str(&format!("&amp;{name};")),
                    },
                }
                rest = &rest[end + 1..];
            }
            _ if rest[1..].starts_with('#') => {
                out.push('&');
                rest = &rest[1..];
            }
            _ => {
                out.push_str("&amp;");
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.into()
}

fn named_entity(name: &str) -> Option<u32> {
    Some(match name {
        "nbsp" => 160,
        "iexcl" => 161,
        "cent" => 162,
        "pound" => 163,
        "yen" => 165,
        "sect" => 167,
        "copy" => 169,
        "laquo" => 171,
        "not" => 172,
        "shy" => 173,
        "reg" => 174,
        "deg" => 176,
        "plusmn" => 177,
        "sup2" => 178,
        "sup3" => 179,
        "middot" => 183,
        "raquo" => 187,
        "frac12" => 189,
        "iquest" => 191,
        "times" => 215,
        "divide" => 247,
        "ensp" => 8194,
        "emsp" => 8195,
        "thinsp" => 8201,
        "zwnj" => 8204,
        "zwj" => 8205,
        "ndash" => 8211,
        "mdash" => 8212,
        "lsquo" => 8216,
        "rsquo" => 8217,
        "sbquo" => 8218,
        "ldquo" => 8220,
        "rdquo" => 8221,
        "bdquo" => 8222,
        "dagger" => 8224,
        "Dagger" => 8225,
        "bull" => 8226,
        "hellip" => 8230,
        "permil" => 8240,
        "prime" => 8242,
        "Prime" => 8243,
        "lsaquo" => 8249,
        "rsaquo" => 8250,
        "euro" => 8364,
        "trade" => 8482,
        "larr" => 8592,
        "uarr" => 8593,
        "rarr" => 8594,
        "darr" => 8595,
        "harr" => 8596,
        "minus" => 8722,
        "infin" => 8734,
        "ne" => 8800,
        "le" => 8804,
        "ge" => 8805,
        _ => return None,
    })
}

/// `href` resolved against the file at `base`, as an archive path: query
/// dropped, percent-escapes decoded, `.` and `..` folded. The fragment is
/// kept.
pub fn resolve(base: &str, href: &str) -> String {
    let (href, fragment) = match href.find('#') {
        Some(i) => (&href[..i], Some(&href[i..])),
        None => (href, None),
    };
    let href = href.split('?').next().unwrap_or("");
    let href = percent_encoding::percent_decode_str(href).decode_utf8_lossy();
    let mut parts: Vec<&str> = if href.starts_with('/') || href.is_empty() {
        Vec::new()
    } else {
        base.rsplit_once('/').map(|(dir, _)| dir.split('/').collect()).unwrap_or_default()
    };
    if href.is_empty() {
        // A fragment alone: the same document.
        let mut path = base.to_owned();
        path.push_str(fragment.unwrap_or(""));
        return path;
    }
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let mut path = parts.join("/");
    path.push_str(fragment.unwrap_or(""));
    path
}

/// `path#fragment` split in two.
pub fn split_fragment(path: &str) -> (String, Option<String>) {
    match path.split_once('#') {
        Some((p, f)) => (p.to_owned(), (!f.is_empty()).then(|| f.to_owned())),
        None => (path.to_owned(), None),
    }
}

/// Whether `href` leaves the book (`https:`, `mailto:`, ...).
pub fn is_external(href: &str) -> bool {
    let Some(colon) = href.find(':') else { return false };
    let scheme = &href[..colon];
    !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && !href[..colon].contains('/')
}

fn guess_media_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "css" => "text/css",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "mp3" => "audio/mpeg",
        "mp4" | "m4v" => "video/mp4",
        "xhtml" | "html" | "htm" => "application/xhtml+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(resolve("OEBPS/content.opf", "text/ch1.xhtml"), "OEBPS/text/ch1.xhtml");
        assert_eq!(resolve("OEBPS/text/ch1.xhtml", "../images/a%20b.png"), "OEBPS/images/a b.png");
        assert_eq!(resolve("OEBPS/text/ch1.xhtml", "#note"), "OEBPS/text/ch1.xhtml#note");
        assert_eq!(resolve("content.opf", "ch1.xhtml#x"), "ch1.xhtml#x");
    }

    #[test]
    fn rewrites_named_entities() {
        assert_eq!(numeric_entities("a&nbsp;b &amp; c&hellip;"), "a&#160;b &amp; c&#8230;");
        assert_eq!(numeric_entities("x & y &#169;"), "x &amp; y &#169;");
    }

    #[test]
    fn knows_external_links() {
        assert!(is_external("https://example.com"));
        assert!(is_external("mailto:a@b.c"));
        assert!(!is_external("ch1.xhtml#a:b"));
        assert!(!is_external("text/ch1.xhtml"));
    }
}
