//! The app's Rust UI: an EPUB 2 / EPUB 3 reader that shows the book as A4
//! portrait sheets. `nsh run`, `nsh package` and `cargo run` mount `App`.

mod epub;
mod render;

use std::{path::PathBuf, sync::Arc};

use nsh_base::theme;
use nsh_core::{keymap::When, prelude::*};

pub use epub::Book;
pub use render::{Paper, Settings, Target, chapter_page};

/// The app's UI set (`nsh.json` `"rust"`): what a TSX entry may import from
/// `epub-viewer/native` (`README.md`, "Adding a TSX entry").
#[native_module("epub-viewer/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// A book that was opened; two are equal when they are the same opening.
#[derive(Clone)]
struct Opened(Arc<Book>);

impl PartialEq for Opened {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What the sheet shows: the chapter, where it opens, and how. Changing it
/// loads the page again; turning pages inside a chapter does not.
#[derive(Clone, PartialEq)]
struct Reading {
    chapter: usize,
    target: Target,
    settings: Settings,
}

#[derive(Clone, PartialEq)]
enum Status {
    Empty,
    Loading,
    Ready,
    Failed(String),
}

const TOOLBAR: f32 = 52.0;
const SIDEBAR: f32 = 280.0;
const TEXT_SIZES: &[u32] = &[70, 80, 90, 100, 110, 120, 135, 150, 175, 200];

/// The application's root component.
#[component]
pub fn App() -> impl IntoElement {
    let book = signal(None::<Opened>);
    let reading = signal(Reading {
        chapter: 0,
        target: Target::Page(0),
        settings: Settings {
            text_size: 100,
            paper: Paper::White,
            spread: true,
        },
    });
    // The chapter's page and page count, as the sheet reports them.
    let position = signal((0_u32, 1_u32));
    let status = signal(Status::Empty);
    let toc_open = signal(true);
    let area = signal(Size {
        width: px(0.0),
        height: px(0.0),
    });
    let sheet = element_ref();
    maximize_window();

    let open_path = move |path: PathBuf| {
        status.set(Status::Loading);
        spawn(async move {
            match blocking(move || Book::open(&path)).await {
                Ok(Ok(opened)) => {
                    position.set((0, 1));
                    reading.update(|r| {
                        r.chapter = 0;
                        r.target = Target::Page(0);
                    });
                    book.set(Some(Opened(Arc::new(opened))));
                    status.set(Status::Ready);
                }
                Ok(Err(error)) => status.set(Status::Failed(error)),
                Err(error) => status.set(Status::Failed(error.to_string())),
            }
        });
    };
    let pick = move || {
        spawn(async move {
            let picked = blocking(|| {
                rfd::FileDialog::new()
                    .set_title("EPUB 열기")
                    .add_filter("EPUB", &["epub"])
                    .pick_file()
            })
            .await;
            if let Ok(Some(path)) = picked {
                open_path(path);
            }
        });
    };
    // A book named on the command line (`epub-viewer book.epub`, "Open with").
    if let Some(path) = std::env::args_os().skip(1).map(PathBuf::from).find(|p| p.is_file()) {
        open_path(path);
    }

    let chapters = move || book.with(|b| b.as_ref().map_or(0, |b| b.0.spine.len()));
    // Another chapter loads its page; the open one only moves to the target.
    let go_chapter = move |chapter: usize, target: Target| {
        if book.with_untracked(|b| b.is_none()) {
            return;
        }
        if chapter == reading.with_untracked(|r| r.chapter) {
            sheet.post_message(format!("{{\"cmd\":\"go\",\"target\":{}}}", target.json()));
        } else {
            reading.update(|r| {
                r.chapter = chapter;
                r.target = target;
            });
        }
    };
    let go_start = move || go_chapter(0, Target::Page(0));
    let go_end = move || go_chapter(chapters().saturating_sub(1), Target::Last);
    let turn = move |dir: i32| {
        if book.with_untracked(|b| b.is_some()) {
            sheet.post_message(format!("{{\"cmd\":\"turn\",\"dir\":{dir}}}"));
        }
    };
    // Settings change the layout: reopen at the same share of the chapter.
    let change_settings = move |change: &dyn Fn(&mut Settings)| {
        let (page, pages) = position.get_untracked();
        let fraction = if pages > 1 { page as f32 / (pages - 1) as f32 } else { 0.0 };
        reading.update(|r| {
            change(&mut r.settings);
            r.target = Target::Fraction(fraction);
        });
    };
    let step_text = move |dir: i32| {
        change_settings(&|s: &mut Settings| {
            let at = TEXT_SIZES.iter().position(|&t| t >= s.text_size).unwrap_or(3) as i32;
            let next = (at + dir).clamp(0, TEXT_SIZES.len() as i32 - 1) as usize;
            s.text_size = TEXT_SIZES[next];
        })
    };

    let on_message = move |data: &str| {
        log::debug!("sheet: {data}");
        let Ok(message) = serde_json::from_str::<serde_json::Value>(data) else {
            return;
        };
        let text = |key: &str| message[key].as_str().unwrap_or("").to_owned();
        match message["type"].as_str() {
            Some("page") => {
                let page = message["page"].as_u64().unwrap_or(0) as u32;
                let pages = message["pages"].as_u64().unwrap_or(1) as u32;
                position.set((page, pages));
            }
            Some("edge") => {
                let chapter = reading.with_untracked(|r| r.chapter);
                if message["dir"].as_i64() == Some(1) {
                    if chapter + 1 < chapters() {
                        go_chapter(chapter + 1, Target::Page(0));
                    }
                } else if chapter > 0 {
                    go_chapter(chapter - 1, Target::Last);
                }
            }
            Some("link") => {
                let Some(Opened(b)) = book.get_untracked() else { return };
                let (path, fragment) = epub::split_fragment(&text("href"));
                let Some(chapter) = b.chapter_of(&path) else { return };
                go_chapter(chapter, fragment.map(Target::Anchor).unwrap_or(Target::Page(0)));
            }
            Some("external") => drop(open_url(text("href"))),
            Some("copy") => set_clipboard(text("text")),
            Some("key") => match text("key").as_str() {
                "open" => pick(),
                "bigger" => step_text(1),
                "smaller" => step_text(-1),
                "home" => go_start(),
                "end" => go_end(),
                _ => {}
            },
            _ => {}
        }
    };

    // Keys while the focus is outside the sheet (the sheet handles its own).
    let keys = KeyOptions {
        when: When::NotEditing,
        repeat: true,
    };
    for key in ["right", "down", "pagedown", "space"] {
        on_key_with(key, keys, move |_| turn(1));
    }
    for key in ["left", "up", "pageup", "shift+space"] {
        on_key_with(key, keys, move |_| turn(-1));
    }
    on_key_with("home", keys, move |_| go_start());
    on_key_with("end", keys, move |_| go_end());
    on_key("mod+o", move |_| pick());
    on_key("mod+=", move |_| step_text(1));
    on_key("mod+-", move |_| step_text(-1));

    let html = move || {
        let Some(Opened(b)) = book.get() else {
            return String::new();
        };
        let r = reading.get();
        let chapter = r.chapter.min(b.spine.len() - 1);
        render::chapter_page(&b, chapter, &r.target, &r.settings)
    };
    // The sheets: A4 portrait, one or two side by side, as large as the
    // space around them allows.
    let sheets = move || if reading.with(|r| r.settings.spread) { 2.0 } else { 1.0 };
    let sheet_height = move || {
        let size = area.get();
        let room_w = (size.width.0 - 48.0).max(120.0);
        let room_h = (size.height.0 - 48.0).max(170.0);
        room_h.min(room_w * render::PAGE_HEIGHT / (render::PAGE_WIDTH * sheets()))
    };
    let sheet_width = move || sheet_height() * render::PAGE_WIDTH * sheets() / render::PAGE_HEIGHT;

    let toolbar = view()
        .h(px(TOOLBAR))
        .flex_row()
        .items_center()
        .gap_2()
        .px_3()
        .border_b_1()
        .border_color(move || theme().border)
        .bg(move || theme().background)
        .child(tool("☰", "목차", move || toc_open.update(|o| *o = !*o)))
        .child(tool("열기", "EPUB 파일 열기 (Ctrl+O)", pick))
        .child(
            view()
                .flex_1()
                .px_2()
                .child(
                    text(move || {
                        book.with(|b| match b {
                            Some(Opened(b)) if b.author.is_empty() => b.title.clone(),
                            Some(Opened(b)) => format!("{} — {}", b.title, b.author),
                            None => "EPUB 뷰어".to_owned(),
                        })
                    })
                    .number_of_lines(1)
                    .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    text(move || {
                        book.with(|b| match b {
                            Some(Opened(b)) => format!("EPUB {} · A4 세로", b.version),
                            None => "A4 세로 · EPUB 2 / EPUB 3".to_owned(),
                        })
                    })
                    .text_size(px(11.0))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(tool("가−", "글자 작게 (Ctrl+−)", move || step_text(-1)))
        .child(
            text(move || format!("{}%", reading.with(|r| r.settings.text_size)))
                .text_size(px(12.0))
                .w(px(40.0))
                .text_center()
                .text_color(move || theme().muted_foreground),
        )
        .child(tool("가+", "글자 크게 (Ctrl+=)", move || step_text(1)))
        .child(tool_dynamic(
            move || reading.with(|r| r.settings.paper.label()).to_owned(),
            "종이 색 바꾸기",
            move || change_settings(&|s: &mut Settings| s.paper = s.paper.next()),
        ))
        .child(tool_dynamic(
            move || if reading.with(|r| r.settings.spread) { "양쪽 보기" } else { "한쪽 보기" }.to_owned(),
            "양쪽 보기 / 한쪽 보기 바꾸기",
            move || change_settings(&|s: &mut Settings| s.spread = !s.spread),
        ))
        .child(view().w(px(1.0)).h(px(24.0)).mx_1().bg(move || theme().border))
        .child(tool("◀", "이전 쪽 (←)", move || turn(-1)))
        .child(
            text(move || {
                if chapters() == 0 {
                    return "— / —".to_owned();
                }
                let (page, pages) = position.get();
                let shown = if reading.with(|r| r.settings.spread) && page + 1 < pages {
                    format!("{}–{}", page + 1, page + 2)
                } else {
                    (page + 1).to_string()
                };
                format!(
                    "{} / {}쪽 · {} / {}장",
                    shown,
                    pages,
                    reading.with(|r| r.chapter) + 1,
                    chapters()
                )
            })
            .text_size(px(12.0))
            .tabular_nums()
            .w(px(150.0))
            .text_center(),
        )
        .child(tool("▶", "다음 쪽 (→)", move || turn(1)));

    let toc = move || {
        view()
        .w(px(SIDEBAR))
        .border_r_1()
        .border_color(move || theme().border)
        .bg(move || theme().sidebar)
        .child(
            text("목차")
                .px_4()
                .pt_4()
                .pb_2()
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().muted_foreground),
        )
        .child(
            scroll().flex_1().child(
                view().pb_4().child(each(
                    move || {
                        book.with(|b| {
                            b.as_ref()
                                .map(|b| b.0.toc.iter().cloned().enumerate().collect::<Vec<_>>())
                                .unwrap_or_default()
                        })
                    },
                    |(index, _)| *index,
                    move |entry, _| {
                        let current = move || {
                            entry.with(|(_, e)| e.chapter) == Some(reading.with(|r| r.chapter))
                        };
                        view()
                            .mx_2()
                            .py(px(7.0))
                            .pr_2()
                            .pl(move || px(10.0 + 14.0 * entry.with(|(_, e)| e.depth) as f32))
                            .rounded(px(6.0))
                            .cursor(Cursor::Pointer)
                            .bg(move || if current() { theme().sidebar_accent } else { theme().sidebar })
                            .hover(|s| s.bg(move || theme().sidebar_accent))
                            .on_press(move || {
                                let (_, e) = entry.get();
                                if let Some(chapter) = e.chapter {
                                    go_chapter(chapter, e.fragment.map(Target::Anchor).unwrap_or(Target::Page(0)));
                                }
                            })
                            .child(
                                text(move || entry.with(|(_, e)| e.label.clone()))
                                    .text_size(px(13.0))
                                    .number_of_lines(2)
                                    
                                    .text_color(move || theme().sidebar_foreground),
                            )
                    },
                )),
            ),
        )
    };

    let desk = view()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || if dark() { rgb(0x111214) } else { rgb(0x8a8d93) })
        .on_layout(move |bounds: &Bounds<Points>| area.set(bounds.size))
        .on_file_drop(move |event: &FileDropEvent| {
            let path = event.uris.iter().find_map(|uri| file_path(uri));
            if let Some(path) = path {
                open_path(path);
            }
        })
        .child(
            when(move || status.get() == Status::Ready, move || {
                view()
                    .w(move || px(sheet_width()))
                    .h(move || px(sheet_height()))
                    .bg(move || reading.with(|r| hex(r.settings.paper.colors().0)))
                    .shadow_xl()
                    .child(
                        web_view_html(html, Some("https://book.epub-viewer.invalid/page.html"))
                            .element_ref(&sheet)
                            .flex_1()
                            .on_message(on_message),
                    )
            })
            .otherwise(move || welcome(status, pick)),
        );

    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(toolbar)
        .child(
            view()
                .flex_1()
                .flex_row()
                .child(when(move || toc_open.get() && chapters() > 0, toc))
                .child(desk),
        )
}

/// What the desk shows with no book open: a blank A4 sheet with the way in.
fn welcome(status: Signal<Status>, pick: impl Fn() + Copy + 'static) -> impl IntoElement {
    view()
        .w(px(318.0))
        .h(px(450.0))
        .bg(rgb(0xffffff))
        .shadow_xl()
        .items_center()
        .justify_center()
        .gap_3()
        .p_6()
        .child(text("A4").text_size(px(40.0)).font_weight(FontWeight::BOLD).text_color(rgb(0xd0d3d8)))
        .child(text("EPUB 2 · EPUB 3 뷰어").text_size(px(17.0)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(0x1a1a1a)))
        .child(
            text(move || match status.get() {
                Status::Loading => "책을 여는 중…".to_owned(),
                Status::Failed(error) => format!("열 수 없습니다: {error}"),
                _ => "EPUB 파일을 여기로 끌어 놓거나\n[EPUB 열기]를 누르세요.".to_owned(),
            })
            .text_size(px(13.0))
            .text_center()
            .text_color(move || {
                if matches!(status.get(), Status::Failed(_)) { rgb(0xc0392b) } else { rgb(0x6b7076) }
            }),
        )
        .child(
            view()
                .mt_2()
                .px_5()
                .py_2()
                .rounded(px(8.0))
                .bg(rgb(0x1f2937))
                .cursor(Cursor::Pointer)
                .active(|s| s.opacity(0.7))
                .on_press(pick)
                .child(text("EPUB 열기").text_color(rgb(0xffffff)).font_weight(FontWeight::SEMIBOLD)),
        )
}

/// Maximizes the app's window once it shows. nsh 0.3.0 has no Rust call
/// for it (TSX's `Window.toggleMaximize` only), so on Windows the app asks
/// Win32 itself; elsewhere the window keeps the platform's size.
fn maximize_window() {
    #[cfg(target_os = "windows")]
    spawn(async {
        for _ in 0..60 {
            if win::maximize_own_window() {
                return;
            }
            timer(std::time::Duration::from_millis(50)).await;
        }
    });
}

#[cfg(target_os = "windows")]
mod win {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
            SW_MAXIMIZE, ShowWindowAsync,
        },
    };

    /// Maximizes this process's visible top-level window; `false` while it
    /// has none yet.
    pub fn maximize_own_window() -> bool {
        unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut pid = 0;
            // SAFETY: plain calls on a window EnumWindows handed over.
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == GetCurrentProcessId()
                    && IsWindowVisible(hwnd) != 0
                    && GetWindow(hwnd, GW_OWNER).is_null()
                {
                    *(found as *mut HWND) = hwnd;
                    return 0;
                }
            }
            1
        }
        let mut hwnd: HWND = std::ptr::null_mut();
        // SAFETY: `find` writes only through the pointer it is given, which
        // outlives the call.
        unsafe {
            EnumWindows(Some(find), &mut hwnd as *mut HWND as LPARAM);
        }
        if hwnd.is_null() {
            return false;
        }
        // Async: the window belongs to the UI thread, not this one.
        // SAFETY: a plain call on a live window.
        unsafe { ShowWindowAsync(hwnd, SW_MAXIMIZE) };
        true
    }
}

/// A toolbar button.
fn tool(label: &'static str, hint: &'static str, on_press: impl Fn() + 'static) -> impl IntoElement {
    tool_dynamic(move || label.to_owned(), hint, on_press)
}

fn tool_dynamic(
    label: impl Fn() -> String + 'static,
    hint: &'static str,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .px_3()
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor(Cursor::Pointer)
        .hover(|s| s.bg(move || theme().accent))
        .active(|s| s.opacity(0.6))
        .tooltip(
            view()
                .px_2()
                .py_1()
                .rounded(px(6.0))
                .bg(move || theme().popover)
                .border_1()
                .border_color(move || theme().border)
                .shadow_md()
                .child(text(hint).text_size(px(12.0)).text_color(move || theme().popover_foreground))
                .into_element(),
        )
        .on_press(on_press)
        .child(text(label).text_size(px(13.0)))
}

/// `#rrggbb` as a color.
fn hex(color: &str) -> Hsla {
    rgb(u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0xffffff)).into()
}

/// The local path of a `file:` URL.
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(rest).decode_utf8_lossy();
    // `file:///C:/…` on Windows: no leading slash before the drive.
    let path = match decoded.as_bytes() {
        [b'/', _, b':', ..] => decoded[1..].to_owned(),
        _ => decoded.into_owned(),
    };
    Some(PathBuf::from(path))
}
