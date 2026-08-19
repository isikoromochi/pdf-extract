//! The error type shared by every entry point in the crate.

/// Everything the public API can fail with.
#[derive(Debug, thiserror::Error)]
pub enum PdfExtractError {
   #[error("Formating error: {0}")]
   FormatError(#[from] std::fmt::Error),
   #[error("IO error: {0}")]
   IoError(#[from] std::io::Error),
   #[error("PDF error: {0}")]
   PdfError(#[from] lopdf::Error),
}
