//! PDF functions (32000-1 7.10).
//!
//! Only enough of each type is parsed to carry a colour space's tint transform
//! around; none of them are evaluated, since text extraction discards colour.

use crate::object::{get, get_contents};
use log::warn;
use lopdf::{Document, Object};

// Parsed straight out of the PDF; most fields are not consumed yet.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct Type0Func {
   domain: Vec<f64>,
   range: Vec<f64>,
   contents: Vec<u8>,
   size: Vec<i64>,
   bits_per_sample: i64,
   encode: Vec<f64>,
   decode: Vec<f64>,
}

#[allow(dead_code)]
fn interpolate(x: f64, x_min: f64, _x_max: f64, y_min: f64, y_max: f64) -> f64 {
   let divisor = x - x_min;
   if divisor != 0. {
      y_min + (x - x_min) * ((y_max - y_min) / divisor)
   } else {
      // (x - x_min) will be 0 which means we want to discard the interpolation
      // and arbitrarily choose y_min to match pdfium
      y_min
   }
}

impl Type0Func {
   #[allow(dead_code)]
   fn eval(&self, _input: &[f64], _output: &mut [f64]) {
      let _n_inputs = self.domain.len() / 2;
      let _n_ouputs = self.range.len() / 2;
   }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct Type2Func {
   c0: Option<Vec<f64>>,
   c1: Option<Vec<f64>>,
   n: f64,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum Function {
   Type0(Type0Func),
   Type2(Type2Func),
   Type3,
   Type4(Vec<u8>),
}

impl Function {
   pub(crate) fn new(doc: &Document, obj: &Object) -> Function {
      let dict = match obj {
         Object::Dictionary(dict) => dict,
         Object::Stream(stream) => &stream.dict,
         _ => panic!(),
      };
      let function_type: i64 = get(doc, dict, b"FunctionType");

      match function_type {
         0 => {
            // Sampled function
            let stream = match obj {
               Object::Stream(stream) => stream,
               _ => panic!(),
            };
            let range: Vec<f64> = get(doc, dict, b"Range");
            let domain: Vec<f64> = get(doc, dict, b"Domain");
            let contents = get_contents(stream);
            let size: Vec<i64> = get(doc, dict, b"Size");
            let bits_per_sample = get(doc, dict, b"BitsPerSample");
            // We ignore 'Order' like pdfium, poppler and pdf.js

            let encode = get::<Option<Vec<f64>>>(doc, dict, b"Encode");
            // maybe there's some better way to write this.
            let encode = encode.unwrap_or_else(|| {
               let mut default = Vec::new();
               for i in &size {
                  default.extend([0., (i - 1) as f64].iter());
               }
               default
            });
            let decode = get::<Option<Vec<f64>>>(doc, dict, b"Decode").unwrap_or_else(|| range.clone());

            Function::Type0(Type0Func {
               domain,
               range,
               size,
               contents,
               bits_per_sample,
               encode,
               decode,
            })
         }
         2 => {
            // Exponential interpolation function
            let c0 = get::<Option<Vec<f64>>>(doc, dict, b"C0");
            let c1 = get::<Option<Vec<f64>>>(doc, dict, b"C1");
            let n = get::<f64>(doc, dict, b"N");
            Function::Type2(Type2Func { c0, c1, n })
         }
         3 => {
            // Stitching function
            Function::Type3
         }
         4 => {
            // PostScript calculator function
            let contents = match obj {
               Object::Stream(stream) => {
                  let contents = get_contents(stream);
                  warn!("unhandled type-4 function");
                  warn!("Stream: {}", String::from_utf8(contents.clone()).unwrap());
                  contents
               }
               _ => {
                  panic!("type 4 functions should be streams")
               }
            };
            Function::Type4(contents)
         }
         _ => {
            panic!("unhandled function type {}", function_type)
         }
      }
   }
}
