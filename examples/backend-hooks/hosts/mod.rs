//! Three independent hosts for the same compiled YL language.
//!
//! Each host owns its values and scope stack. Shared code only loads the language,
//! declares the hook interface, and renders command-line diagnostics.
#![allow(dead_code, unused_imports)]

mod common;
mod interpreter;
mod javascript;
mod webassembly;

pub use common::{load, render_errors, source_argument};
pub use interpreter::Interpreter;
pub use javascript::JavaScript;
pub use webassembly::Wasm;
