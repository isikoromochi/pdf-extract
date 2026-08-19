//! Golden tests over the hand-written PDFs in `tests/fixtures`.
//!
//! These differ from `tests.rs` in two ways: they need no network access, and
//! they pin down the extracted text rather than only checking that extraction
//! doesn't crash. Regenerate the fixtures with
//! `python tests/fixtures/generate.py`.

use pdf_extract::{extract_text, extract_text_by_pages};

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
    // /Differences. Resource names are scoped to a page's resource dictionary
    // (PDF 32000-1 7.8.3), so the two pages must decode differently.
    let path = fixture("font_resource_scope");

    // `output_doc_page` builds a fresh Processor per page, so this path is
    // already correct. Pin it so it stays that way.
    assert_eq!(
        extract_text_by_pages(&path).unwrap(),
        vec!["\n\nABC".to_string(), "\n\n\u{2022}\u{2020}\u{2021}".to_string()]
    );

    // `output_doc` reuses one Processor for the whole document, and its font
    // cache is keyed on the resource name alone, so page 2 is decoded with
    // page 1's font.
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
    // show) -- PDF 32000-1 9.4.3. Only Tj is currently implemented, so the text
    // shown by ' and " is silently dropped.
    let text = extract_text(fixture("quote_operators")).unwrap();
    for shown in ["AAA", "BBB", "CCC"] {
        assert!(text.contains(shown), "{:?} missing from {:?}", shown, text);
    }
}

#[test]
#[ignore = "unbounded recursion in the `Do` handler overflows the stack, which \
            aborts the whole test binary rather than failing this test"]
fn self_referencing_xobject_terminates() {
    // A form XObject that invokes itself. Malformed, but extractors are handed
    // malformed input; whatever comes back, it must come back.
    let _ = extract_text(fixture("self_referencing_xobject"));
}
