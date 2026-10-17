//! Declarative language compiler and grammar interpreter. Source offsets are UTF-8 bytes.
mod compiler;
pub mod diagnostic;
pub mod diagnostic_render;
pub mod frontend;
pub mod ir;
pub use compiler::{compile_language, compile_sources};
pub use ir::{CompiledLanguage, HighlightSpan};
mod artifact;
pub mod image;
mod runtime;
pub use artifact::load_compiled_language;
pub use runtime::{parse, parse_named, AstNode, AstValue, ParseResult};
mod lexer;
pub mod semantics;
pub mod syntax;

mod formatting;
pub use formatting::{format_yl, format_yl_with_options, FormatOptions};

pub mod examples;
pub mod language_server;
pub mod project;
