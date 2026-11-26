# EPUB 뷰어 (A4 세로)

[Native Shell (nsh.dev)](https://nsh.dev)로 만든 EPUB 2 / EPUB 3 뷰어입니다. 본문을 A4 세로 용지(210 × 297 mm)에
조판해 한 쪽씩 보여 줍니다. 레이아웃은 A4 기준(794 × 1123 CSS px, 여백 20 mm)으로 고정되고 창 크기에 맞춰 확대·축소만
되므로, 창 크기가 바뀌어도 쪽 나눔은 그대로입니다.

## 실행

```sh
nsh run                                   # 이 컴퓨터의 데스크톱 창
cargo run -- samples/sample-epub3.epub    # 책을 바로 열기
nsh package --target windows              # 설치용 앱 만들기
```

Windows에서는 WebView2 런타임이 필요합니다(Windows 11에는 기본 포함).

## Windows 릴리스와 설치

```sh
nsh package --target windows                                       # .nsh\out\EPUB Viewer\
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" installer\epub-viewer.iss  # dist\EPUB-Viewer-Setup-1.0.3.exe
```

설치 프로그램은 관리자 권한 없이 현재 사용자에게 설치하고(`%LOCALAPPDATA%\Programs\EPUB Viewer`), `.epub`을
`OpenLearningLab.EpubViewer.epub` 형식으로 등록해 [연결 프로그램]과 Windows [기본 앱] 목록에 나타나게 합니다.
Windows 10/11은 앱이 스스로 기본 앱이 될 수 없으므로, 설치 마지막의 "기본 앱 설정 열기"에서 `.epub` → EPUB 뷰어를
고르거나, `.epub` 파일을 오른쪽 클릭 → 연결 프로그램 → 다른 앱 선택 → EPUB 뷰어 → [항상]을 누르세요.

아이콘은 `scripts/make_icon.py`가 `assets/icon.png`(1024 px, `nsh.json`의 `"icon"`)와 `assets/icon.ico`(설치 프로그램)로
그립니다.

## 기능

- EPUB 2(NCX 목차)와 EPUB 3(nav 목차) 읽기, 목차가 없으면 spine 순서로 대체
- A4 세로 쪽 나눔, 아래쪽 쪽 번호, 장 끝에서 다음/이전 장으로 이어짐
- 책의 CSS·이미지·글꼴을 그대로 적용(`@import`와 `url()` 포함)
- 책 안 링크·각주 이동, 외부 링크는 기본 브라우저로 열기
- 양쪽 보기(펼친 책, 기본)와 한쪽 보기, 실행하면 창 최대화
- 글자 크기 70–200 %, 흰 종이 / 세피아 / 어두운 종이(바꿔도 읽던 위치 유지)
- 파일 열기: [열기] 버튼, Ctrl+O, 창에 끌어 놓기, 명령줄 인자

| 조작 | 동작 |
|---|---|
| →, ↓, PageDown, Space, 오른쪽 클릭 영역, 휠 아래 | 다음 쪽 |
| ←, ↑, PageUp, Shift+Space, 왼쪽 클릭 영역, 휠 위 | 이전 쪽 |
| Home / End | 책의 맨 처음 / 맨 끝 |
| Ctrl+= / Ctrl+− | 글자 크게 / 작게 |
| 더블클릭·끌어서 선택 → [복사] 또는 Ctrl+C | 선택한 글 복사 |

## 구조

| 파일 | 역할 |
|---|---|
| `src/epub.rs` | ZIP·`container.xml`·OPF(메타데이터, manifest, spine)·목차(nav, NCX) 읽기 |
| `src/render.rs` | spine 문서 하나를 자체 완결 HTML로 변환하고 CSS 다단으로 A4 쪽 나누기 |
| `src/lib.rs` | nsh UI: 도구 막대, 목차, A4 용지 WebView, 키보드·파일 열기 |
| `tests/books.rs` | 샘플 책 읽기·변환 테스트 (`cargo test`) |
| `scripts/make_samples.py` | `samples/`의 시험용 EPUB 2·3 만들기 |
