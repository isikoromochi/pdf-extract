//! Absolutely-positioned HTML, one `<div>` per run of glyphs.

use crate::error::PdfExtractError;
use crate::geom::{ArtBox, MediaBox, Transform};
use crate::output::OutputDev;
use euclid::{Transform2D, vec2};
use log::warn;

pub struct HTMLOutput<'a> {
   file: &'a mut dyn std::io::Write,
   flip_ctm: Transform,
   last_ctm: Transform,
   buf_ctm: Transform,
   buf_font_size: f64,
   buf: String,
}

fn insert_nbsp(input: &str) -> String {
   let mut result = String::new();
   let mut word_end = false;
   let mut chars = input.chars().peekable();
   while let Some(c) = chars.next() {
      if c == ' ' {
         if !word_end || chars.peek().filter(|x| **x != ' ').is_none() {
            result += "&nbsp;";
         } else {
            result += " ";
         }
         word_end = false;
      } else {
         word_end = true;
         result.push(c);
      }
   }
   result
}

impl<'a> HTMLOutput<'a> {
   pub fn new(file: &mut dyn std::io::Write) -> HTMLOutput<'_> {
      HTMLOutput {
         file,
         flip_ctm: Transform2D::identity(),
         last_ctm: Transform2D::identity(),
         buf_ctm: Transform2D::identity(),
         buf: String::new(),
         buf_font_size: 0.,
      }
   }
   fn flush_string(&mut self) -> Result<(), PdfExtractError> {
      if !self.buf.is_empty() {
         let position = self.buf_ctm.post_transform(&self.flip_ctm);
         let transformed_font_size_vec = self.buf_ctm.transform_vector(vec2(self.buf_font_size, self.buf_font_size));
         // get the length of one sized of the square with the same area with a rectangle of size (x, y)
         let transformed_font_size = (transformed_font_size_vec.x * transformed_font_size_vec.y).sqrt();
         let (x, y) = (position.m31, position.m32);
         warn!("flush {} {:?}", self.buf, (x, y));

         writeln!(
            self.file,
            "<div style='position: absolute; left: {}px; top: {}px; font-size: {}px'>{}</div>",
            x,
            y,
            transformed_font_size,
            insert_nbsp(&self.buf)
         )?;
      }
      Ok(())
   }
}

impl<'a> OutputDev for HTMLOutput<'a> {
   fn begin_page(&mut self, page_num: u32, media_box: &MediaBox, _: Option<ArtBox>) -> Result<(), PdfExtractError> {
      write!(self.file, "<meta charset='utf-8' /> ")?;
      write!(self.file, "<!-- page {} -->", page_num)?;
      write!(
         self.file,
         "<div id='page{}' style='position: relative; height: {}px; width: {}px; border: 1px black solid'>",
         page_num,
         media_box.ury - media_box.lly,
         media_box.urx - media_box.llx
      )?;
      self.flip_ctm = Transform::row_major(1., 0., 0., -1., 0., media_box.ury - media_box.lly);
      Ok(())
   }
   fn end_page(&mut self) -> Result<(), PdfExtractError> {
      self.flush_string()?;
      self.buf = String::new();
      self.last_ctm = Transform::identity();
      write!(self.file, "</div>")?;
      Ok(())
   }
   fn output_character(
      &mut self,
      trm: &Transform,
      width: f64,
      spacing: f64,
      font_size: f64,
      char: &str,
   ) -> Result<(), PdfExtractError> {
      if trm.approx_eq(&self.last_ctm) {
         let position = trm.post_transform(&self.flip_ctm);
         let (x, y) = (position.m31, position.m32);

         warn!("accum {} {:?}", char, (x, y));
         self.buf += char;
      } else {
         warn!("flush {} {:?} {:?} {} {} {}", char, trm, self.last_ctm, width, font_size, spacing);
         self.flush_string()?;
         self.buf = char.to_owned();
         self.buf_font_size = font_size;
         self.buf_ctm = *trm;
      }
      let position = trm.post_transform(&self.flip_ctm);
      let transformed_font_size_vec = trm.transform_vector(vec2(font_size, font_size));
      // get the length of one sized of the square with the same area with a rectangle of size (x, y)
      let transformed_font_size = (transformed_font_size_vec.x * transformed_font_size_vec.y).sqrt();
      let (x, y) = (position.m31, position.m32);
      write!(
         self.file,
         "<div style='position: absolute; color: red; left: {}px; top: {}px; font-size: {}px'>{}</div>",
         x, y, transformed_font_size, char
      )?;
      self.last_ctm = trm.pre_transform(&Transform2D::create_translation(width * font_size + spacing, 0.));

      Ok(())
   }
   fn begin_word(&mut self) -> Result<(), PdfExtractError> {
      Ok(())
   }
   fn end_word(&mut self) -> Result<(), PdfExtractError> {
      Ok(())
   }
   fn end_line(&mut self) -> Result<(), PdfExtractError> {
      Ok(())
   }
}
