//! blast-radius: predict what a code change might break.
//!
//! 1. [`parse`] reads a Rust codebase with `syn`: functions, their kinds, their calls.
//! 2. [`graph`] resolves the calls into a call graph, each edge with a confidence.
//! 3. [`impact`] walks the graph backwards from a function: its blast radius and a risk score.
//! 4. [`predict`] asks a model what the proposed change could break, and checks its
//!    answer against the graph.

pub mod config;
pub mod graph;
pub mod impact;
pub mod openai;
pub mod parse;
pub mod predict;
pub mod report;
