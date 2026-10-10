//! Declarative language compiler and grammar interpreter. Source offsets are UTF-8 bytes.
mod compiler;
pub mod diagnostic;
pub mod frontend;
pub mod ir;
pub use compiler::{compile_language, compile_sources};
pub use ir::CompiledLanguage;
mod artifact;
mod runtime;
pub use artifact::load_compiled_language;
pub use runtime::{parse, parse_named, AstNode, AstValue, ParseResult};
mod lexer;
pub mod semantics;
pub mod syntax;
