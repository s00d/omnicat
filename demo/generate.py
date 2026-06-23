#!/usr/bin/env python3
"""Create minimal valid binary fixtures for omnicat demo."""

from __future__ import annotations

import base64
import math
import struct
import sys
import zipfile
import zlib
from pathlib import Path


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)


def write_png_rgb(path: Path, width: int, height: int, pixel_fn) -> None:
    """Write an 8-bit RGB PNG; pixel_fn(x, y) -> (r, g, b)."""
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            r, g, b = pixel_fn(x, y, width, height)
            rows.extend((r, g, b))
    compressed = zlib.compress(bytes(rows), 9)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n"
    png += _png_chunk(b"IHDR", ihdr)
    png += _png_chunk(b"IDAT", compressed)
    png += _png_chunk(b"IEND", b"")
    path.write_bytes(png)


def write_png(path: Path) -> None:
    # 64x64 diagonal gradient — visible in terminal image preview
    write_png_rgb(
        path,
        64,
        64,
        lambda x, y, w, h: (
            int(40 + 215 * x / max(w - 1, 1)),
            int(40 + 180 * y / max(h - 1, 1)),
            int(120 + 80 * (x + y) / max(w + h - 2, 1)),
        ),
    )


def write_png_icon(path: Path) -> None:
    # 32x32 checker + accent block for small preview
    def px(x: int, y: int, _w: int, _h: int) -> tuple[int, int, int]:
        if 8 <= x < 24 and 8 <= y < 24:
            return (108, 158, 255)
        if (x // 4 + y // 4) % 2 == 0:
            return (48, 52, 70)
        return (30, 32, 48)

    write_png_rgb(path, 32, 32, px)


def write_png_wide(path: Path) -> None:
    # 128x48 wide banner-style image
    write_png_rgb(
        path,
        128,
        48,
        lambda x, y, w, h: (
            int(20 + 200 * x / max(w - 1, 1)),
            int(max(0, min(255, 60 + 120 * math.sin(x / 12) * math.cos(y / 8)))),
            int(90 + 100 * y / max(h - 1, 1)),
        ),
    )


def write_gif(path: Path) -> None:
    # 1x1 GIF
    path.write_bytes(
        base64.b64decode(
            "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
        )
    )


def write_pdf(path: Path) -> None:
    """Minimal valid PDF with extractable text (offsets computed at write time)."""
    chunks: list[bytes] = []

    def emit(data: bytes) -> int:
        offset = sum(len(c) for c in chunks)
        chunks.append(data)
        return offset

    emit(b"%PDF-1.4\n")

    offsets: list[int] = []
    offsets.append(emit(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"))
    offsets.append(emit(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"))
    offsets.append(
        emit(
            b"3 0 obj\n"
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] "
            b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\n"
            b"endobj\n"
        )
    )
    offsets.append(
        emit(b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n")
    )
    stream = b"BT /F1 18 Tf 72 72 Td (omnicat PDF demo) Tj ET"
    offsets.append(
        emit(
            f"5 0 obj\n<< /Length {len(stream)} >>\nstream\n".encode("ascii")
            + stream
            + b"\nendstream\nendobj\n"
        )
    )

    xref_start = sum(len(c) for c in chunks)
    xref = b"xref\n0 6\n0000000000 65535 f \n"
    for off in offsets:
        xref += f"{off:010d} 00000 n \n".encode("ascii")
    chunks.append(xref)
    chunks.append(
        b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n"
        + str(xref_start).encode("ascii")
        + b"\n%%EOF\n"
    )
    path.write_bytes(b"".join(chunks))


def write_wav(path: Path, duration: float = 6.0, freq: float = 440.0) -> None:
    """Mono 16-bit PCM WAV with a short audible tone (fade in/out)."""
    sample_rate = 44100
    n_samples = int(sample_rate * duration)
    samples = bytearray()
    for i in range(n_samples):
        t = i / sample_rate
        env = min(1.0, t / 0.08, (duration - t) / 0.12) if duration > 0.2 else 1.0
        # gentle two-tone demo: A4 then E5 after half
        f = freq if t < duration / 2 else freq * 1.5
        val = int(32767 * 0.55 * env * math.sin(2 * math.pi * f * t))
        samples.extend(struct.pack("<h", val))
    data = bytes(samples)
    byte_rate = sample_rate * 2
    block_align = 2
    bits = 16
    data_size = len(data)
    riff_size = 36 + data_size
    header = struct.pack(
        "<4sI4s4sIHHIIHH4sI",
        b"RIFF",
        riff_size,
        b"WAVE",
        b"fmt ",
        16,
        1,
        1,
        sample_rate,
        byte_rate,
        block_align,
        bits,
        b"data",
        data_size,
    )
    path.write_bytes(header + data)


def zip_write(path: Path, files: dict[str, str | bytes]) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        for name, content in files.items():
            if isinstance(content, str):
                zf.writestr(name, content)
            else:
                zf.writestr(name, content)


def write_docx(path: Path) -> None:
    doc = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>omnicat DOCX demo — paragraph one.</w:t></w:r></w:p>
    <w:p><w:r><w:t>Second paragraph for document preview.</w:t></w:r></w:p>
  </w:body>
</w:document>"""
    zip_write(path, {"word/document.xml": doc})


def write_odt(path: Path) -> None:
    content = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:text>
    <text:p>omnicat ODT demo paragraph.</text:p>
  </office:text></office:body>
</office:document-content>"""
    zip_write(path, {"content.xml": content})


def write_xlsx(path: Path) -> None:
    files = {
        "[Content_Types].xml": """<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>""",
        "_rels/.rels": """<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>""",
        "xl/workbook.xml": """<?xml version="1.0"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="Demo" sheetId="1" r:id="rId1"/></sheets>
</workbook>""",
        "xl/_rels/workbook.xml.rels": """<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>""",
        "xl/sharedStrings.xml": """<?xml version="1.0"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="4" uniqueCount="4">
  <si><t>name</t></si><si><t>score</t></si><si><t>alpha</t></si><si><t>42</t></si>
</sst>""",
        "xl/worksheets/sheet1.xml": """<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
    <row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2" t="s"><v>3</v></c></row>
  </sheetData>
</worksheet>""",
    }
    zip_write(path, files)


def write_ods(path: Path) -> None:
    content = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:spreadsheet>
    <table:table table:name="Sheet1">
      <table:table-row>
        <table:table-cell><text:p>item</text:p></table:table-cell>
        <table:table-cell><text:p>qty</text:p></table:table-cell>
      </table:table-row>
      <table:table-row>
        <table:table-cell><text:p>widget</text:p></table:table-cell>
        <table:table-cell><text:p>3</text:p></table:table-cell>
      </table:table-row>
    </table:table>
  </office:spreadsheet></office:body>
</office:document-content>"""
    zip_write(path, {"content.xml": content})


def write_pptx(path: Path) -> None:
    slide = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:sp><p:txBody><a:p><a:r><a:t>Slide 1 — omnicat PPTX demo</a:t></a:r></a:p></p:txBody></p:sp>
  </p:spTree></p:cSld>
</p:sld>"""
    slide2 = slide.replace("Slide 1", "Slide 2")
    zip_write(
        path,
        {
            "ppt/slides/slide1.xml": slide,
            "ppt/slides/slide2.xml": slide2,
        },
    )


def write_odp(path: Path) -> None:
    content = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
  <office:body><office:presentation>
    <draw:page><text:p>ODP slide one</text:p></draw:page>
    <draw:page><text:p>ODP slide two</text:p></draw:page>
  </office:presentation></office:body>
</office:document-content>"""
    zip_write(path, {"content.xml": content})


def write_epub(path: Path) -> None:
    files = {
        "mimetype": "application/epub+zip",
        "META-INF/container.xml": """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>""",
        "OEBPS/content.opf": """<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>omnicat EPUB demo</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="chapter.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>""",
        "OEBPS/chapter.xhtml": """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>EPUB chapter body for preview.</p></body></html>""",
    }
    # mimetype must be stored uncompressed first for valid epub
    with zipfile.ZipFile(path, "w") as zf:
        zf.writestr("mimetype", files["mimetype"], compress_type=zipfile.ZIP_STORED)
        for name, content in files.items():
            if name == "mimetype":
                continue
            zf.writestr(name, content, compress_type=zipfile.ZIP_DEFLATED)


def _large_epub_chapter_body(page: int, paragraphs: int = 6) -> str:
    lines = [
        f'    <h1>Page {page}</h1>',
        f'    <p><strong>OMNICAT_PAGE_{page:03d}</strong> — pagination stress fixture.</p>',
    ]
    for para in range(1, paragraphs + 1):
        lines.append(
            f"    <p>Paragraph {para} on page {page}: "
            f"Lorem ipsum dolor sit amet, consectetur adipiscing elit. "
            f"Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. "
            f"Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris "
            f"nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in "
            f"reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla "
            f"pariatur.</p>"
        )
    return "\n".join(lines)


def write_large_epub(path: Path, pages: int = 100) -> None:
    """EPUB with one spine item per page — converted to MOBI for pagination tests."""
    manifest_items = []
    spine_items = []
    chapter_files: dict[str, str] = {}

    for i in range(1, pages + 1):
        chap_id = f"ch{i:03d}"
        href = f"chapter{i:03d}.xhtml"
        manifest_items.append(
            f'    <item id="{chap_id}" href="{href}" media-type="application/xhtml+xml"/>'
        )
        spine_items.append(f'    <itemref idref="{chap_id}"/>')
        body = _large_epub_chapter_body(i)
        chapter_files[f"OEBPS/{href}"] = f"""<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Page {i}</title></head>
<body>
{body}
</body>
</html>"""

    content_opf = f"""<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>omnicat large book ({pages} pages)</dc:title>
    <dc:creator>Demo Author</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="uid">omnicat-large-{pages}</dc:identifier>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
{chr(10).join(manifest_items)}
  </manifest>
  <spine toc="ncx">
{chr(10).join(spine_items)}
  </spine>
</package>"""

    nav_points = []
    for i in range(1, pages + 1):
        nav_points.append(
            f"""    <navPoint id="nav{i}" playOrder="{i}">
      <navLabel><text>Page {i}</text></navLabel>
      <content src="chapter{i:03d}.xhtml"/>
    </navPoint>"""
        )
    toc_ncx = f"""<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="omnicat-large-{pages}"/>
    <meta name="dtb:depth" content="1"/>
    <meta name="dtb:totalPageCount" content="{pages}"/>
    <meta name="dtb:maxPageNumber" content="{pages}"/>
  </head>
  <docTitle><text>omnicat large book</text></docTitle>
  <navMap>
{chr(10).join(nav_points)}
  </navMap>
</ncx>"""

    static_files = {
        "mimetype": "application/epub+zip",
        "META-INF/container.xml": """<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>""",
        "OEBPS/content.opf": content_opf,
        "OEBPS/toc.ncx": toc_ncx,
    }
    static_files.update(chapter_files)

    with zipfile.ZipFile(path, "w") as zf:
        zf.writestr("mimetype", static_files["mimetype"], compress_type=zipfile.ZIP_STORED)
        for name, content in static_files.items():
            if name == "mimetype":
                continue
            zf.writestr(name, content, compress_type=zipfile.ZIP_DEFLATED)


def write_cbz(path: Path, image_path: Path) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        for i in range(1, 4):
            zf.write(image_path, f"page{i:03d}.png")


def write_legacy_doc(path: Path) -> None:
    # Minimal OLE2 header so extension/mime detection works; content may be Unsupported.
    header = (
        b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1"
        + b"\x00" * 512
        + b"omnicat legacy DOC demo printable text padding"
    )
    path.write_bytes(header)


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    pages = int(sys.argv[2]) if len(sys.argv) > 2 else 100
    write_png(out / "sample.png")
    write_png_icon(out / "sample-icon.png")
    write_png_wide(out / "sample-wide.png")
    write_gif(out / "sample.gif")
    write_pdf(out / "sample.pdf")
    write_wav(out / "sample.wav")
    write_docx(out / "sample.docx")
    write_odt(out / "sample.odt")
    write_xlsx(out / "sample.xlsx")
    write_ods(out / "sample.ods")
    write_pptx(out / "sample.pptx")
    write_odp(out / "sample.odp")
    write_epub(out / "sample.epub")
    write_large_epub(out / "sample-large.epub", pages=pages)
    write_legacy_doc(out / "sample.doc")
    write_cbz(out / "sample.cbz", out / "sample-icon.png")
    (out / "sample.bin").write_bytes(bytes([0x00, 0x01, 0x02, 0xCA, 0xFE, 0xBA, 0xBE]) + b"omnicat-binary-demo")
    print(f"python fixtures -> {out} (large epub: {pages} pages)")


if __name__ == "__main__":
    main()
