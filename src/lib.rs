//! Syntax-v0 compiler and interpreter. Source offsets are UTF-8 bytes.
pub mod diagnostic;
pub mod frontend;
pub mod ir;
mod lexer;
pub mod syntax;
