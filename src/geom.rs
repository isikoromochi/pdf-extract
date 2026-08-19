//! The geometric vocabulary shared by the content-stream processor and the
//! output devices: the coordinate space, the page boxes, and paths.

use crate::error::PdfExtractError;
use euclid::Transform2D;

/// euclid tags a transform with the spaces it maps between. This crate does not
/// distinguish user, text and device space at the type level, so every
/// `Transform` maps `Space` to `Space`.
pub struct Space;
pub type Transform = Transform2D<f64, Space, Space>;

#[derive(Debug, Clone, Copy)]
pub struct MediaBox {
   pub llx: f64,
   pub lly: f64,
   pub urx: f64,
   pub ury: f64,
}

pub(crate) type ArtBox = (f64, f64, f64, f64);

#[derive(Debug)]
pub enum PathOp {
   MoveTo(f64, f64),
   LineTo(f64, f64),
   // XXX: is it worth distinguishing the different kinds of curve ops?
   CurveTo(f64, f64, f64, f64, f64, f64),
   Rect(f64, f64, f64, f64),
   Close,
}

#[derive(Debug)]
pub struct Path {
   pub ops: Vec<PathOp>,
}

impl Path {
   pub(crate) fn new() -> Path {
      Path { ops: Vec::new() }
   }
   /// Where the last drawn segment left off. `v` needs it, and a file can name
   /// `v` with no current point at all.
   pub(crate) fn current_point(&self) -> Result<(f64, f64), PdfExtractError> {
      match self.ops.last() {
         Some(&PathOp::MoveTo(x, y)) => Ok((x, y)),
         Some(&PathOp::LineTo(x, y)) => Ok((x, y)),
         Some(&PathOp::CurveTo(_, _, _, _, x, y)) => Ok((x, y)),
         _ => Err(PdfExtractError::MalformedPdf(
            "a path operator needs a current point, and the path has none".to_owned(),
         )),
      }
   }
}
