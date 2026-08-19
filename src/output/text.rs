//! Plain text.
//!
//! PDF has no notion of a word or a line, so both are guessed back from the
//! positions the glyphs were placed at.

use crate::error::PdfExtractError;
use crate::geom::{ArtBox, MediaBox, Transform};
use crate::output::{ConvertToFmt, OutputDev};
use euclid::{Transform2D, vec2};

pub struct PlainTextOutput<W: ConvertToFmt> {
   writer: W::Writer,
   last_end: f64,
   last_y: f64,
   first_char: bool,
   flip_ctm: Transform,
}

impl<W: ConvertToFmt> PlainTextOutput<W> {
   pub fn new(writer: W) -> PlainTextOutput<W> {
      PlainTextOutput {
         writer: writer.convert(),
         last_end: 100000.,
         first_char: false,
         last_y: 0.,
         flip_ctm: Transform2D::identity(),
      }
   }
}

/* There are some structural hints that PDFs can use to signal word and line endings:
 * however relying on these is not likely to be sufficient. */
impl<W: ConvertToFmt> OutputDev for PlainTextOutput<W> {
   fn begin_page(&mut self, _page_num: u32, media_box: &MediaBox, _: Option<ArtBox>) -> Result<(), PdfExtractError> {
      self.flip_ctm = Transform2D::row_major(1., 0., 0., -1., 0., media_box.ury - media_box.lly);
      Ok(())
   }
   fn end_page(&mut self) -> Result<(), PdfExtractError> {
      Ok(())
   }
   fn output_character(
      &mut self,
      trm: &Transform,
      width: f64,
      _spacing: f64,
      font_size: f64,
      char: &str,
   ) -> Result<(), PdfExtractError> {
      let position = trm.post_transform(&self.flip_ctm);
      let transformed_font_size_vec = trm.transform_vector(vec2(font_size, font_size));
      // get the length of one sized of the square with the same area with a rectangle of size (x, y)
      let transformed_font_size = (transformed_font_size_vec.x * transformed_font_size_vec.y).sqrt();
      let (x, y) = (position.m31, position.m32);
      use std::fmt::Write;
      if self.first_char {
         if (y - self.last_y).abs() > transformed_font_size * 1.5 {
            writeln!(self.writer)?;
         }

         // we've moved to the left and down
         if x < self.last_end && (y - self.last_y).abs() > transformed_font_size * 0.5 {
            writeln!(self.writer)?;
         }

         if x > self.last_end + transformed_font_size * 0.1 {
            write!(self.writer, " ")?;
         }
      }
      //let norm = unicode_normalization::UnicodeNormalization::nfkc(char);
      write!(self.writer, "{}", char)?;
      self.first_char = false;
      self.last_y = y;
      self.last_end = x + width * transformed_font_size;
      Ok(())
   }
   fn begin_word(&mut self) -> Result<(), PdfExtractError> {
      self.first_char = true;
      Ok(())
   }
   fn end_word(&mut self) -> Result<(), PdfExtractError> {
      Ok(())
   }
   fn end_line(&mut self) -> Result<(), PdfExtractError> {
      //write!(self.file, "\n");
      Ok(())
   }
}
