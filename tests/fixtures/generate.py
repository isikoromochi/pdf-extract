#!/usr/bin/env python3
"""Regenerate the hand-written PDF fixtures used by tests/golden.rs.

    python tests/fixtures/generate.py

Each fixture is a minimal, uncompressed PDF built so that a single behaviour of
the extractor can be checked in isolation. They are checked in so the tests run
offline; this script exists so they can be audited and adjusted.
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def stream(dict_body, content):
    """Build a stream object with a correct /Length."""
    return (b"<< " + dict_body + b" /Length %d >>\nstream\n" % len(content)
            + content + b"\nendstream")


def build(objs, name):
    """Serialise {num: body} into a PDF with a valid xref table."""
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for num in sorted(objs):
        offsets[num] = len(out)
        out += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"
    xref_pos = len(out)
    size = max(objs) + 1
    out += b"xref\n0 %d\n" % size + b"0000000000 65535 f \n"
    for num in range(1, size):
        out += b"%010d 00000 n \n" % offsets[num]
    out += (b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
            % (size, xref_pos))
    path = os.path.join(HERE, name)
    with open(path, "wb") as f:
        f.write(bytes(out))
    print("wrote %s (%d bytes)" % (name, len(out)))


# --------------------------------------------------------------------------
# 1. A baseline document: one page, one core font, no surprises.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (Hello World) Tj ET"),
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}, "simple_text.pdf")


# --------------------------------------------------------------------------
# 2. Two pages that both show `(ABC) Tj` through a resource named /F1, but
#    whose /F1 entries point at different font dictionaries. Resource names are
#    scoped to a page's resource dictionary (PDF 32000-1 7.8.3), so page 2 must
#    decode through its own /Differences, not page 1's font.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: (b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2"
        b" /MediaBox [0 0 200 200] >>"),
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 7 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (ABC) Tj ET"),
    5: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 8 0 R >> >>"
        b" /Contents 6 0 R >>"),
    6: stream(b"", b"BT /F1 24 Tf 20 100 Td (ABC) Tj ET"),
    7: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    8: (b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding"
        b" << /Type /Encoding /Differences [65 /bullet /dagger /daggerdbl] >>"
        b" >>"),
}, "font_resource_scope.pdf")


# --------------------------------------------------------------------------
# 3. The three text-showing operators: Tj, ' (next line and show) and
#    " (set word/char spacing, next line and show). See 32000-1 9.4.3.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 40 TL 20 150 Td (AAA) Tj (BBB) ' 0 0 (CCC) \" ET"),
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}, "quote_operators.pdf")


# --------------------------------------------------------------------------
# 4. A form XObject whose content stream invokes itself. Malformed, but it is
#    the kind of input a text extractor is handed; it must not take the process
#    down with it.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R"
        b" /Resources << /XObject << /X1 5 0 R >> >> /Contents 4 0 R >>"),
    4: stream(b"", b"/X1 Do"),
    5: stream(b"/Type /XObject /Subtype /Form /BBox [0 0 200 200]"
              b" /Resources << /XObject << /X1 5 0 R >> >>", b"/X1 Do"),
}, "self_referencing_xobject.pdf")


# --------------------------------------------------------------------------
# 5. A page whose /Parent points at itself and which carries no /Resources.
#    /Resources is an inherited attribute (32000-1 7.7.3.4), so resolving it
#    walks /Parent -- and here that walk never reaches the root. The page draws
#    a rectangle rather than text so that the missing /Resources is not itself
#    fatal, leaving the cycle as the only thing under test.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: (b"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 200 200]"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"0 0 100 100 re f"),
}, "cyclic_page_parent.pdf")


# --------------------------------------------------------------------------
# 6. /F1 resolves to an object that is not in the file. Following a dangling
#    reference is the most ordinary kind of corruption there is, and it has to
#    come back as an error rather than take the process down.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 99 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (X) Tj ET"),
}, "dangling_font_reference.pdf")


# --------------------------------------------------------------------------
# 7. A Type 0 font with no /DescendantFonts, which 32000-1 9.7.4 requires. A
#    required entry that is simply absent is the other everyday failure.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (X) Tj ET"),
    5: (b"<< /Type /Font /Subtype /Type0 /BaseFont /Whatever"
        b" /Encoding /Identity-H >>"),
}, "type0_without_descendants.pdf")


# --------------------------------------------------------------------------
# 8. A Type 3 font whose /Widths covers only code 65, on a page that shows code
#    66. A glyph with no width is a statement about the file, and reaches the
#    caller through PdfFont::get_width.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (B) Tj ET"),
    5: (b"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 100 100]"
        b" /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << >>"
        b" /Encoding << /Type /Encoding /Differences [65 /A] >>"
        b" /FirstChar 65 /LastChar 65 /Widths [500] >>"),
}, "type3_missing_width.pdf")


# --------------------------------------------------------------------------
# 9. /Differences entries whose codes are not single-byte codes: one past the
#    end of the encoding table and one negative. A simple font can only ever
#    show codes 0..255 (32000-1 9.6.6.1), so neither entry is reachable -- but
#    both used to index straight into a 256-entry table. The third entry is in
#    range, and has to survive its neighbours.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (A) Tj ET"),
    5: (b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding"
        b" << /Type /Encoding"
        b" /Differences [300 /bullet -1 /daggerdbl 65 /dagger] >> >>"),
}, "differences_out_of_range.pdf")


# --------------------------------------------------------------------------
# 10. The same out-of-range /Differences codes on a Type 3 font, which builds
#     its encoding table through a separate path from a simple font's.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>",
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (A) Tj ET"),
    5: (b"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 100 100]"
        b" /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << >>"
        b" /Encoding << /Type /Encoding"
        b" /Differences [300 /bullet -1 /daggerdbl 65 /dagger] >>"
        b" /FirstChar 65 /LastChar 65 /Widths [500] >>"),
}, "type3_differences_out_of_range.pdf")


# --------------------------------------------------------------------------
# 11. Three pages, the middle one pointing /F1 at an object the file does not
#     contain. Damage is normally confined to the page carrying it, so the two
#     intact pages have to survive it -- and the run must not be cut short at
#     the damaged page, which is what reading an error as "no more pages" did.
# --------------------------------------------------------------------------
build({
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: (b"<< /Type /Pages /Kids [3 0 R 6 0 R 8 0 R] /Count 3"
        b" /MediaBox [0 0 200 200] >>"),
    3: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 4 0 R >>"),
    4: stream(b"", b"BT /F1 24 Tf 20 100 Td (PAGE-ONE) Tj ET"),
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    6: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 99 0 R >> >>"
        b" /Contents 7 0 R >>"),
    7: stream(b"", b"BT /F1 24 Tf 20 100 Td (PAGE-TWO) Tj ET"),
    8: (b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >>"
        b" /Contents 9 0 R >>"),
    9: stream(b"", b"BT /F1 24 Tf 20 100 Td (PAGE-THREE) Tj ET"),
}, "broken_middle_page.pdf")
