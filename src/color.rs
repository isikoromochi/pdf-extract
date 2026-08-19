//! Colour spaces (32000-1 8.6).
//!
//! Text extraction throws colour away, so these are parsed into the shapes the
//! spec describes and handed to `OutputDev` without ever being converted.

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

pub(crate) fn make_colorspace<'a>(doc: &'a Document, name: &[u8], resources: &'a Dictionary) -> ColorSpace {
   match name {
      b"DeviceGray" => ColorSpace::DeviceGray,
      b"DeviceRGB" => ColorSpace::DeviceRGB,
      b"DeviceCMYK" => ColorSpace::DeviceCMYK,
      b"Pattern" => ColorSpace::Pattern,
      _ => {
         let colorspaces: &Dictionary = get(doc, resources, b"ColorSpace");
         let cs: &Object =
            maybe_get_obj(doc, colorspaces, name).unwrap_or_else(|| panic!("missing colorspace {:?}", name));
         if let Ok(cs) = cs.as_array() {
            let cs_name = pdf_to_utf8(cs[0].as_name().expect("first arg must be a name"));
            match cs_name.as_ref() {
               "Separation" => {
                  let name = pdf_to_utf8(cs[1].as_name().expect("second arg must be a name"));
                  let alternate_space = match &maybe_deref(doc, &cs[2]) {
                     Object::Name(name) => match &name[..] {
                        b"DeviceGray" => AlternateColorSpace::DeviceGray,
                        b"DeviceRGB" => AlternateColorSpace::DeviceRGB,
                        b"DeviceCMYK" => AlternateColorSpace::DeviceCMYK,
                        _ => panic!("unexpected color space name"),
                     },
                     Object::Array(cs) => {
                        let cs_name = pdf_to_utf8(cs[0].as_name().expect("first arg must be a name"));
                        match cs_name.as_ref() {
                           "ICCBased" => {
                              let stream = maybe_deref(doc, &cs[1]).as_stream().unwrap();
                              // XXX: we're going to be continually decompressing everytime this object is referenced
                              AlternateColorSpace::ICCBased(get_contents(stream))
                           }
                           "CalGray" => {
                              let dict = cs[1].as_dict().expect("second arg must be a dict");
                              AlternateColorSpace::CalGray(CalGray {
                                 white_point: get(doc, dict, b"WhitePoint"),
                                 black_point: get(doc, dict, b"BackPoint"),
                                 gamma: get(doc, dict, b"Gamma"),
                              })
                           }
                           "CalRGB" => {
                              let dict = cs[1].as_dict().expect("second arg must be a dict");
                              AlternateColorSpace::CalRGB(CalRGB {
                                 white_point: get(doc, dict, b"WhitePoint"),
                                 black_point: get(doc, dict, b"BackPoint"),
                                 gamma: get(doc, dict, b"Gamma"),
                                 matrix: get(doc, dict, b"Matrix"),
                              })
                           }
                           "Lab" => {
                              let dict = cs[1].as_dict().expect("second arg must be a dict");
                              AlternateColorSpace::Lab(Lab {
                                 white_point: get(doc, dict, b"WhitePoint"),
                                 black_point: get(doc, dict, b"BackPoint"),
                                 range: get(doc, dict, b"Range"),
                              })
                           }
                           _ => panic!("Unexpected color space name"),
                        }
                     }
                     _ => panic!("Alternate space should be name or array {:?}", cs[2]),
                  };
                  let tint_transform = Box::new(Function::new(doc, maybe_deref(doc, &cs[3])));

                  ColorSpace::Separation(Separation {
                     name,
                     alternate_space,
                     tint_transform,
                  })
               }
               "ICCBased" => {
                  let stream = maybe_deref(doc, &cs[1]).as_stream().unwrap();
                  // XXX: we're going to be continually decompressing everytime this object is referenced
                  ColorSpace::ICCBased(get_contents(stream))
               }
               "CalGray" => {
                  let dict = cs[1].as_dict().expect("second arg must be a dict");
                  ColorSpace::CalGray(CalGray {
                     white_point: get(doc, dict, b"WhitePoint"),
                     black_point: get(doc, dict, b"BackPoint"),
                     gamma: get(doc, dict, b"Gamma"),
                  })
               }
               "CalRGB" => {
                  let dict = cs[1].as_dict().expect("second arg must be a dict");
                  ColorSpace::CalRGB(CalRGB {
                     white_point: get(doc, dict, b"WhitePoint"),
                     black_point: get(doc, dict, b"BackPoint"),
                     gamma: get(doc, dict, b"Gamma"),
                     matrix: get(doc, dict, b"Matrix"),
                  })
               }
               "Lab" => {
                  let dict = cs[1].as_dict().expect("second arg must be a dict");
                  ColorSpace::Lab(Lab {
                     white_point: get(doc, dict, b"WhitePoint"),
                     black_point: get(doc, dict, b"BackPoint"),
                     range: get(doc, dict, b"Range"),
                  })
               }
               "Pattern" => ColorSpace::Pattern,
               "DeviceGray" => ColorSpace::DeviceGray,
               "DeviceRGB" => ColorSpace::DeviceRGB,
               "DeviceCMYK" => ColorSpace::DeviceCMYK,
               "DeviceN" => ColorSpace::DeviceN,
               _ => {
                  panic!("color_space {:?} {:?} {:?}", name, cs_name, cs)
               }
            }
         } else if let Ok(cs) = cs.as_name() {
            match pdf_to_utf8(cs).as_ref() {
               "DeviceRGB" => ColorSpace::DeviceRGB,
               "DeviceGray" => ColorSpace::DeviceGray,
               _ => panic!(),
            }
         } else {
            panic!();
         }
      }
   }
}
