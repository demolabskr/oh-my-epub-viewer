//! The sample books (`scripts/make_samples.py`) open and render.

use std::path::Path;

use nsh_app_epub_viewer::{Book, Paper, Settings, Target, chapter_page};

fn sample(name: &str) -> Book {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples").join(name);
    Book::open(&path).unwrap()
}

#[test]
fn epub2_reads_its_ncx() {
    let book = sample("sample-epub2.epub");
    assert_eq!(book.title, "샘플 책 (EPUB 2)");
    assert_eq!(book.version, "2.0");
    assert_eq!(book.spine.len(), 3);
    let labels: Vec<_> = book.toc.iter().map(|e| (e.label.as_str(), e.depth)).collect();
    assert_eq!(
        labels,
        [("첫째 장: 시작", 0), ("둘째 장: 여정", 0), ("각주 2", 1), ("셋째 장: 끝", 0)]
    );
    assert_eq!(book.toc[2].chapter, Some(1));
    assert_eq!(book.toc[2].fragment.as_deref(), Some("note2"));
}

#[test]
fn epub3_reads_its_nav() {
    let book = sample("sample-epub3.epub");
    assert_eq!(book.version, "3.0");
    assert_eq!(book.toc.len(), 3);
    assert_eq!(book.toc[2].chapter, Some(2));
}

#[test]
fn pages_inline_styles_and_images() {
    let book = sample("sample-epub2.epub");
    let settings = Settings {
        text_size: 100,
        paper: Paper::White,
        spread: false,
    };
    let html = chapter_page(&book, 0, &Target::Page(0), &settings);
    assert!(html.contains("<h1 id=\"top\">첫째 장: 시작</h1>"));
    assert!(html.contains("src=\"data:image/png;base64,"));
    assert!(html.contains("url(\"data:image/png;base64,"));
    assert!(html.contains("data-epub-href=\"OEBPS/text/ch2.xhtml#top\""));
    assert!(html.contains("href=\"https://example.com/\" data-ext=\"1\""));
    assert!(html.contains("\u{2014}"), "named entities become characters");
    if let Ok(dir) = std::env::var("EPUB_PREVIEW_DIR") {
        std::fs::write(Path::new(&dir).join("chapter.html"), html).unwrap();
    }
}

#[test]
fn spreads_show_two_sheets() {
    let book = sample("sample-epub3.epub");
    let settings = Settings {
        text_size: 100,
        paper: Paper::Sepia,
        spread: true,
    };
    let html = chapter_page(&book, 1, &Target::Last, &settings);
    assert!(html.contains("VIEW = 2"));
    assert!(html.contains("<div id=\"spine\"></div>"));
    assert!(html.contains("width:1588px"));
    if let Ok(dir) = std::env::var("EPUB_PREVIEW_DIR") {
        std::fs::write(Path::new(&dir).join("spread.html"), html).unwrap();
    }
}

#[test]
fn covers_inline_their_image() {
    let book = sample("sample-cover.epub");
    assert_eq!(book.spine.len(), 4);
    let settings = Settings {
        text_size: 100,
        paper: Paper::White,
        spread: false,
    };
    for chapter in 0..3 {
        let html = chapter_page(&book, chapter, &Target::Page(0), &settings);
        assert!(html.contains("data:image/png;base64,"), "cover {chapter} has its image");
        if let Ok(dir) = std::env::var("EPUB_PREVIEW_DIR") {
            std::fs::write(Path::new(&dir).join(format!("cover-{chapter}.html")), html).unwrap();
        }
    }
}
