//! Twelvety: a Rust port of Build Awesome (Twelvety), a simpler static site generator.

pub mod build_context;
pub mod config_file;
pub mod core;
pub mod data;
pub mod default_config;
pub mod engines;
pub mod errors;
pub mod extension_map;
pub mod file_system_search;
pub mod files;
pub mod filters;
pub mod global_dependency_map;
pub mod liquid;
pub mod page_entry;
pub mod plugins;
pub mod template;
pub mod template_collection;
pub mod template_config;
pub mod template_content;
pub mod template_file_slug;
pub mod template_layout;
pub mod template_map;
pub mod template_passthrough;
pub mod template_permalink;
pub mod template_pipeline;
pub mod template_render;
pub mod template_writer;
pub mod user_config;
pub mod util;
pub mod value;

pub use crate::core::{Twelvety, Options};
pub use crate::user_config::UserConfig;
pub use crate::value::Value;
pub mod serve;
pub mod watch;
pub mod testing;
