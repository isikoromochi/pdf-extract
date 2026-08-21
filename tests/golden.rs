//! Golden tests over the hand-written PDFs in `tests/fixtures`.
//!
//! These differ from `tests.rs` in two ways: they need no network access, and
//! they pin down the extracted text rather than only checking that extraction
//! doesn't crash. Regenerate the fixtures with
//! `python tests/fixtures/generate.py`.

use pdf_extract::{PdfExtractError, extract_text, extract_text_by_pages};

fn fixture(name: &str) -> String {
    format!("tests/fixtures/{}.pdf", name)
}

#[test]
fn simple_text() {
    // One page, one core font, no encoding tricks: the baseline that tells us
    // whether a regression is specific to a feature or affects everything.
    assert_eq!(extract_text(fixture("simple_text")).unwrap(), "\n\nHello World");
}

#[test]
fn font_resources_are_scoped_per_page() {
    // Both pages show `(ABC) Tj` through a resource named /F1, but each page's
    // /F1 points at a different font dictionary -- page 2 remaps A, B and C via
    // /Differences. Resource names are scoped to the resource dictionary they
    // appear in (32000-1 7.8.3), so the two pages must decode differently.
    let path = fixture("font_resource_scope");

    assert_eq!(
        extract_text_by_pages(&path).unwrap(),
        vec!["\n\nABC".to_string(), "\n\n\u{2022}\u{2020}\u{2021}".to_string()]
    );

    // `output_doc` drives every page through one Processor, so this is the path
    // that regresses if the font cache is ever keyed on the resource name again.
    let whole = extract_text(&path).unwrap();
    assert!(
        whole.contains('\u{2022}'),
        "page 2 was decoded with page 1's /F1: {:?}",
        whole
    );
}

#[test]
fn all_text_showing_operators() {
    // Tj, ' (next line and show) and " (set word/char spacing, next line and
    // show) -- 32000-1 9.4.3. The fixture shows one string through each.
    //
    // The blank line between each pair comes from PlainTextOutput: the fixture
    // sets 40pt leading on 24pt text, which trips both of its newline rules. It
    // is a property of the layout heuristics, not of these operators.
    assert_eq!(
        extract_text(fixture("quote_operators")).unwrap(),
        "\n\nAAA\n\nBBB\n\nCCC"
    );
}

#[test]
fn self_referencing_xobject_terminates() {
    // A form XObject that invokes itself. Malformed, but extractors are handed
    // malformed input; whatever comes back, it must come back. Nothing is drawn
    // but the nested `Do`s, so the page is empty.
    assert_eq!(extract_text(fixture("self_referencing_xobject")).unwrap(), "");
}

#[test]
fn cyclic_page_parent_terminates() {
    // /Resources is inherited (32000-1 7.7.3.4), so resolving it walks /Parent
    // -- and this page is its own parent. The walk has to give up rather than
    // recurse until the stack runs out. The page draws only a rectangle, so a
    // successful run produces no text.
    assert_eq!(extract_text(fixture("cyclic_page_parent")).unwrap(), "");
}

#[test]
fn dangling_reference_is_an_error() {
    // /F1 points at an object the file does not contain. Corruption like this is
    // ordinary, and has to arrive as an error the caller can handle rather than
    // as a panic.
    let err = extract_text(fixture("dangling_font_reference")).unwrap_err();
    assert!(
        matches!(err, PdfExtractError::PdfError(_)),
        "expected the dangling reference to surface as a PDF error, got {:?}",
        err
    );
}

#[test]
fn missing_required_entry_is_an_error() {
    // 32000-1 9.7.4 requires /DescendantFonts on a Type 0 font. Its absence is a
    // statement about the file, so it should read as one.
    let err = extract_text(fixture("type0_without_descendants")).unwrap_err();
    let PdfExtractError::MalformedPdf(message) = &err else {
        panic!("expected a MalformedPdf error, got {:?}", err);
    };
    assert!(
        message.contains("/DescendantFonts"),
        "the error should name the entry that is missing: {:?}",
        message
    );
}

#[test]
fn missing_type3_width_is_an_error() {
    // The font's /Widths covers code 65 and the page shows code 66. A Type 3
    // font carries no metrics of its own to fall back on, so the gap reaches
    // the caller through PdfFont::get_width rather than aborting.
    let err = extract_text(fixture("type3_missing_width")).unwrap_err();
    let PdfExtractError::MalformedPdf(message) = &err else {
        panic!("expected a MalformedPdf error, got {:?}", err);
    };
    assert!(
        message.contains("/Widths"),
        "the error should say which table came up short: {:?}",
        message
    );
}

#[test]
fn differences_outside_the_encoding_table_are_ignored() {
    // /Differences names code 300 and code -1, neither of which a simple font
    // can show: its codes are single bytes (32000-1 9.6.6.1). Both used to be
    // written straight into a 256-entry table, so the file panicked rather than
    // extracting. The in-range entry -- 65 to /dagger -- still has to apply.
    assert_eq!(
        extract_text(fixture("differences_out_of_range")).unwrap(),
        "\n\n\u{2020}"
    );
}

#[test]
fn type3_differences_outside_the_encoding_table_are_ignored() {
    // As above, on a Type 3 font: it builds its encoding table through its own
    // path, so it needs its own bounds check and its own regression test.
    assert_eq!(
        extract_text(fixture("type3_differences_out_of_range")).unwrap(),
        "\n\n\u{2020}"
    );
}
