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
   /// The file contradicts what the spec requires at this point: a required
   /// entry is absent, or a value is not of the type the spec gives it.
   #[error("Malformed PDF: {0}")]
   MalformedPdf(String),
   /// The file is well formed, but uses something this crate does not implement.
   #[error("Unsupported PDF feature: {0}")]
   Unsupported(String),
}
