//! blast-radius: predict what a code change might break.
//!
//! 1. [`parse`] reads a Rust codebase with `syn`: functions, their kinds, their calls.

pub mod parse;
