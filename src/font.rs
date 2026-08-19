//! Font dictionaries: turning character codes into Unicode and glyph widths.
//!
//! The three shapes the spec defines each get their own type: simple fonts
//! (32000-1 9.6), Type 3 fonts (9.6.5) and composite Type 0 / CID fonts (9.7).
//! `PdfFont` is what the content-stream processor sees of all three.

use crate::object::{
   as_num, get, get_contents, get_name_string, maybe_deref, maybe_get, maybe_get_array, maybe_get_name,
   maybe_get_name_string, maybe_get_obj,
};
use crate::strings::{PDF_DOC_ENCODING, pdf_to_utf8, to_utf8};
use crate::{core_fonts, encodings, glyphnames, zapfglyphnames};
use adobe_cmap_parser::{ByteMapping, CIDRange, CodeRange};
use log::{debug, warn};
use lopdf::{Dictionary, Document, Object};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::fmt::Debug;
use std::rc::Rc;
use std::slice::Iter;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone)]
struct PdfSimpleFont<'a> {
   font: &'a Dictionary,
   doc: &'a Document,
   encoding: Option<Vec<u16>>,
   unicode_map: Option<HashMap<u32, String>>,
   widths: HashMap<CharCode, f64>, // should probably just use i32 here
   missing_width: f64,
}

#[derive(Clone)]
struct PdfType3Font<'a> {
   font: &'a Dictionary,
   _doc: &'a Document,
   encoding: Option<Vec<u16>>,
   unicode_map: Option<HashMap<CharCode, String>>,
   widths: HashMap<CharCode, f64>, // should probably just use i32 here
}

pub(crate) fn make_font<'a>(doc: &'a Document, font: &'a Dictionary) -> Rc<dyn PdfFont + 'a> {
   let subtype = get_name_string(doc, font, b"Subtype");
   if subtype == "Type0" {
      Rc::new(PdfCIDFont::new(doc, font))
   } else if subtype == "Type3" {
      Rc::new(PdfType3Font::new(doc, font))
   } else {
      Rc::new(PdfSimpleFont::new(doc, font))
   }
}

fn is_core_font(name: &str) -> bool {
   matches!(
      name,
      "Courier-Bold"
         | "Courier-BoldOblique"
         | "Courier-Oblique"
         | "Courier"
         | "Helvetica-Bold"
         | "Helvetica-BoldOblique"
         | "Helvetica-Oblique"
         | "Helvetica"
         | "Symbol"
         | "Times-Bold"
         | "Times-BoldItalic"
         | "Times-Italic"
         | "Times-Roman"
         | "ZapfDingbats"
   )
}

fn encoding_to_unicode_table(name: &[u8]) -> Vec<u16> {
   let encoding = match name {
      b"MacRomanEncoding" => encodings::MAC_ROMAN_ENCODING,
      b"MacExpertEncoding" => encodings::MAC_EXPERT_ENCODING,
      b"WinAnsiEncoding" => encodings::WIN_ANSI_ENCODING,
      _ => panic!("unexpected encoding {:?}", pdf_to_utf8(name)),
   };

   encoding
      .iter()
      .map(|x| {
         if let &Some(x) = x {
            glyphnames::name_to_unicode(x).unwrap()
         } else {
            0
         }
      })
      .collect()
}

/* "Glyphs in the font are selected by single-byte character codes obtained from a string that
    is shown by the text-showing operators. Logically, these codes index into a table of 256
    glyphs; the mapping from codes to glyphs is called the font’s encoding. Each font program
    has a built-in encoding. Under some circumstances, the encoding can be altered by means
    described in Section 5.5.5, “Character Encoding.”
*/
impl<'a> PdfSimpleFont<'a> {
   fn new(doc: &'a Document, font: &'a Dictionary) -> PdfSimpleFont<'a> {
      let base_name = get_name_string(doc, font, b"BaseFont");
      let subtype = get_name_string(doc, font, b"Subtype");

      let encoding: Option<&Object> = get(doc, font, b"Encoding");
      let descriptor: Option<&Dictionary> = get(doc, font, b"FontDescriptor");
      let mut type1_encoding = None;
      let mut unicode_map = None;
      if let Some(descriptor) = descriptor {
         if subtype == "Type1" {
            let file = maybe_get_obj(doc, descriptor, b"FontFile");
            if let Some(Object::Stream(s)) = file {
               let s = get_contents(s);
               type1_encoding = Some(type1_encoding_parser::get_encoding_map(&s).expect("encoding"));
            }
         } else if subtype == "TrueType" {
            let file = maybe_get_obj(doc, descriptor, b"FontFile2");
            if let Some(Object::Stream(s)) = file {
               let _s = get_contents(s);
               //File::create(format!("/tmp/{}", base_name)).unwrap().write_all(&s);
            }
         }

         let font_file3 = get::<Option<&Object>>(doc, descriptor, b"FontFile3");
         if let Some(Object::Stream(s)) = font_file3 {
            let subtype = get_name_string(doc, &s.dict, b"Subtype");
            let s = get_contents(s);
            if subtype == "Type1C" {
               let table = cff_parser::Table::parse(&s).unwrap();
               //use std::io::Write;
               //File::create(format!("/tmp/{}", base_name)).unwrap().write_all(&s);

               let encoding = table.encoding.get_code_to_sid_table(&table.charset);

               let mapping: HashMap<u32, String> = encoding
                  .into_iter()
                  .filter_map(|(cid, sid)| {
                     let name = cff_parser::string_by_id(&table, sid).unwrap();
                     if name == ".notdef" {
                        return None;
                     }
                     let unicode = glyphnames::name_to_unicode(name)
                        .or_else(|| zapfglyphnames::zapfdigbats_names_to_unicode(name));
                     if unicode.is_none() {
                        warn!("Couldn't find unicode for {}", name);
                        return None;
                     }
                     let str = String::from_utf16(&[unicode.unwrap()]).unwrap();
                     Some((cid as u32, str))
                  })
                  .collect();
               unicode_map = Some(mapping);
            }

            //
            //File::create(format!("/tmp/{}", base_name)).unwrap().write_all(&s);
         }

         let charset = maybe_get_obj(doc, descriptor, b"CharSet");
         let _charset = match charset {
            Some(Object::String(s, _)) => Some(pdf_to_utf8(s)),
            _ => None,
         };
      }

      let mut unicode_map = match unicode_map {
         Some(mut unicode_map) => {
            unicode_map.extend(get_unicode_map(doc, font).unwrap_or_default());
            Some(unicode_map)
         }
         None => get_unicode_map(doc, font),
      };

      let mut encoding_table = None;
      match encoding {
         Some(Object::Name(encoding_name)) => {
            encoding_table = Some(encoding_to_unicode_table(encoding_name));
         }
         Some(Object::Dictionary(encoding)) => {
            let mut table = if let Some(base_encoding) = maybe_get_name(doc, encoding, b"BaseEncoding") {
               encoding_to_unicode_table(base_encoding)
            } else {
               Vec::from(PDF_DOC_ENCODING)
            };
            let differences = maybe_get_array(doc, encoding, b"Differences");
            if let Some(differences) = differences {
               let mut code = 0;
               for o in differences {
                  let o = maybe_deref(doc, o);
                  match *o {
                     Object::Integer(i) => {
                        code = i;
                     }
                     Object::Name(ref n) => {
                        let name = pdf_to_utf8(n);
                        // XXX: names of Type1 fonts can map to arbitrary strings instead of real
                        // unicode names, so we should probably handle this differently
                        let unicode = glyphnames::name_to_unicode(&name);
                        if let Some(unicode) = unicode {
                           table[code as usize] = unicode;
                           if let Some(ref mut unicode_map) = unicode_map {
                              let be = [unicode];
                              match unicode_map.entry(code as u32) {
                                 // If there's a unicode table entry missing use one based on the name
                                 Entry::Vacant(v) => {
                                    v.insert(String::from_utf16(&be).unwrap());
                                 }
                                 Entry::Occupied(e) => {
                                    if e.get() != &String::from_utf16(&be).unwrap() {
                                       let normal_match = e.get().nfkc().eq(String::from_utf16(&be).unwrap().nfkc());
                                       if !normal_match {
                                          warn!(
                                             "Unicode mismatch {} {} {:?} {:?} {:?}",
                                             normal_match,
                                             name,
                                             e.get(),
                                             String::from_utf16(&be),
                                             be
                                          );
                                       }
                                    }
                                 }
                              }
                           }
                        } else {
                           match unicode_map {
                              Some(ref mut unicode_map) if base_name.contains("FontAwesome") => {
                                 // the fontawesome tex package will use glyph names that don't have a corresponding unicode
                                 // code point, so we'll use an empty string instead. See issue #76
                                 match unicode_map.entry(code as u32) {
                                    Entry::Vacant(v) => {
                                       v.insert("".to_owned());
                                    }
                                    Entry::Occupied(_e) => {
                                       panic!("unexpected entry in unicode map")
                                    }
                                 }
                              }
                              _ => {
                                 warn!("unknown glyph name '{}' for font {}", name, base_name);
                              }
                           }
                        }
                        code += 1;
                     }
                     _ => {
                        panic!("wrong type {:?}", o);
                     }
                  }
               }
            }
            encoding_table = Some(table);
         }
         None => {
            if let Some(type1_encoding) = type1_encoding {
               let mut table = Vec::from(PDF_DOC_ENCODING);
               for (code, name) in type1_encoding {
                  let unicode = glyphnames::name_to_unicode(&pdf_to_utf8(&name));
                  if let Some(unicode) = unicode {
                     table[code as usize] = unicode;
                  }
               }
               encoding_table = Some(table)
            } else if subtype == "TrueType" {
               encoding_table = Some(
                  encodings::WIN_ANSI_ENCODING
                     .iter()
                     .map(|x| {
                        if let &Some(x) = x {
                           glyphnames::name_to_unicode(x).unwrap()
                        } else {
                           0
                        }
                     })
                     .collect(),
               );
            }
         }
         _ => {
            panic!()
         }
      }

      let mut width_map = HashMap::new();
      /* "Ordinarily, a font dictionary that refers to one of the standard fonts
            should omit the FirstChar, LastChar, Widths, and FontDescriptor entries.
            However, it is permissible to override a standard font by including these
            entries and embedding the font program in the PDF file."

      Note: some PDFs include a descriptor but still don't include these entries */

      // If we have widths prefer them over the core font widths. Needed for https://dkp.de/wp-content/uploads/parteitage/Sozialismusvorstellungen-der-DKP.pdf
      if let (Some(first_char), Some(last_char), Some(widths)) = (
         maybe_get::<i64>(doc, font, b"FirstChar"),
         maybe_get::<i64>(doc, font, b"LastChar"),
         maybe_get::<Vec<f64>>(doc, font, b"Widths"),
      ) {
         // Some PDF's don't have these like fips-197.pdf
         let mut i: i64 = 0;

         for w in widths {
            width_map.insert((first_char + i) as CharCode, w);
            i += 1;
         }
         assert_eq!(first_char + i - 1, last_char);
      } else {
         // FIXME: `name` is computed to pick a substitute font, but the lookup below
         // matches on `base_name`, so the Helvetica fallback never takes effect.
         let name = if is_core_font(&base_name) {
            &base_name
         } else {
            warn!("no widths and not core font {:?}", base_name);

            // This situation is handled differently by different readers
            // but basically we try to substitute the best font that we can.

            // Poppler/Xpdf:
            // this is technically an error -- the Widths entry is required
            // for all but the Base-14 fonts -- but certain PDF generators
            // apparently don't include widths for Arial and TimesNewRoman

            // Pdfium: CFX_FontMapper::FindSubstFont

            // mupdf: pdf_load_substitute_font

            // We can try to do a better job guessing at a font by looking at the flags
            // or the basename but for now we'll just use Helvetica
            "Helvetica"
         };
         for font_metrics in core_fonts::metrics().iter() {
            if font_metrics.0 == base_name {
               if let Some(ref encoding) = encoding_table {
                  for w in font_metrics.2 {
                     let c = glyphnames::name_to_unicode(w.2).unwrap();
                     for (i, &e) in encoding.iter().enumerate() {
                        if e == c {
                           width_map.insert(i as CharCode, w.1);
                        }
                     }
                  }
               } else {
                  // Instead of using the encoding from the core font we'll just look up all
                  // of the character names. We should probably verify that this produces the
                  // same result.

                  let mut table = vec![0; 256];
                  for w in font_metrics.2 {
                     // -1 is "not encoded"
                     if w.0 != -1 {
                        table[w.0 as usize] = if base_name == "ZapfDingbats" {
                           zapfglyphnames::zapfdigbats_names_to_unicode(w.2)
                              .unwrap_or_else(|| panic!("bad name {:?}", w))
                        } else {
                           glyphnames::name_to_unicode(w.2).unwrap()
                        }
                     }
                  }

                  let encoding = &table[..];
                  for w in font_metrics.2 {
                     width_map.insert(w.0 as CharCode, w.1);
                     // -1 is "not encoded"
                  }
                  encoding_table = Some(encoding.to_vec());
               }
               /* "Ordinarily, a font dictionary that refers to one of the standard fonts
                        should omit the FirstChar, LastChar, Widths, and FontDescriptor entries.
                        However, it is permissible to override a standard font by including these
                        entries and embedding the font program in the PDF file."

               Note: some PDFs include a descriptor but still don't include these entries */
               // assert!(maybe_get_obj(doc, font, b"FirstChar").is_none());
               // assert!(maybe_get_obj(doc, font, b"LastChar").is_none());
               // assert!(maybe_get_obj(doc, font, b"Widths").is_none());
            }
         }
      }

      let missing_width = get::<Option<f64>>(doc, font, b"MissingWidth").unwrap_or(0.);
      PdfSimpleFont {
         doc,
         font,
         widths: width_map,
         encoding: encoding_table,
         missing_width,
         unicode_map,
      }
   }

   #[allow(dead_code)]
   fn get_type(&self) -> String {
      get_name_string(self.doc, self.font, b"Type")
   }
   #[allow(dead_code)]
   fn get_basefont(&self) -> String {
      get_name_string(self.doc, self.font, b"BaseFont")
   }
   #[allow(dead_code)]
   fn get_subtype(&self) -> String {
      get_name_string(self.doc, self.font, b"Subtype")
   }
   #[allow(dead_code)]
   fn get_widths(&self) -> Option<&Vec<Object>> {
      maybe_get_obj(self.doc, self.font, b"Widths").map(|widths| widths.as_array().expect("Widths should be an array"))
   }
   /* For type1: This entry is obsolescent and its use is no longer recommended. (See
    * implementation note 42 in Appendix H.) */
   #[allow(dead_code)]
   fn get_name(&self) -> Option<String> {
      maybe_get_name_string(self.doc, self.font, b"Name")
   }

   #[allow(dead_code)]
   fn get_descriptor(&self) -> Option<PdfFontDescriptor<'_>> {
      maybe_get_obj(self.doc, self.font, b"FontDescriptor")
         .and_then(|desc| desc.as_dict().ok())
         .map(|desc| PdfFontDescriptor { desc, doc: self.doc })
   }
}

impl<'a> PdfType3Font<'a> {
   fn new(doc: &'a Document, font: &'a Dictionary) -> PdfType3Font<'a> {
      let unicode_map = get_unicode_map(doc, font);
      let encoding: Option<&Object> = get(doc, font, b"Encoding");

      let encoding_table;
      match encoding {
         Some(Object::Name(encoding_name)) => {
            encoding_table = Some(encoding_to_unicode_table(encoding_name));
         }
         Some(Object::Dictionary(encoding)) => {
            let mut table = if let Some(base_encoding) = maybe_get_name(doc, encoding, b"BaseEncoding") {
               encoding_to_unicode_table(base_encoding)
            } else {
               Vec::from(PDF_DOC_ENCODING)
            };
            let differences = maybe_get_array(doc, encoding, b"Differences");
            if let Some(differences) = differences {
               let mut code = 0;
               for o in differences {
                  match o {
                     &Object::Integer(i) => {
                        code = i;
                     }
                     Object::Name(n) => {
                        let name = pdf_to_utf8(n);
                        // XXX: names of Type1 fonts can map to arbitrary strings instead of real
                        // unicode names, so we should probably handle this differently
                        let unicode = glyphnames::name_to_unicode(&name);
                        if let Some(unicode) = unicode {
                           table[code as usize] = unicode;
                        }
                        code += 1;
                     }
                     _ => {
                        panic!("wrong type");
                     }
                  }
               }
            }
            encoding_table = Some(table);
         }
         _ => {
            panic!()
         }
      }

      let first_char: i64 = get(doc, font, b"FirstChar");
      let last_char: i64 = get(doc, font, b"LastChar");
      let widths: Vec<f64> = get(doc, font, b"Widths");

      let mut width_map = HashMap::new();

      let mut i = 0;

      for w in widths {
         width_map.insert((first_char + i) as CharCode, w);
         i += 1;
      }
      assert_eq!(first_char + i - 1, last_char);
      PdfType3Font {
         _doc: doc,
         font,
         widths: width_map,
         encoding: encoding_table,
         unicode_map,
      }
   }
}

pub(crate) type CharCode = u32;

pub(crate) struct PdfFontIter<'a> {
   i: Iter<'a, u8>,
   font: &'a dyn PdfFont,
}

impl<'a> Iterator for PdfFontIter<'a> {
   type Item = (CharCode, u8);
   fn next(&mut self) -> Option<(CharCode, u8)> {
      self.font.next_char(&mut self.i)
   }
}

pub(crate) trait PdfFont: Debug {
   fn get_width(&self, id: CharCode) -> f64;
   fn next_char(&self, iter: &mut Iter<u8>) -> Option<(CharCode, u8)>;
   fn decode_char(&self, char: CharCode) -> String;

   /*fn char_codes<'a>(&'a self, chars: &'a [u8]) -> PdfFontIter {
       let p = self;
       PdfFontIter{i: chars.iter(), font: p as &PdfFont}
   }*/
}

impl<'a> dyn PdfFont + 'a {
   pub(crate) fn char_codes(&'a self, chars: &'a [u8]) -> PdfFontIter<'a> {
      PdfFontIter {
         i: chars.iter(),
         font: self,
      }
   }
}

impl<'a> PdfFont for PdfSimpleFont<'a> {
   fn get_width(&self, id: CharCode) -> f64 {
      let width = self.widths.get(&id);
      if let Some(width) = width {
         *width
      } else {
         let mut widths = self.widths.iter().collect::<Vec<_>>();
         widths.sort_by_key(|x| x.0);
         self.missing_width
      }
   }
   /*fn decode(&self, chars: &[u8]) -> String {
       let encoding = self.encoding.as_ref().map(|x| &x[..]).unwrap_or(&PDFDocEncoding);
       to_utf8(encoding, chars)
   }*/

   fn next_char(&self, iter: &mut Iter<u8>) -> Option<(CharCode, u8)> {
      iter.next().map(|x| (*x as CharCode, 1))
   }
   fn decode_char(&self, char: CharCode) -> String {
      let slice = [char as u8];
      if let Some(ref unicode_map) = self.unicode_map {
         let s = unicode_map.get(&char);
         let s = match s {
            Some(s) => s.clone(),
            None => {
               debug!("missing char {:?} in unicode map {:?} for {:?}", char, unicode_map, self.font);
               // some pdf's like http://arxiv.org/pdf/2312.00064v1 are missing entries in their unicode map but do have
               // entries in the encoding.
               let encoding = self.encoding.as_ref().map(|x| &x[..]).expect("missing unicode map and encoding");
               let s = to_utf8(encoding, &slice);
               debug!("falling back to encoding {} -> {:?}", char, s);
               s
            }
         };
         return s;
      }
      let encoding = self.encoding.as_ref().map(|x| &x[..]).unwrap_or(PDF_DOC_ENCODING);

      to_utf8(encoding, &slice)
   }
}

impl<'a> fmt::Debug for PdfSimpleFont<'a> {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      self.font.fmt(f)
   }
}

impl<'a> PdfFont for PdfType3Font<'a> {
   fn get_width(&self, id: CharCode) -> f64 {
      let width = self.widths.get(&id);
      if let Some(width) = width {
         *width
      } else {
         panic!("missing width for {} {:?}", id, self.font);
      }
   }
   /*fn decode(&self, chars: &[u8]) -> String {
       let encoding = self.encoding.as_ref().map(|x| &x[..]).unwrap_or(&PDFDocEncoding);
       to_utf8(encoding, chars)
   }*/

   fn next_char(&self, iter: &mut Iter<u8>) -> Option<(CharCode, u8)> {
      iter.next().map(|x| (*x as CharCode, 1))
   }
   fn decode_char(&self, char: CharCode) -> String {
      let slice = [char as u8];
      if let Some(ref unicode_map) = self.unicode_map {
         let s = unicode_map.get(&char);
         let s = match s {
            None => {
               debug!("missing char {:?} in unicode map {:?} for {:?}", char, unicode_map, self.font);
               // some pdf's like http://arxiv.org/pdf/2312.00577v1 are missing entries in their unicode map but do have
               // entries in the encoding.
               let encoding = self.encoding.as_ref().map(|x| &x[..]).expect("missing unicode map and encoding");
               let s = to_utf8(encoding, &slice);
               debug!("falling back to encoding {} -> {:?}", char, s);
               s
            }
            Some(s) => s.clone(),
         };
         return s;
      }
      let encoding = self.encoding.as_ref().map(|x| &x[..]).unwrap_or(PDF_DOC_ENCODING);

      to_utf8(encoding, &slice)
   }
}

impl<'a> fmt::Debug for PdfType3Font<'a> {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      self.font.fmt(f)
   }
}

struct PdfCIDFont<'a> {
   font: &'a Dictionary,
   #[allow(dead_code)]
   doc: &'a Document,
   #[allow(dead_code)]
   encoding: ByteMapping,
   to_unicode: Option<HashMap<u32, String>>,
   widths: HashMap<CharCode, f64>, // should probably just use i32 here
   default_width: Option<f64>,     // only used for CID fonts and we should probably brake out the different font types
}

fn get_unicode_map<'a>(doc: &'a Document, font: &'a Dictionary) -> Option<HashMap<u32, String>> {
   let to_unicode = maybe_get_obj(doc, font, b"ToUnicode");
   let mut unicode_map = None;
   match to_unicode {
      Some(Object::Stream(stream)) => {
         let contents = get_contents(stream);

         let cmap = adobe_cmap_parser::get_unicode_map(&contents).unwrap();
         let mut unicode = HashMap::new();
         // "It must use the beginbfchar, endbfchar, beginbfrange, and endbfrange operators to
         // define the mapping from character codes to Unicode character sequences expressed in
         // UTF-16BE encoding."
         for (&k, v) in cmap.iter() {
            let mut be: Vec<u16> = Vec::new();
            let mut i = 0;
            assert!(v.len() % 2 == 0);
            while i < v.len() {
               be.push(((v[i] as u16) << 8) | v[i + 1] as u16);
               i += 2;
            }
            if let [0xd800..=0xdfff] = &be[..] {
               // this range is not specified as not being encoded
               // we ignore them so we don't an error from from_utt16
               continue;
            }
            let s = String::from_utf16(&be).unwrap();

            unicode.insert(k, s);
         }
         unicode_map = Some(unicode);
      }
      None => {}
      Some(Object::Name(name)) => {
         let name = pdf_to_utf8(name);
         if name != "Identity-H" {
            todo!("unsupported ToUnicode name: {:?}", name);
         }
      }
      _ => {
         panic!("unsupported cmap {:?}", to_unicode)
      }
   }
   unicode_map
}

impl<'a> PdfCIDFont<'a> {
   fn new(doc: &'a Document, font: &'a Dictionary) -> PdfCIDFont<'a> {
      let descendants = maybe_get_array(doc, font, b"DescendantFonts").expect("Descendant fonts required");
      let ciddict = maybe_deref(doc, &descendants[0]).as_dict().expect("should be CID dict");
      let encoding = maybe_get_obj(doc, font, b"Encoding").expect("Encoding required in type0 fonts");

      let encoding = match encoding {
         Object::Name(name) => {
            let name = pdf_to_utf8(name);
            if name == "Identity-H" || name == "Identity-V" {
               ByteMapping {
                  codespace: vec![CodeRange {
                     width: 2,
                     start: 0,
                     end: 0xffff,
                  }],
                  cid: vec![CIDRange {
                     src_code_lo: 0,
                     src_code_hi: 0xffff,
                     dst_cid_lo: 0,
                  }],
               }
            } else {
               panic!("unsupported encoding {}", name);
            }
         }
         Object::Stream(stream) => {
            let contents = get_contents(stream);
            adobe_cmap_parser::get_byte_mapping(&contents).unwrap()
         }
         _ => {
            panic!("unsupported encoding {:?}", encoding)
         }
      };

      // Sometimes a Type0 font might refer to the same underlying data as regular font. In this case we may be able to extract some encoding
      // data.
      // We should also look inside the truetype data to see if there's a cmap table. It will help us convert as well.
      // This won't work if the cmap has been subsetted. A better approach might be to hash glyph contents and use that against
      // a global library of glyph hashes
      let unicode_map = get_unicode_map(doc, font);

      let font_dict = maybe_get_obj(doc, ciddict, b"FontDescriptor").expect("required");
      let _f = font_dict.as_dict().expect("must be dict");
      let default_width = get::<Option<i64>>(doc, ciddict, b"DW").unwrap_or(1000);
      let w: Option<Vec<&Object>> = get(doc, ciddict, b"W");
      let mut widths = HashMap::new();
      let mut i = 0;
      if let Some(w) = w {
         while i < w.len() {
            if let Object::Array(wa) = w[i + 1] {
               let cid = w[i].as_i64().expect("id should be num");
               for (j, w) in wa.iter().enumerate() {
                  widths.insert((cid + j as i64) as CharCode, as_num(w));
               }
               i += 2;
            } else {
               let c_first = w[i].as_i64().expect("first should be num");
               let c_last = w[i].as_i64().expect("last should be num");
               let c_width = as_num(w[i]);
               for id in c_first..c_last {
                  widths.insert(id as CharCode, c_width);
               }
               i += 3;
            }
         }
      }
      PdfCIDFont {
         doc,
         font,
         widths,
         to_unicode: unicode_map,
         encoding,
         default_width: Some(default_width as f64),
      }
   }
}

impl<'a> PdfFont for PdfCIDFont<'a> {
   fn get_width(&self, id: CharCode) -> f64 {
      let width = self.widths.get(&id);
      if let Some(width) = width {
         *width
      } else {
         self.default_width.unwrap()
      }
   } /*
   pub(crate) fn decode(&self, chars: &[u8]) -> String {
   self.char_codes(chars);

   //let utf16 = Vec::new();

   let encoding = self.encoding.as_ref().map(|x| &x[..]).unwrap_or(&PDFDocEncoding);
   to_utf8(encoding, chars)
   }*/

   fn next_char(&self, iter: &mut Iter<u8>) -> Option<(CharCode, u8)> {
      let mut c = *iter.next()? as u32;
      let mut code = None;
      'outer: for width in 1..=4 {
         for range in &self.encoding.codespace {
            if c >= range.start && c <= range.end && range.width == width {
               code = Some((c, width));
               break 'outer;
            }
         }
         let next = *iter.next()?;
         c = (c << 8) | next as u32;
      }
      let code = code?;
      for range in &self.encoding.cid {
         if code.0 >= range.src_code_lo && code.0 <= range.src_code_hi {
            return Some((code.0 + range.dst_cid_lo, code.1 as u8));
         }
      }
      None
   }
   fn decode_char(&self, char: CharCode) -> String {
      let s = self.to_unicode.as_ref().and_then(|x| x.get(&char));
      if let Some(s) = s { s.clone() } else { "".to_string() }
   }
}

impl<'a> fmt::Debug for PdfCIDFont<'a> {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      self.font.fmt(f)
   }
}

#[derive(Copy, Clone)]
struct PdfFontDescriptor<'a> {
   desc: &'a Dictionary,
   doc: &'a Document,
}

impl<'a> PdfFontDescriptor<'a> {
   #[allow(dead_code)]
   fn get_file(&self) -> Option<&'a Object> {
      maybe_get_obj(self.doc, self.desc, b"FontFile")
   }
}

impl<'a> fmt::Debug for PdfFontDescriptor<'a> {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      self.desc.fmt(f)
   }
}
