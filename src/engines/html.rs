//! Port of `Engines/Html.js`.

use std::sync::Arc;

use anyhow::Result;

use super::{RenderFn, TemplateEngine};

pub struct Html {
    name: String,
}

impl Html {
    pub fn new(name: &str) -> Self {
        Html { name: name.into() }
    }
}

impl TemplateEngine for Html {
    fn name(&self) -> &str {
        &self.name
    }

    fn cacheable(&self) -> bool {
        true
    }

    fn compile(&self, src: &str, _input_path: Option<&str>) -> Result<RenderFn> {
        let s = src.to_string();
        Ok(Arc::new(move |_| Ok(s.clone())))
    }
}
