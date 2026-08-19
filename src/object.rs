//! Reading values out of the PDF object graph.
//!
//! PDF lets almost any value be an indirect reference (32000-1 7.3.10), so every
//! path that pulls a value out of a dictionary has to be prepared to follow one.
//! `maybe_deref` does that, and the `FromObj` / `FromOptObj` pair chains it with
//! a conversion to a Rust type.

use crate::strings::pdf_to_utf8;
use log::warn;
use lopdf::{Dictionary, Document, Object, Stream};

pub(crate) fn maybe_deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
   match o {
      &Object::Reference(r) => doc.get_object(r).expect("missing object reference"),
      _ => o,
   }
}

pub(crate) fn maybe_get_obj<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
   dict.get(key).map(|o| maybe_deref(doc, o)).ok()
}

// an intermediate trait that can be used to chain conversions that may have failed
pub(crate) trait FromOptObj<'a> {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, key: &[u8]) -> Self;
}

// conditionally convert to Self returns None if the conversion failed
pub(crate) trait FromObj<'a>
where
   Self: std::marker::Sized,
{
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<Self>;
}

impl<'a, T: FromObj<'a>> FromOptObj<'a> for Option<T> {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, _key: &[u8]) -> Self {
      obj.and_then(|x| T::from_obj(doc, x))
   }
}

impl<'a, T: FromObj<'a>> FromOptObj<'a> for T {
   fn from_opt_obj(doc: &'a Document, obj: Option<&'a Object>, key: &[u8]) -> Self {
      T::from_obj(doc, obj.unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(key)))).expect("wrong type")
   }
}

// we follow the same conventions as pdfium for when to support indirect objects:
// on arrays, streams and dicts
impl<'a, T: FromObj<'a>> FromObj<'a> for Vec<T> {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<Self> {
      maybe_deref(doc, obj)
         .as_array()
         .map(|x| x.iter().map(|x| T::from_obj(doc, x).expect("wrong type")).collect())
         .ok()
   }
}

// XXX: These will panic if we don't have the right number of items
// we don't want to do that
impl<'a, T: FromObj<'a>> FromObj<'a> for [T; 4] {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<Self> {
      maybe_deref(doc, obj)
         .as_array()
         .map(|x| {
            let mut all = x.iter().map(|x| T::from_obj(doc, x).expect("wrong type"));
            [
               all.next().unwrap(),
               all.next().unwrap(),
               all.next().unwrap(),
               all.next().unwrap(),
            ]
         })
         .ok()
   }
}

impl<'a, T: FromObj<'a>> FromObj<'a> for [T; 3] {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<Self> {
      maybe_deref(doc, obj)
         .as_array()
         .map(|x| {
            let mut all = x.iter().map(|x| T::from_obj(doc, x).expect("wrong type"));
            [all.next().unwrap(), all.next().unwrap(), all.next().unwrap()]
         })
         .ok()
   }
}

impl<'a> FromObj<'a> for f64 {
   fn from_obj(_doc: &Document, obj: &Object) -> Option<Self> {
      match *obj {
         Object::Integer(i) => Some(i as f64),
         Object::Real(f) => Some(f.into()),
         _ => None,
      }
   }
}

impl<'a> FromObj<'a> for i64 {
   fn from_obj(_doc: &Document, obj: &Object) -> Option<Self> {
      match obj {
         &Object::Integer(i) => Some(i),
         _ => None,
      }
   }
}

impl<'a> FromObj<'a> for &'a Dictionary {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
      maybe_deref(doc, obj).as_dict().ok()
   }
}

impl<'a> FromObj<'a> for &'a Stream {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<&'a Stream> {
      maybe_deref(doc, obj).as_stream().ok()
   }
}

impl<'a> FromObj<'a> for &'a Object {
   fn from_obj(doc: &'a Document, obj: &'a Object) -> Option<&'a Object> {
      Some(maybe_deref(doc, obj))
   }
}

pub(crate) fn get<'a, T: FromOptObj<'a>>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> T {
   T::from_opt_obj(doc, dict.get(key).ok(), key)
}

pub(crate) fn maybe_get<'a, T: FromObj<'a>>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<T> {
   maybe_get_obj(doc, dict, key).and_then(|o| T::from_obj(doc, o))
}

pub(crate) fn get_name_string<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> String {
   pdf_to_utf8(
      dict
         .get(key)
         .map(|o| maybe_deref(doc, o))
         .unwrap_or_else(|_| panic!("deref"))
         .as_name()
         .expect("name"),
   )
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

pub(crate) fn as_num(o: &Object) -> f64 {
   match *o {
      Object::Integer(i) => i as f64,
      Object::Real(f) => f.into(),
      _ => {
         panic!("not a number")
      }
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

pub(crate) fn get_inherited<'a, T: FromObj<'a>>(doc: &'a Document, dict: &'a Dictionary, key: &[u8], depth: u32) -> Option<T> {
   let o: Option<T> = get(doc, dict, key);
   if let Some(o) = o {
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
