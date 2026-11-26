"""Writes two small sample books, one EPUB 2 (NCX) and one EPUB 3 (nav), to samples/."""
import base64, os, zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "samples")

PNG = base64.b64decode(  # a 2x2 blue PNG
    "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGNkYPjPwMDAxMDAwMDAAAAhCAMB2nVTSgAAAABJRU5ErkJggg=="
)

CSS = """body { font-family: serif; }
h1 { text-align: center; margin: 2em 0 1em; }
p { text-indent: 1em; margin: 0 0 .4em; }
.figure img { width: 60%; display: block; margin: 1em auto; background: url(../images/dot.png); }
"""

def paragraphs(n, chapter):
    text = ("이 문단은 A4 세로 쪽 나누기를 시험하기 위한 본문입니다. "
            "글이 길어지면 다음 쪽으로 넘어가고, 장이 끝나면 다음 장으로 이어집니다. "
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor. ")
    return "\n".join(f"<p>{chapter}장 {i+1}번째 문단 &mdash; {text * 2}</p>" for i in range(n))

def chapter(title, num, xhtml_ns_extra="", epub3=False):
    doctype = "" if epub3 else '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">\n'
    note = f'<p id="note{num}">각주 {num}: 같은 장 안의 링크 대상입니다.</p>'
    links = (f'<p><a href="#note{num}">이 장의 각주로</a> · '
             f'<a href="ch{(num % 3) + 1}.xhtml#top">다른 장으로</a> · '
             f'<a href="https://example.com/">외부 링크</a></p>')
    return f"""<?xml version="1.0" encoding="utf-8"?>
{doctype}<html xmlns="http://www.w3.org/1999/xhtml"{xhtml_ns_extra} xml:lang="ko">
<head><title>{title}</title><link rel="stylesheet" type="text/css" href="../styles/book.css"/></head>
<body>
<h1 id="top">{title}</h1>
{links}
<div class="figure"><img src="../images/blue.png" alt="그림"/></div>
{paragraphs(18 + num * 6, num)}
{note}
</body></html>"""

def write(name, files):
    path = os.path.join(OUT, name)
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for n, data in files.items():
            z.writestr(n, data, compress_type=zipfile.ZIP_DEFLATED)
    print("wrote", os.path.normpath(path))

CONTAINER = """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""

TITLES = ["첫째 장: 시작", "둘째 장: 여정", "셋째 장: 끝"]

def common(epub3):
    files = {"META-INF/container.xml": CONTAINER,
             "OEBPS/styles/book.css": CSS,
             "OEBPS/images/blue.png": PNG,
             "OEBPS/images/dot.png": PNG}
    ns = ' xmlns:epub="http://www.idpf.org/2007/ops"' if epub3 else ""
    for i, t in enumerate(TITLES, 1):
        files[f"OEBPS/text/ch{i}.xhtml"] = chapter(t, i, ns, epub3)
    return files

def manifest_items():
    items = ['<item id="css" href="styles/book.css" media-type="text/css"/>',
             '<item id="img" href="images/blue.png" media-type="image/png"/>',
             '<item id="dot" href="images/dot.png" media-type="image/png"/>']
    items += [f'<item id="ch{i}" href="text/ch{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(1, 4)]
    return "\n".join(items)

SPINE = "\n".join(f'<itemref idref="ch{i}"/>' for i in range(1, 4))

# EPUB 2: NCX table of contents.
e2 = common(False)
e2["OEBPS/content.opf"] = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>샘플 책 (EPUB 2)</dc:title><dc:creator>EPUB 뷰어</dc:creator><dc:language>ko</dc:language>
<dc:identifier id="id">sample-epub2</dc:identifier></metadata>
<manifest>{manifest_items()}
<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest>
<spine toc="ncx">{SPINE}</spine></package>"""
points = "\n".join(
    f'<navPoint id="n{i}" playOrder="{i}"><navLabel><text>{t}</text></navLabel><content src="text/ch{i}.xhtml"/>'
    + (f'<navPoint id="n{i}b" playOrder="{i}0"><navLabel><text>각주 {i}</text></navLabel><content src="text/ch{i}.xhtml#note{i}"/></navPoint>' if i == 2 else "")
    + "</navPoint>" for i, t in enumerate(TITLES, 1))
e2["OEBPS/toc.ncx"] = f"""<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head/>
<docTitle><text>샘플 책</text></docTitle><navMap>{points}</navMap></ncx>"""
write("sample-epub2.epub", e2)

# EPUB 3: navigation document.
e3 = common(True)
e3["OEBPS/content.opf"] = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>샘플 책 (EPUB 3)</dc:title><dc:creator>EPUB 뷰어</dc:creator><dc:language>ko</dc:language>
<dc:identifier id="id">sample-epub3</dc:identifier><meta property="dcterms:modified">2026-10-30T00:00:00Z</meta></metadata>
<manifest>{manifest_items()}
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest>
<spine>{SPINE}</spine></package>"""
lis = "\n".join(f'<li><a href="text/ch{i}.xhtml">{t}</a></li>' for i, t in enumerate(TITLES, 1))
e3["OEBPS/nav.xhtml"] = f"""<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>목차</title></head><body>
<nav epub:type="toc"><h1>목차</h1><ol>{lis}</ol></nav></body></html>"""
write("sample-epub3.epub", e3)

# Covers as real books make them: Calibre's SVG cover, an <img> at 100%
# height (Sigil, InDesign) and a 100vh box, before one chapter.
import struct, zlib

def png(w, h):
    """A w x h PNG: a dark blue cover with a light band."""
    rows = b""
    for y in range(h):
        band = h * 0.35 < y < h * 0.5
        rows += b"\x00" + (bytes((240, 200, 90)) if band else bytes((30, 50, 110))) * w
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))

COVERS = {
    "cover-svg": """<div><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1"
 width="100%" height="100%" viewBox="0 0 600 800" preserveAspectRatio="none">
<image width="600" height="800" xlink:href="../images/cover.png"/></svg></div>""",
    "cover-img": """<div class="cover"><img src="../images/cover.png" alt="표지"/></div>""",
    "cover-vh": """<div style="height:100vh;text-align:center"><img style="height:100%" src="../images/cover.png" alt="표지"/></div>""",
}
COVER_CSS = """@page { padding: 0; margin: 0 }
body { text-align: center; padding: 0; margin: 0; height: 100%; }
.cover { height: 100%; text-align: center; }
.cover img { height: 100%; max-width: 100%; }
"""
ec = common(True)
ec["OEBPS/images/cover.png"] = png(600, 800)
ec["OEBPS/styles/cover.css"] = COVER_CSS
for name, body in COVERS.items():
    ec[f"OEBPS/text/{name}.xhtml"] = f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="ko">
<head><title>표지</title><link rel="stylesheet" type="text/css" href="../styles/cover.css"/></head>
<body epub:type="cover">{body}</body></html>"""
cover_items = "\n".join(f'<item id="{n}" href="text/{n}.xhtml" media-type="application/xhtml+xml"/>' for n in COVERS)
ec["OEBPS/content.opf"] = f"""<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>표지 시험 책</dc:title><dc:language>ko</dc:language><dc:identifier id="id">sample-cover</dc:identifier>
<meta property="dcterms:modified">2026-11-26T00:00:00Z</meta></metadata>
<manifest>{manifest_items()}
<item id="cover-image" href="images/cover.png" media-type="image/png" properties="cover-image"/>
<item id="cover-css" href="styles/cover.css" media-type="text/css"/>
{cover_items}</manifest>
<spine>{"".join(f'<itemref idref="{n}"/>' for n in COVERS)}<itemref idref="ch1"/></spine></package>"""
write("sample-cover.epub", ec)
