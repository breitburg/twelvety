//! A Liquid template engine modelled after liquidjs (the library used upstream).
//!
//! Values are the crate-wide dynamic [`crate::value::Value`] so no conversion is
//! needed between the data cascade and templates.

pub mod filters;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod strftime;

pub use parser::Template;
pub use render::{Liquid, LiquidOptions, PartialLoader};
