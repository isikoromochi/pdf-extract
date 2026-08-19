//! Extract the text and drawing operations from a PDF.
//!
//! The layers, innermost first: [`object`] reads values out of the PDF object
//! graph, [`font`] and [`color`] interpret the resources a page refers to,
//! [`content`] executes a page's content stream, and [`output`] receives what
//! that execution produces.

mod color;
mod content;
mod core_fonts;
mod encodings;
mod error;
mod font;
mod function;
mod geom;
mod glyphnames;
mod object;
mod output;
mod strings;
mod zapfglyphnames;

pub use crate::color::{AlternateColorSpace, CalGray, CalRGB, ColorSpace, Lab, Separation};
pub use crate::error::PdfExtractError;
pub use crate::geom::{MediaBox, Path, PathOp, Space, Transform};
pub use crate::output::{ConvertToFmt, HTMLOutput, OutputDev, PlainTextOutput, SVGOutput, WriteAdapter};
pub use lopdf;

use crate::content::Processor;
use crate::object::{get, get_inherited};
use log::error;
use lopdf::encryption::DecryptionError;
use lopdf::{Dictionary, Document, Error, ObjectId};

/// Extract the text from a pdf at `path` and return a `String` with the results
pub fn extract_text<P: std::convert::AsRef<std::path::Path>>(path: P) -> Result<String, PdfExtractError> {
   let mut s = String::new();
   {
      let mut output = PlainTextOutput::new(&mut s);
      let mut doc = Document::load(path)?;
      maybe_decrypt(&mut doc)?;
      output_doc(&doc, &mut output)?;
   }
   Ok(s)
}

fn maybe_decrypt(doc: &mut Document) -> Result<(), PdfExtractError> {
   if !doc.is_encrypted() {
      return Ok(());
   }

   if let Err(e) = doc.decrypt("") {
      if let Error::Decryption(DecryptionError::IncorrectPassword) = e {
         error!(
            "Encrypted documents must be decrypted with a password using {{extract_text|extract_text_from_mem|output_doc}}_encrypted"
         )
      }

      return Err(PdfExtractError::PdfError(e));
   }

   Ok(())
}

pub fn extract_text_encrypted<P: std::convert::AsRef<std::path::Path>>(
   path: P,
   password: &str,
) -> Result<String, PdfExtractError> {
   let mut s = String::new();
   {
      let mut output = PlainTextOutput::new(&mut s);
      let mut doc = Document::load(path)?;
      output_doc_encrypted(&mut doc, &mut output, password)?;
   }
   Ok(s)
}

pub fn extract_text_from_mem(buffer: &[u8]) -> Result<String, PdfExtractError> {
   let mut s = String::new();
   {
      let mut output = PlainTextOutput::new(&mut s);
      let mut doc = Document::load_mem(buffer)?;
      maybe_decrypt(&mut doc)?;
      output_doc(&doc, &mut output)?;
   }
   Ok(s)
}

pub fn extract_text_from_mem_encrypted(buffer: &[u8], password: &str) -> Result<String, PdfExtractError> {
   let mut s = String::new();
   {
      let mut output = PlainTextOutput::new(&mut s);
      let mut doc = Document::load_mem(buffer)?;
      output_doc_encrypted(&mut doc, &mut output, password)?;
   }
   Ok(s)
}

fn extract_text_by_page(doc: &Document, page_num: u32) -> Result<String, PdfExtractError> {
   let mut s = String::new();
   {
      let mut output = PlainTextOutput::new(&mut s);
      output_doc_page(doc, &mut output, page_num)?;
   }
   Ok(s)
}

/// Extract the text from a pdf at `path` and return a `Vec<String>` with the results separately by page
pub fn extract_text_by_pages<P: std::convert::AsRef<std::path::Path>>(path: P) -> Result<Vec<String>, PdfExtractError> {
   let mut v = Vec::new();
   {
      let mut doc = Document::load(path)?;
      maybe_decrypt(&mut doc)?;
      let mut page_num = 1;
      while let Ok(content) = extract_text_by_page(&doc, page_num) {
         v.push(content);
         page_num += 1;
      }
   }
   Ok(v)
}

pub fn extract_text_by_pages_encrypted<P: std::convert::AsRef<std::path::Path>>(
   path: P,
   password: &str,
) -> Result<Vec<String>, PdfExtractError> {
   let mut v = Vec::new();
   {
      let mut doc = Document::load(path)?;
      doc.decrypt(password)?;
      let mut page_num = 1;
      while let Ok(content) = extract_text_by_page(&doc, page_num) {
         v.push(content);
         page_num += 1;
      }
   }
   Ok(v)
}

pub fn extract_text_from_mem_by_pages(buffer: &[u8]) -> Result<Vec<String>, PdfExtractError> {
   let mut v = Vec::new();
   {
      let mut doc = Document::load_mem(buffer)?;
      maybe_decrypt(&mut doc)?;
      let mut page_num = 1;
      while let Ok(content) = extract_text_by_page(&doc, page_num) {
         v.push(content);
         page_num += 1;
      }
   }
   Ok(v)
}

pub fn extract_text_from_mem_by_pages_encrypted(buffer: &[u8], password: &str) -> Result<Vec<String>, PdfExtractError> {
   let mut v = Vec::new();
   {
      let mut doc = Document::load_mem(buffer)?;
      doc.decrypt(password)?;
      let mut page_num = 1;
      while let Ok(content) = extract_text_by_page(&doc, page_num) {
         v.push(content);
         page_num += 1;
      }
   }
   Ok(v)
}

pub fn output_doc_encrypted(
   doc: &mut Document,
   output: &mut dyn OutputDev,
   password: &str,
) -> Result<(), PdfExtractError> {
   doc.decrypt(password)?;
   output_doc(doc, output)
}

/// Parse a given document and output it to `output`
pub fn output_doc(doc: &Document, output: &mut dyn OutputDev) -> Result<(), PdfExtractError> {
   if doc.is_encrypted() {
      error!(
         "Encrypted documents must be decrypted with a password using {{extract_text|extract_text_from_mem|output_doc}}_encrypted"
      );
   }
   let empty_resources = Dictionary::new();
   let pages = doc.get_pages();
   let mut p = Processor::new();
   for dict in pages {
      let page_num = dict.0;
      let object_id = dict.1;
      output_doc_inner(page_num, object_id, doc, &mut p, output, &empty_resources)?;
   }
   Ok(())
}

pub fn output_doc_page(doc: &Document, output: &mut dyn OutputDev, page_num: u32) -> Result<(), PdfExtractError> {
   if doc.is_encrypted() {
      error!(
         "Encrypted documents must be decrypted with a password using {{extract_text|extract_text_from_mem|output_doc}}_encrypted"
      );
   }
   let empty_resources = Dictionary::new();
   let pages = doc.get_pages();
   let object_id = pages.get(&page_num).ok_or(lopdf::Error::PageNumberNotFound(page_num))?;
   let mut p = Processor::new();
   output_doc_inner(page_num, *object_id, doc, &mut p, output, &empty_resources)?;
   Ok(())
}

fn output_doc_inner<'a>(
   page_num: u32,
   object_id: ObjectId,
   doc: &'a Document,
   p: &mut Processor<'a>,
   output: &mut dyn OutputDev,
   empty_resources: &'a Dictionary,
) -> Result<(), PdfExtractError> {
   let page_dict = doc.get_object(object_id)?.as_dict()?;
   // XXX: Some pdfs lack a Resources directory
   let resources = get_inherited(doc, page_dict, b"Resources", 0).unwrap_or(empty_resources);
   // pdfium searches up the page tree for MediaBoxes as needed. Asking for
   // `[f64; 4]` rather than a `Vec` puts the "a rectangle has four numbers"
   // check in one place instead of at each index.
   let media_box: [f64; 4] = get_inherited(doc, page_dict, b"MediaBox", 0).ok_or_else(|| {
      PdfExtractError::MalformedPdf("no /MediaBox on this page or any of its ancestors".to_owned())
   })?;
   let media_box = MediaBox {
      llx: media_box[0],
      lly: media_box[1],
      urx: media_box[2],
      ury: media_box[3],
   };
   let art_box = get::<Option<[f64; 4]>>(doc, page_dict, b"ArtBox")?.map(|x| (x[0], x[1], x[2], x[3]));
   output.begin_page(page_num, &media_box, art_box)?;
   p.process_stream(doc, doc.get_page_content(object_id), resources, &media_box, output, 0)?;
   output.end_page()?;
   Ok(())
}
