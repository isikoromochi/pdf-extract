//! Executing a page's content stream.
//!
//! `Processor` walks the operators of a content stream (32000-1 8 and 9),
//! maintaining the graphics and text state they manipulate, and reports each
//! glyph it places to an `OutputDev`.

use crate::color::{ColorSpace, make_colorspace};
use crate::error::PdfExtractError;
use crate::font::{PdfFont, make_font};
use crate::geom::{MediaBox, Path, PathOp, Transform};
use crate::object::{as_num, get, get_contents, maybe_deref, maybe_get_obj};
use crate::output::OutputDev;
use euclid::Transform2D;
use log::warn;
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone)]
struct TextState<'a> {
   font: Option<Rc<dyn PdfFont + 'a>>,
   font_size: f64,
   character_spacing: f64,
   word_spacing: f64,
   horizontal_scaling: f64,
   leading: f64,
   rise: f64,
   tm: Transform,
}

#[derive(Clone)]
struct GraphicsState<'a> {
   ctm: Transform,
   ts: TextState<'a>,
   smask: Option<Dictionary>,
   fill_colorspace: ColorSpace,
   fill_color: Vec<f64>,
   stroke_colorspace: ColorSpace,
   stroke_color: Vec<f64>,
   line_width: f64,
}

fn show_text(
   gs: &mut GraphicsState,
   s: &[u8],
   _tlm: &Transform,
   _flip_ctm: &Transform,
   output: &mut dyn OutputDev,
) -> Result<(), PdfExtractError> {
   let ts = &mut gs.ts;
   let font = ts.font.as_ref().unwrap();
   //let encoding = font.encoding.as_ref().map(|x| &x[..]).unwrap_or(&PDFDocEncoding);
   output.begin_word()?;

   for (c, length) in font.char_codes(s) {
      // 5.3.3 Text Space Details
      let tsm = Transform2D::row_major(ts.horizontal_scaling, 0., 0., 1.0, 0., ts.rise);
      // Trm = Tsm × Tm × CTM
      let trm = tsm.post_transform(&ts.tm.post_transform(&gs.ctm));
      // 5.9 Extraction of Text Content

      let w0 = font.get_width(c) / 1000.;

      let mut spacing = ts.character_spacing;
      // "Word spacing is applied to every occurrence of the single-byte character code 32 in a
      //  string when using a simple font or a composite font that defines code 32 as a
      //  single-byte code. It does not apply to occurrences of the byte value 32 in
      //  multiple-byte codes."
      let is_space = c == 32 && length == 1;
      if is_space {
         spacing += ts.word_spacing
      }

      output.output_character(&trm, w0, spacing, ts.font_size, &font.decode_char(c))?;
      let tj = 0.;
      let ty = 0.;
      let tx = ts.horizontal_scaling * ((w0 - tj / 1000.) * ts.font_size + spacing);
      ts.tm = ts.tm.pre_transform(&Transform2D::create_translation(tx, ty));
      let _trm = ts.tm.pre_transform(&gs.ctm);
   }
   output.end_word()?;
   Ok(())
}

fn apply_state(doc: &Document, gs: &mut GraphicsState, state: &Dictionary) {
   for (k, v) in state.iter() {
      let k: &[u8] = k.as_ref();
      match k {
         b"SMask" => match maybe_deref(doc, v) {
            Object::Name(name) => {
               if name == b"None" {
                  gs.smask = None;
               } else {
                  panic!("unexpected smask name")
               }
            }
            Object::Dictionary(dict) => {
               gs.smask = Some(dict.clone());
            }
            _ => {
               panic!("unexpected smask type {:?}", v)
            }
         },
         b"Type" => match v {
            Object::Name(name) => {
               assert_eq!(name, b"ExtGState")
            }
            _ => {
               panic!("unexpected type")
            }
         },
         _ => {}
      }
   }
}

/// Form XObjects nest, and a malformed file can nest them without end -- a form
/// whose content stream invokes itself is enough. Bound the nesting at the depth
/// pdfium uses rather than letting it run the stack out.
const MAX_XOBJECT_DEPTH: u32 = 32;

pub(crate) struct Processor<'a> {
   font_table: HashMap<ObjectId, Rc<dyn PdfFont + 'a>>,
   _none: PhantomData<&'a ()>,
}

impl<'a> Processor<'a> {
   pub(crate) fn new() -> Processor<'a> {
      Processor {
         font_table: HashMap::new(),
         _none: PhantomData,
      }
   }

   pub(crate) fn process_stream(
      &mut self,
      doc: &'a Document,
      content: Vec<u8>,
      resources: &'a Dictionary,
      media_box: &MediaBox,
      output: &mut dyn OutputDev,
      depth: u32,
   ) -> Result<(), PdfExtractError> {
      let content = Content::decode(&content).unwrap();
      let mut gs: GraphicsState = GraphicsState {
         ts: TextState {
            font: None,
            font_size: f64::NAN,
            character_spacing: 0.,
            word_spacing: 0.,
            horizontal_scaling: 1.,
            leading: 0.,
            rise: 0.,
            tm: Transform2D::identity(),
         },
         fill_color: Vec::new(),
         fill_colorspace: ColorSpace::DeviceGray,
         stroke_color: Vec::new(),
         stroke_colorspace: ColorSpace::DeviceGray,
         line_width: 1.,
         ctm: Transform2D::identity(),
         smask: None,
      };
      //let mut ts = &mut gs.ts;
      let mut gs_stack = Vec::new();
      let mut mc_stack = Vec::new();
      // XXX: replace tlm with a point for text start
      let mut tlm = Transform2D::identity();
      let mut path = Path::new();
      let flip_ctm = Transform2D::row_major(1., 0., 0., -1., 0., media_box.ury - media_box.lly);
      for operation in &content.operations {
         match operation.operator.as_ref() {
            "BT" => {
               tlm = Transform2D::identity();
               gs.ts.tm = tlm;
            }
            "ET" => {
               tlm = Transform2D::identity();
               gs.ts.tm = tlm;
            }
            "cm" => {
               assert!(operation.operands.len() == 6);
               let m = Transform2D::row_major(
                  as_num(&operation.operands[0]),
                  as_num(&operation.operands[1]),
                  as_num(&operation.operands[2]),
                  as_num(&operation.operands[3]),
                  as_num(&operation.operands[4]),
                  as_num(&operation.operands[5]),
               );
               gs.ctm = gs.ctm.pre_transform(&m);
            }
            "CS" => {
               let name = operation.operands[0].as_name().unwrap();
               gs.stroke_colorspace = make_colorspace(doc, name, resources);
            }
            "cs" => {
               let name = operation.operands[0].as_name().unwrap();
               gs.fill_colorspace = make_colorspace(doc, name, resources);
            }
            "SC" | "SCN" => {
               gs.stroke_color = match gs.stroke_colorspace {
                  ColorSpace::Pattern => Vec::new(),
                  _ => operation.operands.iter().map(as_num).collect(),
               };
            }
            "sc" | "scn" => {
               gs.fill_color = match gs.fill_colorspace {
                  ColorSpace::Pattern => Vec::new(),
                  _ => operation.operands.iter().map(as_num).collect(),
               };
            }
            // color-setting shorthands: unhandled
            "G" | "g" | "RG" | "rg" | "K" | "k" => {}
            "TJ" => {
               if let Object::Array(ref array) = operation.operands[0] {
                  for e in array {
                     match e {
                        Object::String(s, _) => {
                           show_text(&mut gs, s, &tlm, &flip_ctm, output)?;
                        }
                        &Object::Integer(i) => {
                           let ts = &mut gs.ts;
                           let w0 = 0.;
                           let tj = i as f64;
                           let ty = 0.;
                           let tx = ts.horizontal_scaling * ((w0 - tj / 1000.) * ts.font_size);
                           ts.tm = ts.tm.pre_transform(&Transform2D::create_translation(tx, ty));
                        }
                        &Object::Real(i) => {
                           let ts = &mut gs.ts;
                           let w0 = 0.;
                           let tj = i as f64;
                           let ty = 0.;
                           let tx = ts.horizontal_scaling * ((w0 - tj / 1000.) * ts.font_size);
                           ts.tm = ts.tm.pre_transform(&Transform2D::create_translation(tx, ty));
                        }
                        _ => {}
                     }
                  }
               }
            }
            "Tj" => match operation.operands[0] {
               Object::String(ref s, _) => {
                  show_text(&mut gs, s, &tlm, &flip_ctm, output)?;
               }
               _ => {
                  panic!("unexpected Tj operand {:?}", operation)
               }
            },
            // `string '` is `T*` followed by `Tj`. `aw ac string "` is the same with
            // the word and character spacing set first; those two assignments persist
            // in the text state rather than applying to this line alone. 32000-1 9.4.3.
            "'" | "\"" => {
               let string = if operation.operator == "\"" {
                  gs.ts.word_spacing = as_num(&operation.operands[0]);
                  gs.ts.character_spacing = as_num(&operation.operands[1]);
                  &operation.operands[2]
               } else {
                  &operation.operands[0]
               };

               // T*: move to the start of the next line.
               tlm = tlm.pre_transform(&Transform2D::create_translation(0., -gs.ts.leading));
               gs.ts.tm = tlm;
               output.end_line()?;

               match string {
                  Object::String(s, _) => show_text(&mut gs, s, &tlm, &flip_ctm, output)?,
                  other => warn!("unexpected operand for {}: {:?}", operation.operator, other),
               }
            }
            "Tc" => {
               gs.ts.character_spacing = as_num(&operation.operands[0]);
            }
            "Tw" => {
               gs.ts.word_spacing = as_num(&operation.operands[0]);
            }
            "Tz" => {
               gs.ts.horizontal_scaling = as_num(&operation.operands[0]) / 100.;
            }
            "TL" => {
               gs.ts.leading = as_num(&operation.operands[0]);
            }
            "Tf" => {
               let fonts: &Dictionary = get(doc, resources, b"Font");
               let name = operation.operands[0].as_name().unwrap();
               // Resource names are scoped to the resource dictionary they appear
               // in (32000-1 7.8.3), so the cache has to be keyed on the identity of
               // the dictionary a name resolves to. Keyed on the name alone, page 2's
               // /F1 picks up whatever page 1 called /F1.
               let font = match fonts.get(name) {
                  Ok(&Object::Reference(id)) => self
                     .font_table
                     .entry(id)
                     .or_insert_with(|| make_font(doc, get::<&Dictionary>(doc, fonts, name)))
                     .clone(),
                  // A font written inline has no object id to key on, and is not
                  // reachable from any other resource dictionary, so skip the cache.
                  _ => make_font(doc, get::<&Dictionary>(doc, fonts, name)),
               };
               {
                  /*let file = font.get_descriptor().and_then(|desc| desc.get_file());
                  if let Some(file) = file {
                      let file_contents = filter_data(file.as_stream().unwrap());
                      let mut cursor = Cursor::new(&file_contents[..]);
                      //let f = Font::read(&mut cursor);
                  }*/
               }
               gs.ts.font = Some(font);

               gs.ts.font_size = as_num(&operation.operands[1]);
            }
            "Ts" => {
               gs.ts.rise = as_num(&operation.operands[0]);
            }
            "Tm" => {
               assert!(operation.operands.len() == 6);
               tlm = Transform2D::row_major(
                  as_num(&operation.operands[0]),
                  as_num(&operation.operands[1]),
                  as_num(&operation.operands[2]),
                  as_num(&operation.operands[3]),
                  as_num(&operation.operands[4]),
                  as_num(&operation.operands[5]),
               );
               gs.ts.tm = tlm;
               output.end_line()?;
            }
            "Td" => {
               /* Move to the start of the next line, offset from the start of the current line by (tx , ty ).
                 tx and ty are numbers expressed in unscaled text space units.
                 More precisely, this operator performs the following assignments:
               */
               assert!(operation.operands.len() == 2);
               let tx = as_num(&operation.operands[0]);
               let ty = as_num(&operation.operands[1]);

               tlm = tlm.pre_transform(&Transform2D::create_translation(tx, ty));
               gs.ts.tm = tlm;
               output.end_line()?;
            }

            "TD" => {
               /* Move to the start of the next line, offset from the start of the current line by (tx , ty ).
                 As a side effect, this operator sets the leading parameter in the text state.
               */
               assert!(operation.operands.len() == 2);
               let tx = as_num(&operation.operands[0]);
               let ty = as_num(&operation.operands[1]);
               gs.ts.leading = -ty;

               tlm = tlm.pre_transform(&Transform2D::create_translation(tx, ty));
               gs.ts.tm = tlm;
               output.end_line()?;
            }

            "T*" => {
               let tx = 0.0;
               let ty = -gs.ts.leading;

               tlm = tlm.pre_transform(&Transform2D::create_translation(tx, ty));
               gs.ts.tm = tlm;
               output.end_line()?;
            }
            "q" => {
               gs_stack.push(gs.clone());
            }
            "Q" => {
               let s = gs_stack.pop();
               if let Some(s) = s {
                  gs = s;
               } else {
                  warn!("No state to pop");
               }
            }
            "gs" => {
               let ext_gstate: &Dictionary = get(doc, resources, b"ExtGState");
               let name = operation.operands[0].as_name().unwrap();
               let state: &Dictionary = get(doc, ext_gstate, name);
               apply_state(doc, &mut gs, state);
            }
            // flatness tolerance: no bearing on text extraction
            "i" => {}
            "w" => {
               gs.line_width = as_num(&operation.operands[0]);
            }
            // line cap/join, miter limit, dash pattern, rendering intent: unhandled
            "J" | "j" | "M" | "d" | "ri" => {}
            "m" => path
               .ops
               .push(PathOp::MoveTo(as_num(&operation.operands[0]), as_num(&operation.operands[1]))),
            "l" => path
               .ops
               .push(PathOp::LineTo(as_num(&operation.operands[0]), as_num(&operation.operands[1]))),
            "c" => path.ops.push(PathOp::CurveTo(
               as_num(&operation.operands[0]),
               as_num(&operation.operands[1]),
               as_num(&operation.operands[2]),
               as_num(&operation.operands[3]),
               as_num(&operation.operands[4]),
               as_num(&operation.operands[5]),
            )),
            "v" => {
               let (x, y) = path.current_point();
               path.ops.push(PathOp::CurveTo(
                  x,
                  y,
                  as_num(&operation.operands[0]),
                  as_num(&operation.operands[1]),
                  as_num(&operation.operands[2]),
                  as_num(&operation.operands[3]),
               ))
            }
            "y" => path.ops.push(PathOp::CurveTo(
               as_num(&operation.operands[0]),
               as_num(&operation.operands[1]),
               as_num(&operation.operands[2]),
               as_num(&operation.operands[3]),
               as_num(&operation.operands[2]),
               as_num(&operation.operands[3]),
            )),
            "h" => path.ops.push(PathOp::Close),
            "re" => path.ops.push(PathOp::Rect(
               as_num(&operation.operands[0]),
               as_num(&operation.operands[1]),
               as_num(&operation.operands[2]),
               as_num(&operation.operands[3]),
            )),
            // path painting variants we don't emit: unhandled
            "s" | "f*" | "B" | "B*" | "b" => {}
            "S" => {
               output.stroke(&gs.ctm, &gs.stroke_colorspace, &gs.stroke_color, &path)?;
               path.ops.clear();
            }
            "F" | "f" => {
               output.fill(&gs.ctm, &gs.fill_colorspace, &gs.fill_color, &path)?;
               path.ops.clear();
            }
            // clipping paths are not applied
            "W" | "w*" => {}
            "n" => {
               path.ops.clear();
            }
            "BMC" | "BDC" => {
               mc_stack.push(operation);
            }
            "EMC" => {
               mc_stack.pop();
            }
            "Do" => {
               // `Do` process an entire subdocument, so we do a recursive call to `process_stream`
               // with the subdocument content and resources
               if depth >= MAX_XOBJECT_DEPTH {
                  warn!("skipping XObject nested deeper than {} levels", MAX_XOBJECT_DEPTH);
                  continue;
               }
               let xobject: &Dictionary = get(doc, resources, b"XObject");
               let name = operation.operands[0].as_name().unwrap();
               let xf: &Stream = get(doc, xobject, name);
               let resources = maybe_get_obj(doc, &xf.dict, b"Resources")
                  .and_then(|n| n.as_dict().ok())
                  .unwrap_or(resources);
               let contents = get_contents(xf);
               self.process_stream(doc, contents, resources, media_box, output, depth + 1)?;
            }
            _ => {}
         }
      }
      Ok(())
   }
}
