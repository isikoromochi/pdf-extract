//! SVG, carrying the filled paths but not the text.

use crate::color::ColorSpace;
use crate::error::PdfExtractError;
use crate::geom::{MediaBox, Path, PathOp, Transform};
use crate::output::OutputDev;
use euclid::vec2;

pub struct SVGOutput<'a> {
   file: &'a mut dyn std::io::Write,
}
impl<'a> SVGOutput<'a> {
   pub fn new(file: &mut dyn std::io::Write) -> SVGOutput<'_> {
      SVGOutput { file }
   }
}

impl<'a> OutputDev for SVGOutput<'a> {
   fn begin_page(
      &mut self,
      _page_num: u32,
      media_box: &MediaBox,
      art_box: Option<(f64, f64, f64, f64)>,
   ) -> Result<(), PdfExtractError> {
      let ver = 1.1;
      writeln!(self.file, "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>")?;
      if ver == 1.1 {
         write!(
            self.file,
            r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#
         )?;
      } else {
         write!(
            self.file,
            r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.0//EN" "http://www.w3.org/TR/2001/REC-SVG-20010904/DTD/svg10.dtd">"#
         )?;
      }
      if let Some(art_box) = art_box {
         let width = art_box.2 - art_box.0;
         let height = art_box.3 - art_box.1;
         let y = media_box.ury - art_box.1 - height;
         write!(
            self.file,
            "<svg width=\"{}\" height=\"{}\" xmlns=\"http://www.w3.org/2000/svg\" version=\"{}\" viewBox='{} {} {} {}'>",
            width, height, ver, art_box.0, y, width, height
         )?;
      } else {
         let width = media_box.urx - media_box.llx;
         let height = media_box.ury - media_box.lly;
         write!(
            self.file,
            "<svg width=\"{}\" height=\"{}\" xmlns=\"http://www.w3.org/2000/svg\" version=\"{}\" viewBox='{} {} {} {}'>",
            width, height, ver, media_box.llx, media_box.lly, width, height
         )?;
      }
      writeln!(self.file)?;
      type Mat = Transform;

      let ctm = Mat::create_scale(1., -1.).post_translate(vec2(0., media_box.ury));
      writeln!(
         self.file,
         "<g transform='matrix({}, {}, {}, {}, {}, {})'>",
         ctm.m11, ctm.m12, ctm.m21, ctm.m22, ctm.m31, ctm.m32,
      )?;
      Ok(())
   }
   fn end_page(&mut self) -> Result<(), PdfExtractError> {
      writeln!(self.file, "</g>")?;
      write!(self.file, "</svg>")?;
      Ok(())
   }
   fn output_character(
      &mut self,
      _trm: &Transform,
      _width: f64,
      _spacing: f64,
      _font_size: f64,
      _char: &str,
   ) -> Result<(), PdfExtractError> {
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
   fn fill(
      &mut self,
      ctm: &Transform,
      _colorspace: &ColorSpace,
      _color: &[f64],
      path: &Path,
   ) -> Result<(), PdfExtractError> {
      write!(
         self.file,
         "<g transform='matrix({}, {}, {}, {}, {}, {})'>",
         ctm.m11, ctm.m12, ctm.m21, ctm.m22, ctm.m31, ctm.m32,
      )?;

      /*if path.ops.len() == 1 {
          if let PathOp::Rect(x, y, width, height) = path.ops[0] {
              write!(self.file, "<rect x={} y={} width={} height={} />\n", x, y, width, height);
              write!(self.file, "</g>");
              return;
          }
      }*/
      let mut d = Vec::new();
      for op in &path.ops {
         match *op {
            PathOp::MoveTo(x, y) => d.push(format!("M{} {}", x, y)),
            PathOp::LineTo(x, y) => d.push(format!("L{} {}", x, y)),
            PathOp::CurveTo(x1, y1, x2, y2, x, y) => d.push(format!("C{} {} {} {} {} {}", x1, y1, x2, y2, x, y)),
            PathOp::Close => d.push("Z".to_string()),
            PathOp::Rect(x, y, width, height) => {
               d.push(format!("M{} {}", x, y));
               d.push(format!("L{} {}", x + width, y));
               d.push(format!("L{} {}", x + width, y + height));
               d.push(format!("L{} {}", x, y + height));
               d.push("Z".to_string());
            }
         }
      }
      write!(self.file, "<path d='{}' />", d.join(" "))?;
      write!(self.file, "</g>")?;
      writeln!(self.file)?;
      Ok(())
   }
}
