//! A spine document made into one self-contained HTML page for the web
//! view: its styles and images inlined (the web view has no access to the
//! archive), its links marked for the app, and the page laid out as A4
//! sheets, one CSS column per sheet.

use std::fmt::Write as _;

use base64::Engine as _;
use roxmltree::{Node, NodeType};

use crate::epub::{self, Book};

/// A4 at 96 CSS pixels per inch: 210 × 297 mm.
pub const PAGE_WIDTH: f32 = 794.0;
pub const PAGE_HEIGHT: f32 = 1123.0;

/// Where the page opens.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page of the chapter, from 0.
    Page(u32),
    /// The last page (turning back from the next chapter).
    Last,
    /// The element with this id.
    Anchor(String),
    /// A share of the chapter, 0..1 (after the text size changed).
    Fraction(f32),
}

impl Target {
    /// What the page's script goes to (its `target`, a `go` command's).
    pub fn json(&self) -> String {
        match self {
            Target::Page(p) => format!("{{\"page\":{p}}}"),
            Target::Last => "{\"last\":true}".to_owned(),
            Target::Anchor(id) => format!("{{\"anchor\":{}}}", serde_json::to_string(id).unwrap()),
            Target::Fraction(f) => format!("{{\"fraction\":{f}}}"),
        }
    }
}

/// The paper's colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    White,
    Sepia,
    Dark,
}

impl Paper {
    pub fn next(self) -> Paper {
        match self {
            Paper::White => Paper::Sepia,
            Paper::Sepia => Paper::Dark,
            Paper::Dark => Paper::White,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Paper::White => "흰 종이",
            Paper::Sepia => "세피아",
            Paper::Dark => "어두운 종이",
        }
    }

    /// Background and text colors.
    pub fn colors(self) -> (&'static str, &'static str) {
        match self {
            Paper::White => ("#ffffff", "#1a1a1a"),
            Paper::Sepia => ("#f6efdf", "#3b2f1e"),
            Paper::Dark => ("#1e1f22", "#d6d6d6"),
        }
    }
}

/// The settings a page is made with.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The text size, in percent.
    pub text_size: u32,
    pub paper: Paper,
    /// Two sheets side by side, as an open book.
    pub spread: bool,
}

/// The page showing spine document `chapter` of `book`.
pub fn chapter_page(book: &Book, chapter: usize, target: &Target, settings: &Settings) -> String {
    let path = &book.spine[chapter];
    let source = book.text(path).unwrap_or_default();
    let mut styles = String::new();
    let mut body = String::new();
    let mut body_attrs = String::new();
    let xml = epub::numeric_entities(&source);
    match epub::parse_xml(&xml) {
        Ok(doc) => {
            let root = doc.root_element();
            for node in root.descendants() {
                match node.tag_name().name() {
                    "link" if is_stylesheet(node) => {
                        if let Some(href) = node.attribute("href") {
                            let css_path = epub::resolve(path, href);
                            let css_path = epub::split_fragment(&css_path).0;
                            styles.push_str(&stylesheet(book, &css_path, 0));
                            styles.push('\n');
                        }
                    }
                    "style" if node.ancestors().any(|a| a.has_tag_name_local("head")) => {
                        styles.push_str(&rewrite_css(book, path, &all_text(node), 0));
                        styles.push('\n');
                    }
                    _ => {}
                }
            }
            if let Some(b) = root.descendants().find(|n| n.has_tag_name_local("body")) {
                for attr in b.attributes() {
                    if matches!(attr.name(), "class" | "id" | "dir" | "lang") {
                        let _ = write!(body_attrs, " {}=\"{}\"", attr.name(), escape_attr(attr.value()));
                    }
                }
                let mut writer = Writer { book, path, out: &mut body };
                for child in b.children() {
                    writer.node(child);
                }
            }
        }
        // Not well-formed XML (some EPUB 2 books): take the body as it is.
        Err(err) => {
            log::warn!("{path}: {err}; showing it as HTML");
            body = loose_body(book, path, &source);
        }
    }
    page(book, &styles, &body_attrs, &body, target, settings)
}

trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}

impl LocalName for Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name().eq_ignore_ascii_case(name)
    }
}

fn is_stylesheet(node: Node) -> bool {
    node.attribute("rel")
        .is_some_and(|r| r.split_whitespace().any(|r| r.eq_ignore_ascii_case("stylesheet")))
        && !node
            .attribute("rel")
            .is_some_and(|r| r.split_whitespace().any(|r| r.eq_ignore_ascii_case("alternate")))
}

fn all_text(node: Node) -> String {
    node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

/// Writes the XHTML tree as HTML.
struct Writer<'a> {
    book: &'a Book,
    /// The document's path, which its links resolve against.
    path: &'a str,
    out: &'a mut String,
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

impl Writer<'_> {
    fn node(&mut self, node: Node) {
        match node.node_type() {
            NodeType::Text => self.out.push_str(&escape_text(node.text().unwrap_or(""))),
            NodeType::Element => self.element(node),
            _ => {}
        }
    }

    fn element(&mut self, node: Node) {
        let name = node.tag_name().name();
        if matches!(name, "script" | "noscript" | "head" | "title" | "meta" | "base") {
            return;
        }
        if name == "style" {
            let css = rewrite_css(self.book, self.path, &all_text(node), 0);
            let _ = write!(self.out, "<style>{css}</style>");
            return;
        }
        let _ = write!(self.out, "<{name}");
        let mut ext_link = false;
        for attr in node.attributes() {
            let local = attr.name();
            let ns = attr.namespace().unwrap_or("");
            let value = attr.value();
            let qualified = match ns {
                "http://www.w3.org/1999/xlink" => format!("xlink:{local}"),
                "http://www.w3.org/XML/1998/namespace" if local == "lang" => "lang".to_owned(),
                "http://www.idpf.org/2007/ops" => format!("data-epub-{local}"),
                "" => local.to_owned(),
                _ => continue,
            };
            if qualified.starts_with("on") {
                continue;
            }
            let value = match (name, local) {
                ("a", "href") | ("area", "href") => {
                    if epub::is_external(value) {
                        ext_link = true;
                        value.to_owned()
                    } else {
                        let target = epub::resolve(self.path, value);
                        let _ = write!(self.out, " data-epub-href=\"{}\"", escape_attr(&target));
                        "#".to_owned()
                    }
                }
                ("img" | "source" | "video" | "audio" | "input" | "embed", "src")
                | ("video", "poster")
                | ("object", "data")
                | ("image" | "use" | "feImage", "href") => self.resource(value),
                (_, "style") => rewrite_css(self.book, self.path, value, 0),
                ("img" | "source", "srcset") => continue,
                _ => value.to_owned(),
            };
            let _ = write!(self.out, " {qualified}=\"{}\"", escape_attr(&value));
        }
        if ext_link {
            self.out.push_str(" data-ext=\"1\"");
        }
        self.out.push('>');
        if VOID.contains(&name) {
            return;
        }
        for child in node.children() {
            self.node(child);
        }
        let _ = write!(self.out, "</{name}>");
    }

    /// A reference to a file of the book as a `data:` URL.
    fn resource(&self, href: &str) -> String {
        if href.starts_with("data:") || epub::is_external(href) {
            return href.to_owned();
        }
        let path = epub::split_fragment(&epub::resolve(self.path, href)).0;
        data_url(self.book, &path).unwrap_or_else(|| href.to_owned())
    }
}

fn data_url(book: &Book, path: &str) -> Option<String> {
    let bytes = book.file(path)?;
    let mime = book.media_type(path);
    let mut url = format!("data:{mime};base64,");
    base64::engine::general_purpose::STANDARD.encode_string(bytes.as_slice(), &mut url);
    Some(url)
}

/// The stylesheet at `path`, its `url()`s and `@import`s inlined.
fn stylesheet(book: &Book, path: &str, depth: u32) -> String {
    match book.text(path) {
        Some(css) => rewrite_css(book, path, &css, depth),
        None => String::new(),
    }
}

/// `css` (of the file at `base`) with `@import`s inlined and `url()`s made
/// `data:` URLs.
fn rewrite_css(book: &Book, base: &str, css: &str, depth: u32) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    loop {
        let import = rest.find("@import");
        let url = rest.find("url(");
        let at = match (import, url) {
            (Some(i), Some(u)) => i.min(u),
            (Some(i), None) => i,
            (None, Some(u)) => u,
            (None, None) => break,
        };
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if rest.starts_with("@import") {
            let end = rest.find(';').map(|i| i + 1).unwrap_or(rest.len());
            let statement = &rest[..end];
            let target = statement
                .split(['"', '\''])
                .nth(1)
                .or_else(|| {
                    statement
                        .find("url(")
                        .map(|i| statement[i + 4..].split(')').next().unwrap_or("").trim())
                })
                .map(|s| s.trim_matches(['"', '\'']));
            if let Some(target) = target
                && depth < 4
            {
                let path = epub::split_fragment(&epub::resolve(base, target)).0;
                out.push_str(&stylesheet(book, &path, depth + 1));
            }
            rest = &rest[end..];
        } else {
            let close = rest.find(')').unwrap_or(rest.len() - 1);
            let raw = rest[4..close].trim().trim_matches(['"', '\'']);
            let replaced = if raw.starts_with("data:") || raw.starts_with('#') || epub::is_external(raw) {
                None
            } else {
                data_url(book, &epub::split_fragment(&epub::resolve(base, raw)).0)
            };
            match replaced {
                Some(url) => {
                    let _ = write!(out, "url(\"{url}\")");
                }
                None => out.push_str(&rest[..=close]),
            }
            rest = &rest[close + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// The body of a document that is not XML, its `src` attributes inlined.
fn loose_body(book: &Book, path: &str, source: &str) -> String {
    let lower = source.to_ascii_lowercase();
    let start = lower
        .find("<body")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1))
        .unwrap_or(0);
    let end = lower.rfind("</body>").unwrap_or(source.len());
    let body = &source[start..end.max(start)];
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find("src=\"") {
        out.push_str(&rest[..at + 5]);
        rest = &rest[at + 5..];
        let close = rest.find('"').unwrap_or(rest.len());
        let value = &rest[..close];
        let path = epub::split_fragment(&epub::resolve(path, value)).0;
        out.push_str(&data_url(book, &path).unwrap_or_else(|| value.to_owned()));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out.replace("<script", "<template").replace("</script>", "</template>")
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
}

fn page(
    book: &Book,
    styles: &str,
    body_attrs: &str,
    body: &str,
    target: &Target,
    settings: &Settings,
) -> String {
    let (paper, ink) = settings.paper.colors();
    let target = target.json();
    let view = if settings.spread { 2 } else { 1 };
    let dark = if settings.paper == Paper::Dark {
        "#flow *{color:inherit!important;background-color:transparent!important;border-color:#555!important}\
         #flow img{filter:brightness(.85)}"
    } else {
        ""
    };
    // The book's styles come first, so the sheet's own rules win.
    format!(
        r##"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,user-scalable=no">
<style>{styles}</style>
<style>
html,body{{margin:0!important;padding:0!important;width:100%!important;height:100%!important;
  overflow:hidden!important;background:{paper}!important;columns:auto!important;}}
#paper{{position:absolute;left:0;top:0;width:{paper_w}px;height:{ph}px;overflow:hidden;
  transform-origin:0 0;background:{paper};color:{ink};}}
#flow{{box-sizing:border-box;width:{pw}px;height:{ph}px;margin:0;
  padding:76px 76px 100px 76px;column-width:642px;column-gap:152px;column-fill:auto;
  font-size:{size}%;line-height:1.65;word-break:keep-all;overflow-wrap:break-word;
  text-align:justify;will-change:transform;}}
#flow img,#flow svg,#flow video,#flow object,#flow table{{max-width:100%!important;}}
#flow img,#flow svg,#flow video{{max-height:925px!important;height:auto;object-fit:contain;
  break-inside:avoid;}}
#flow h1,#flow h2,#flow h3,#flow h4{{break-after:avoid;}}
#flow p,#flow li,#flow blockquote{{orphans:2;widows:2;}}
#flow pre{{white-space:pre-wrap;}}
#flow a{{color:inherit;}}
#flow a[data-ext],#flow a[data-epub-href]{{color:#2f6fdb;}}
#end{{display:inline-block;width:1px;height:1px;}}
#copy{{position:fixed;z-index:10;display:none;align-items:center;gap:6px;padding:7px 12px;border-radius:8px;
  background:#1f2937;color:#fff;font:600 13px/1 system-ui,sans-serif;cursor:pointer;user-select:none;
  box-shadow:0 4px 14px rgba(0,0,0,.28);transform:translate(-50%,-100%);white-space:nowrap;}}
#copy:hover{{background:#111827;}}
#copy svg{{width:14px;height:14px;}}
.folio{{position:absolute;width:{pw}px;bottom:40px;text-align:center;font:13px/1 system-ui,sans-serif;
  color:{ink};opacity:.55;letter-spacing:.05em;}}
#spine{{position:absolute;top:0;bottom:0;left:{spine_x}px;width:80px;pointer-events:none;
  background:linear-gradient(to right,rgba(0,0,0,0),rgba(0,0,0,.07) 42%,rgba(0,0,0,.16) 50%,rgba(0,0,0,.07) 58%,rgba(0,0,0,0));}}
{dark}
</style></head>
<body><div id="paper"><div id="flow"><div class="epub-body"{body_attrs}>{body}</div><span id="end"></span></div>
<div class="folio" style="left:0"></div><div class="folio" style="left:{pw}px"></div>{spine}</div>
<div id="copy" role="button" title="선택한 글 복사 (Ctrl+C)"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/>
<path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg><span>복사</span></div>
<script>
(() => {{
  const PW = {pw}, PH = {ph}, VIEW = {view};
  const paper = document.getElementById('paper');
  const flow = document.getElementById('flow');
  const folios = document.querySelectorAll('.folio');
  let target = {target};
  let page = 0, pages = 1, scale = 1, ready = false;
  const post = (m) => {{ try {{ window.nsh.postMessage(JSON.stringify(m)); }} catch (e) {{}} }};
  function fit() {{
    scale = Math.min(innerWidth / (PW * VIEW), innerHeight / PH) || 1;
    paper.style.transform = 'scale(' + scale + ')';
    paper.style.left = Math.max(0, (innerWidth - PW * VIEW * scale) / 2) + 'px';
  }}
  function x(el) {{
    const r = el.getBoundingClientRect(), f = flow.getBoundingClientRect();
    return (r.left - f.left) / scale;
  }}
  function measure() {{
    pages = Math.max(1, Math.floor(x(document.getElementById('end')) / PW) + 1);
  }}
  function pageOf(el) {{ return Math.max(0, Math.min(pages - 1, Math.floor(x(el) / PW))); }}
  function show(n, quiet) {{
    // A spread starts on an even page; its last sheet may be blank.
    n = Math.max(0, Math.min(pages - 1, n));
    page = n - n % VIEW;
    flow.style.transform = 'translateX(' + (-page * PW) + 'px)';
    hideBubble();
    folios.forEach((f, i) => {{ f.textContent = page + i < pages ? (page + i + 1) + ' / ' + pages : ''; }});
    if (!quiet) post({{ type: 'page', page, pages }});
  }}
  function go(t) {{
    if (t.last) return show(pages - 1);
    if (t.anchor) {{
      const el = document.getElementById(t.anchor) || document.getElementsByName(t.anchor)[0];
      return show(el ? pageOf(el) : 0);
    }}
    if (t.fraction != null) return show(Math.round(t.fraction * (pages - 1)));
    show(t.page || 0);
  }}
  function turn(d) {{
    if (!ready) return;
    const n = page + d * VIEW;
    if (n < 0) post({{ type: 'edge', dir: -1 }});
    else if (n >= pages) post({{ type: 'edge', dir: 1 }});
    else show(n);
  }}
  function layout() {{
    fit(); measure(); go(target); ready = true;
  }}
  window.addEventListener('resize', () => {{ fit(); hideBubble(); }});
  window.addEventListener('message', (e) => {{
    let m; try {{ m = JSON.parse(e.data); }} catch (err) {{ return; }}
    if (m.cmd === 'turn') turn(m.dir);
    else if (m.cmd === 'go') {{ target = m.target; if (ready) go(m.target); }}
  }});
  // A click that loaded this page (a link to another chapter) can reach it
  // too; clicks in its first moments are not the reader's.
  const settled = () => performance.now() > 600;
  // The copy bubble over selected text (the web view has no menu of its own).
  const bubble = document.getElementById('copy'), label = bubble.querySelector('span');
  const selected = () => {{ const s = getSelection(); return s && !s.isCollapsed ? String(s).trim() : ''; }};
  function hideBubble() {{ bubble.style.display = 'none'; }}
  function placeBubble() {{
    const s = getSelection();
    if (!selected() || !s.rangeCount) return hideBubble();
    const rects = [...s.getRangeAt(0).getClientRects()].filter(r => r.width > 0);
    if (!rects.length) return hideBubble();
    const top = rects.reduce((a, r) => (r.top < a.top ? r : a));
    label.textContent = '복사';
    bubble.style.display = 'flex';
    const y = top.top - 8 < 40 ? top.bottom + 8 + bubble.offsetHeight : top.top - 8;
    bubble.style.left = Math.max(50, Math.min(innerWidth - 50, top.left + top.width / 2)) + 'px';
    bubble.style.top = y + 'px';
  }}
  bubble.addEventListener('mousedown', (e) => {{ e.preventDefault(); e.stopPropagation(); }});
  bubble.addEventListener('click', (e) => {{
    e.preventDefault(); e.stopPropagation();
    const text = selected();
    if (!text) return hideBubble();
    post({{ type: 'copy', text }});
    label.textContent = '복사됨';
    setTimeout(() => {{ getSelection().removeAllRanges(); hideBubble(); }}, 700);
  }});
  document.addEventListener('mouseup', () => setTimeout(placeBubble, 0));
  document.addEventListener('keyup', (e) => {{ if (e.shiftKey || e.key === 'Shift') placeBubble(); }});
  document.addEventListener('selectionchange', () => {{ if (!selected()) hideBubble(); }});
  // A click turns the page only once it is not the start of a double click
  // (which selects a word), and not the click that drops a selection.
  let pendingTurn = 0, hadSelection = false;
  document.addEventListener('mousedown', () => {{ hadSelection = !!selected(); }});
  document.addEventListener('dblclick', () => {{ clearTimeout(pendingTurn); }});
  document.addEventListener('click', (e) => {{
    if (!settled()) {{ e.preventDefault(); return; }}
    const a = e.target.closest && e.target.closest('a');
    if (a) {{
      e.preventDefault();
      const href = a.getAttribute('data-epub-href');
      if (href) post({{ type: 'link', href }});
      else if (a.hasAttribute('data-ext')) post({{ type: 'external', href: a.getAttribute('href') }});
      return;
    }}
    clearTimeout(pendingTurn);
    if (e.detail > 1 || hadSelection || selected()) return;
    const r = e.clientX / innerWidth;
    const d = r < 0.35 ? -1 : r > 0.65 ? 1 : 0;
    if (d) pendingTurn = setTimeout(() => {{ if (!selected()) turn(d); }}, 260);
  }});
  let wheelAt = 0;
  document.addEventListener('wheel', (e) => {{
    e.preventDefault();
    if (!settled()) return;
    const d = Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    const now = Date.now();
    if (Math.abs(d) < 4 || now - wheelAt < 280) return;
    wheelAt = now;
    turn(d > 0 ? 1 : -1);
  }}, {{ passive: false }});
  document.addEventListener('keydown', (e) => {{
    const k = e.key, mod = e.ctrlKey || e.metaKey;
    if (mod && (k === 'o' || k === 'O')) {{ e.preventDefault(); post({{ type: 'key', key: 'open' }}); return; }}
    if (mod && (k === '=' || k === '+')) {{ e.preventDefault(); post({{ type: 'key', key: 'bigger' }}); return; }}
    if (mod && k === '-') {{ e.preventDefault(); post({{ type: 'key', key: 'smaller' }}); return; }}
    if (mod) return;
    if (k === 'ArrowRight' || k === 'ArrowDown' || k === 'PageDown' || (k === ' ' && !e.shiftKey)) {{ e.preventDefault(); turn(1); }}
    else if (k === 'ArrowLeft' || k === 'ArrowUp' || k === 'PageUp' || (k === ' ' && e.shiftKey)) {{ e.preventDefault(); turn(-1); }}
    else if (k === 'Home') {{ e.preventDefault(); post({{ type: 'key', key: 'home' }}); }}
    else if (k === 'End') {{ e.preventDefault(); post({{ type: 'key', key: 'end' }}); }}
  }});
  fit();
  const start = () => {{
    const fonts = document.fonts ? document.fonts.ready : Promise.resolve();
    fonts.then(() => requestAnimationFrame(layout));
  }};
  if (document.readyState === 'complete') start(); else window.addEventListener('load', start);
}})();
</script></body></html>"##,
        lang = escape_attr(&book.language),
        pw = PAGE_WIDTH,
        ph = PAGE_HEIGHT,
        paper_w = PAGE_WIDTH * view as f32,
        spine_x = PAGE_WIDTH - 40.0,
        spine = if view == 2 { r#"<div id="spine"></div>"# } else { "" },
        size = settings.text_size,
    )
}
