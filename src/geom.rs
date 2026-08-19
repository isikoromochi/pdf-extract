//! The geometric vocabulary shared by the content-stream processor and the
//! output devices: the coordinate space, the page boxes, and paths.

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
   pub(crate) fn current_point(&self) -> (f64, f64) {
      match *self.ops.last().unwrap() {
         PathOp::MoveTo(x, y) => (x, y),
         PathOp::LineTo(x, y) => (x, y),
         PathOp::CurveTo(_, _, _, _, x, y) => (x, y),
         _ => {
            panic!()
         }
      }
   }
}
