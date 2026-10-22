//! BeanyLab language engine.
//!
//! A clean-room implementation of a matrix programming language that reads `.m` files:
//! lexer → parser → tree-walking interpreter, with a numeric library and a figure model
//! that a UI renders.

// Numeric kernels index several parallel arrays by position, and the interpreter's callback
// and plot-group types are clearer spelled out than hidden behind aliases.
#![allow(clippy::needless_range_loop, clippy::type_complexity)]

pub mod ast;
pub mod builtins;
pub mod classes;
pub mod display;
pub mod error;
pub mod graphics;
pub mod interp;
pub mod lexer;
pub mod mat;
pub mod ops;
pub mod parser;
pub mod session;
pub mod sim;
pub mod value;

pub use error::{MError, MResult};
pub use interp::Interp;
pub use session::{EvalResult, Session};
pub use value::Value;
