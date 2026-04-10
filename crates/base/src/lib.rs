pub mod assertion_error;
pub mod cli;
pub mod compilation_stage;
pub mod diagnostic_level;
pub mod error;
pub mod file_path;
pub mod line_bounds;
pub mod logging;
pub mod render_source_diagnostics;
pub mod result;
pub mod shared_string;
pub mod source_annotation;
pub mod source_diagnostic;
pub mod source_diagnostics;
pub mod source_excerpt;
pub mod source_file;
pub mod span;
pub mod timestamp;

pub use parking_lot::{Mutex, RwLock};

pub fn unansi(string: &str) -> String {
    anstream::adapter::strip_str(string).to_string()
}
