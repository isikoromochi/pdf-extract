//! Reading values out of the PDF object graph.
//!
//! PDF lets almost any value be an indirect reference (32000-1 7.3.10), so every
//! path that pulls a value out of a dictionary has to be prepared to follow one.
//! `maybe_deref` does that, and the `FromObj` / `FromOptObj` pair chains it with
//! a conversion to a Rust type.
//!
//! Two strictness levels run side by side, because the spec makes most entries
//! optional. The `maybe_*` helpers and `get::<Option<T>>` are lenient: an entry
//! that is absent and one that is present but unreadable both read as `None`.
//! `get::<T>` is strict, and says what it could not read.

use crate::error::PdfExtractError;
use crate::strings::pdf_to_utf8;
use log::warn;
use lopdf::{Dictionary, Document, Object, Stream};

fn malformed(what: String) -> PdfExtractError {
   PdfExtractError::MalformedPdf(what)
}

fn missing(key: &[u8]) -> PdfExtractError {
   malformed(format!("missing required entry /{}", String::from_utf8_lossy(key)))
}

/// A short name for an object's kind. `{:?}` on an `Object` can print an entire
/// stream, which is not what anyone wants to read in an error message.
fn kind(o: &Object) -> &'static str {
   match o {
      Object::Null => "null",
      Object::Boolean(_) => "a boolean",
      Object::Integer(_) => "an integer",
      Object::Real(_) => "a real number",
      Object::String(..) => "a string",
      Object::Name(_) => "a name",
      Object::Array(_) => "an array",
      Object::Dictionary(_) => "a dictionary",
      Object::Stream(_) => "a stream",
      Object::Reference(_) => "a reference",
   }
}

pub(crate) fn maybe_deref<'a>(doc: &'a Document, o: &'a Object) -> Result<&'a Object, PdfExtractError> {
   match o {
      &Object::Reference(r) => Ok(doc.get_object(r)?),
      _ => Ok(o),
   }
}

pub(crate) fn maybe_get_obj<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
   dict.get(key).ok().and_then(|o| maybe_deref(doc, o).ok())
}

/// Chains "look this key up" with "convert what was found", so that asking for
/// `T` or for `Option<T>` selects the strict or the lenient reading.
pub(crate) trait FromOptObj<'a>: Sized {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, key: &[u8]) -> Result<Self, PdfExtractError>;
}

/// Convert an object to `Self`, reporting why if it does not fit.
pub(crate) trait FromObj<'a>: Sized {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<Self, PdfExtractError>;
}

impl<'a, T: FromObj<'a>> FromOptObj<'a> for Option<T> {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, _key: &[u8]) -> Result<Self, PdfExtractError> {
      Ok(obj.and_then(|x| T::from_obj(doc, x).ok()))
   }
}

impl<'a, T: FromObj<'a>> FromOptObj<'a> for T {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, key: &[u8]) -> Result<Self, PdfExtractError> {
      T::from_obj(doc, obj.ok_or_else(|| missing(key))?)
   }
}

// we follow the same conventions as pdfium for when to support indirect objects:
// on arrays, streams and dicts
impl<'a, T: FromObj<'a>> FromObj<'a> for Vec<T> {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<Self, PdfExtractError> {
      maybe_deref(doc, obj)?.as_array()?.iter().map(|x| T::from_obj(doc, x)).collect()
   }
}

impl<'a, T: FromObj<'a>> FromObj<'a> for [T; 4] {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<Self, PdfExtractError> {
      let all: Vec<T> = FromObj::from_obj(doc, obj)?;
      let len = all.len();
      all.try_into().map_err(|_| malformed(format!("expected 4 elements, found {}", len)))
   }
}

impl<'a, T: FromObj<'a>> FromObj<'a> for [T; 3] {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<Self, PdfExtractError> {
      let all: Vec<T> = FromObj::from_obj(doc, obj)?;
      let len = all.len();
      all.try_into().map_err(|_| malformed(format!("expected 3 elements, found {}", len)))
   }
}

impl<'a> FromObj<'a> for f64 {
   fn from_obj(_doc: &Document, obj: &Object) -> Result<Self, PdfExtractError> {
      as_num(obj)
   }
}

impl<'a> FromObj<'a> for i64 {
   fn from_obj(_doc: &Document, obj: &Object) -> Result<Self, PdfExtractError> {
      match obj {
         &Object::Integer(i) => Ok(i),
         _ => Err(malformed(format!("expected an integer, found {}", kind(obj)))),
      }
   }
}

impl<'a> FromObj<'a> for &'a Dictionary {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<&'a Dictionary, PdfExtractError> {
      Ok(maybe_deref(doc, obj)?.as_dict()?)
   }
}

impl<'a> FromObj<'a> for &'a Stream {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<&'a Stream, PdfExtractError> {
      Ok(maybe_deref(doc, obj)?.as_stream()?)
   }
}

impl<'a> FromObj<'a> for &'a Object {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Result<&'a Object, PdfExtractError> {
      maybe_deref(doc, obj)
   }
}

pub(crate) fn get<'a, T: FromOptObj<'a>>(
   doc: &'a Document,
   dict: &'a Dictionary,
   key: &[u8],
) -> Result<T, PdfExtractError> {
   T::from_opt_obj(doc, dict.get(key).ok(), key)
}

pub(crate) fn maybe_get<'a, T: FromObj<'a>>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<T> {
   maybe_get_obj(doc, dict, key).and_then(|o| T::from_obj(doc, o).ok())
}

pub(crate) fn get_name_string<'a>(
   doc: &'a Document,
   dict: &'a Dictionary,
   key: &[u8],
) -> Result<String, PdfExtractError> {
   let obj = dict.get(key).map_err(|_| missing(key))?;
   Ok(pdf_to_utf8(maybe_deref(doc, obj)?.as_name()?))
}

#[allow(dead_code)]
pub(crate) fn maybe_get_name_string<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<String> {
   maybe_get_obj(doc, dict, key).and_then(|n| n.as_name().ok()).map(pdf_to_utf8)
}

pub(crate) fn maybe_get_name<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
   maybe_get_obj(doc, dict, key).and_then(|n| n.as_name().ok())
}

pub(crate) fn maybe_get_array<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Vec<Object>> {
   maybe_get_obj(doc, dict, key).and_then(|n| n.as_array().ok())
}

pub(crate) fn as_num(o: &Object) -> Result<f64, PdfExtractError> {
   match *o {
      Object::Integer(i) => Ok(i as f64),
      Object::Real(f) => Ok(f.into()),
      _ => Err(malformed(format!("expected a number, found {}", kind(o)))),
   }
}

// XXX: We'd ideally implement this without having to copy the uncompressed data
pub(crate) fn get_contents(contents: &Stream) -> Vec<u8> {
   if contents.filters().is_ok() {
      contents.decompressed_content().unwrap_or_else(|_| contents.content.clone())
   } else {
      contents.content.clone()
   }
}

/// A `/Parent` chain is a handful of levels deep in a well-formed file, but a
/// malformed one can make it circular.
const MAX_PAGE_TREE_DEPTH: u32 = 64;

pub(crate) fn get_inherited<'a, T: FromObj<'a>>(
   doc: &'a Document,
   dict: &'a Dictionary,
   key: &[u8],
   depth: u32,
) -> Option<T> {
   if let Ok(Some(o)) = get::<Option<T>>(doc, dict, key) {
      return Some(o);
   }
   if depth >= MAX_PAGE_TREE_DEPTH {
      warn!(
         "giving up on inherited {:?} after {} levels of /Parent",
         String::from_utf8_lossy(key),
         MAX_PAGE_TREE_DEPTH
      );
      return None;
   }
   let parent = dict
      .get(b"Parent")
      .and_then(|parent| parent.as_reference())
      .and_then(|id| doc.get_dictionary(id))
      .ok()?;
   get_inherited(doc, parent, key, depth + 1)
}
