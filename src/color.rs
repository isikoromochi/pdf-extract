//! Colour spaces (32000-1 8.6).
//!
//! Text extraction throws colour away, so these are parsed into the shapes the
//! spec describes and handed to `OutputDev` without ever being converted.

use crate::error::PdfExtractError;
use crate::function::Function;
use crate::object::{get, get_contents, maybe_deref, maybe_get_obj};
use crate::strings::pdf_to_utf8;
use lopdf::{Dictionary, Document, Object};

// Colour space parameters are parsed but not yet applied during extraction.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct CalGray {
   white_point: [f64; 3],
   black_point: Option<[f64; 3]>,
   gamma: Option<f64>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct CalRGB {
   white_point: [f64; 3],
   black_point: Option<[f64; 3]>,
   gamma: Option<[f64; 3]>,
   matrix: Option<Vec<f64>>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Lab {
   white_point: [f64; 3],
   black_point: Option<[f64; 3]>,
   range: Option<[f64; 4]>,
}

#[derive(Clone, Debug)]
pub enum AlternateColorSpace {
   DeviceGray,
   DeviceRGB,
   DeviceCMYK,
   CalRGB(CalRGB),
   CalGray(CalGray),
   Lab(Lab),
   ICCBased(Vec<u8>),
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Separation {
   name: String,
   alternate_space: AlternateColorSpace,
   tint_transform: Box<Function>,
}

#[derive(Clone)]
pub enum ColorSpace {
   DeviceGray,
   DeviceRGB,
   DeviceCMYK,
   DeviceN,
   Pattern,
   CalRGB(CalRGB),
   CalGray(CalGray),
   Lab(Lab),
   Separation(Separation),
   ICCBased(Vec<u8>),
}

/// The entry at `i` of a colour space array. The spec fixes how many entries
/// each family takes, but the file need not agree.
fn operand(cs: &[Object], i: usize) -> Result<&Object, PdfExtractError> {
   cs.get(i)
      .ok_or_else(|| PdfExtractError::MalformedPdf(format!("colour space array has no entry {}", i)))
}

fn operand_name(cs: &[Object], i: usize) -> Result<String, PdfExtractError> {
   Ok(pdf_to_utf8(operand(cs, i)?.as_name()?))
}

pub(crate) fn make_colorspace<'a>(
   doc: &'a Document,
   name: &[u8],
   resources: &'a Dictionary,
) -> Result<ColorSpace, PdfExtractError> {
   Ok(match name {
      b"DeviceGray" => ColorSpace::DeviceGray,
      b"DeviceRGB" => ColorSpace::DeviceRGB,
      b"DeviceCMYK" => ColorSpace::DeviceCMYK,
      b"Pattern" => ColorSpace::Pattern,
      _ => {
         let colorspaces: &Dictionary = get(doc, resources, b"ColorSpace")?;
         let cs: &Object = maybe_get_obj(doc, colorspaces, name).ok_or_else(|| {
            PdfExtractError::MalformedPdf(format!("missing colour space /{}", pdf_to_utf8(name)))
         })?;
         if let Ok(cs) = cs.as_array() {
            let cs_name = operand_name(cs, 0)?;
            match cs_name.as_ref() {
               "Separation" => {
                  let name = operand_name(cs, 1)?;
                  let alternate_space = match maybe_deref(doc, operand(cs, 2)?)? {
                     Object::Name(name) => match &name[..] {
                        b"DeviceGray" => AlternateColorSpace::DeviceGray,
                        b"DeviceRGB" => AlternateColorSpace::DeviceRGB,
                        b"DeviceCMYK" => AlternateColorSpace::DeviceCMYK,
                        _ => {
                           return Err(PdfExtractError::Unsupported(format!(
                              "alternate colour space /{}",
                              pdf_to_utf8(name)
                           )));
                        }
                     },
                     Object::Array(cs) => {
                        let cs_name = operand_name(cs, 0)?;
                        match cs_name.as_ref() {
                           "ICCBased" => {
                              let stream = maybe_deref(doc, operand(cs, 1)?)?.as_stream()?;
                              // XXX: we're going to be continually decompressing everytime this object is referenced
                              AlternateColorSpace::ICCBased(get_contents(stream))
                           }
                           "CalGray" => AlternateColorSpace::CalGray(cal_gray(doc, operand(cs, 1)?.as_dict()?)?),
                           "CalRGB" => AlternateColorSpace::CalRGB(cal_rgb(doc, operand(cs, 1)?.as_dict()?)?),
                           "Lab" => AlternateColorSpace::Lab(lab(doc, operand(cs, 1)?.as_dict()?)?),
                           _ => {
                              return Err(PdfExtractError::Unsupported(format!(
                                 "alternate colour space {}",
                                 cs_name
                              )));
                           }
                        }
                     }
                     other => {
                        return Err(PdfExtractError::MalformedPdf(format!(
                           "an alternate colour space must be a name or an array, found {:?}",
                           other
                        )));
                     }
                  };
                  let tint_transform = Box::new(Function::new(doc, maybe_deref(doc, operand(cs, 3)?)?)?);

                  ColorSpace::Separation(Separation {
                     name,
                     alternate_space,
                     tint_transform,
                  })
               }
               "ICCBased" => {
                  let stream = maybe_deref(doc, operand(cs, 1)?)?.as_stream()?;
                  // XXX: we're going to be continually decompressing everytime this object is referenced
                  ColorSpace::ICCBased(get_contents(stream))
               }
               "CalGray" => ColorSpace::CalGray(cal_gray(doc, operand(cs, 1)?.as_dict()?)?),
               "CalRGB" => ColorSpace::CalRGB(cal_rgb(doc, operand(cs, 1)?.as_dict()?)?),
               "Lab" => ColorSpace::Lab(lab(doc, operand(cs, 1)?.as_dict()?)?),
               "Pattern" => ColorSpace::Pattern,
               "DeviceGray" => ColorSpace::DeviceGray,
               "DeviceRGB" => ColorSpace::DeviceRGB,
               "DeviceCMYK" => ColorSpace::DeviceCMYK,
               "DeviceN" => ColorSpace::DeviceN,
               _ => {
                  return Err(PdfExtractError::Unsupported(format!("colour space {}", cs_name)));
               }
            }
         } else if let Ok(cs) = cs.as_name() {
            match pdf_to_utf8(cs).as_ref() {
               "DeviceRGB" => ColorSpace::DeviceRGB,
               "DeviceGray" => ColorSpace::DeviceGray,
               other => {
                  return Err(PdfExtractError::Unsupported(format!("colour space /{}", other)));
               }
            }
         } else {
            return Err(PdfExtractError::MalformedPdf(
               "a colour space must be a name or an array".to_owned(),
            ));
         }
      }
   })
}

fn cal_gray(doc: &Document, dict: &Dictionary) -> Result<CalGray, PdfExtractError> {
   Ok(CalGray {
      white_point: get(doc, dict, b"WhitePoint")?,
      black_point: get(doc, dict, b"BackPoint")?,
      gamma: get(doc, dict, b"Gamma")?,
   })
}

fn cal_rgb(doc: &Document, dict: &Dictionary) -> Result<CalRGB, PdfExtractError> {
   Ok(CalRGB {
      white_point: get(doc, dict, b"WhitePoint")?,
      black_point: get(doc, dict, b"BackPoint")?,
      gamma: get(doc, dict, b"Gamma")?,
      matrix: get(doc, dict, b"Matrix")?,
   })
}

fn lab(doc: &Document, dict: &Dictionary) -> Result<Lab, PdfExtractError> {
   Ok(Lab {
      white_point: get(doc, dict, b"WhitePoint")?,
      black_point: get(doc, dict, b"BackPoint")?,
      range: get(doc, dict, b"Range")?,
   })
}
