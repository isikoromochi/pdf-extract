//! Where the processor sends what it finds.
//!
//! `OutputDev` is called with each glyph and its device-space transform; turning
//! that stream back into lines and words is each implementation's own problem.

mod html;
mod svg;
mod text;

pub use html::HTMLOutput;
pub use svg::SVGOutput;
pub use text::PlainTextOutput;

use crate::color::ColorSpace;
use crate::error::PdfExtractError;
use crate::geom::{MediaBox, Path, Transform};
use std::fmt;
use std::fs::File;

pub trait OutputDev {
   fn begin_page(
      &mut self,
      page_num: u32,
      media_box: &MediaBox,
      art_box: Option<(f64, f64, f64, f64)>,
   ) -> Result<(), PdfExtractError>;
   fn end_page(&mut self) -> Result<(), PdfExtractError>;
   fn output_character(
      &mut self,
      trm: &Transform,
      width: f64,
      spacing: f64,
      font_size: f64,
      char: &str,
   ) -> Result<(), PdfExtractError>;
   fn begin_word(&mut self) -> Result<(), PdfExtractError>;
   fn end_word(&mut self) -> Result<(), PdfExtractError>;
   fn end_line(&mut self) -> Result<(), PdfExtractError>;
   fn stroke(
      &mut self,
      _ctm: &Transform,
      _colorspace: &ColorSpace,
      _color: &[f64],
      _path: &Path,
   ) -> Result<(), PdfExtractError> {
      Ok(())
   }
   fn fill(
      &mut self,
      _ctm: &Transform,
      _colorspace: &ColorSpace,
      _color: &[f64],
      _path: &Path,
   ) -> Result<(), PdfExtractError> {
      Ok(())
   }
}

/*
File doesn't implement std::fmt::Write so we have
to do some gymnastics to accept a File or String
See https://github.com/rust-lang/rust/issues/51305
*/

pub trait ConvertToFmt {
   type Writer: std::fmt::Write;
   fn convert(self) -> Self::Writer;
}

impl<'a> ConvertToFmt for &'a mut String {
   type Writer = &'a mut String;
   fn convert(self) -> Self::Writer {
      self
   }
}

pub struct WriteAdapter<W> {
   f: W,
}

impl<W: std::io::Write> std::fmt::Write for WriteAdapter<W> {
   fn write_str(&mut self, s: &str) -> Result<(), std::fmt::Error> {
      self.f.write_all(s.as_bytes()).map_err(|_| fmt::Error)
   }
}

impl ConvertToFmt for &mut dyn std::io::Write {
   type Writer = WriteAdapter<Self>;
   fn convert(self) -> Self::Writer {
      WriteAdapter { f: self }
   }
}

impl ConvertToFmt for &mut File {
   type Writer = WriteAdapter<Self>;
   fn convert(self) -> Self::Writer {
      WriteAdapter { f: self }
   }
}
