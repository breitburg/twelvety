//! Port of `Util/ProjectTemplateFormats.js`.

use anyhow::{Result, bail};
use indexmap::IndexSet;

#[derive(Clone, Debug, Default)]
pub struct ProjectTemplateFormats {
    use_all_cli: bool,
    use_all_config: bool,
    raw_cli: Option<Vec<String>>,
    values_cli: IndexSet<String>,
    values_config: Option<IndexSet<String>>,
    values_config_add: IndexSet<String>,
}

fn normalize(formats: &[String]) -> IndexSet<String> {
    let joined = formats.join(",");
    let mut out = IndexSet::new();
    for f in joined.split(',') {
        let f = f.trim();
        if !f.is_empty() && f != "*" {
            out.insert(f.to_string());
        }
    }
    out
}

fn is_use_all(formats: &[String]) -> bool {
    formats
        .iter()
        .flat_map(|f| f.split(','))
        .any(|f| f.trim() == "*")
}

impl ProjectTemplateFormats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_wildcard(&self) -> bool {
        self.use_all_cli || self.use_all_config
    }

    pub fn set_via_command_line(&mut self, formats: Option<&str>) {
        let Some(formats) = formats else {
            return;
        };
        let list = vec![formats.to_string()];
        self.use_all_cli = !formats.is_empty() && is_use_all(&list);
        self.values_cli = normalize(&list);
        self.raw_cli = Some(list);
    }

    pub fn set_via_config(&mut self, formats: &[String]) {
        self.use_all_config = is_use_all(formats);
        self.values_config = Some(normalize(formats));
    }

    pub fn add_via_config(&mut self, formats: &[String]) -> Result<()> {
        if formats.is_empty() {
            return Ok(());
        }
        if is_use_all(formats) {
            bail!("`addTemplateFormats(\"*\")` is not supported for project template syntaxes.");
        }
        for f in normalize(formats) {
            self.values_config_add.insert(f);
        }
        Ok(())
    }

    pub fn get_command_line_formats(&self) -> Option<Vec<String>> {
        if self.raw_cli.is_none() || self.use_all_cli {
            return None;
        }
        Some(self.values_cli.iter().cloned().collect())
    }

    pub fn get_all_template_formats(&self) -> Vec<String> {
        let mut out: IndexSet<String> = IndexSet::new();
        if let Some(cfg) = &self.values_config {
            out.extend(cfg.iter().cloned());
        }
        out.extend(self.values_config_add.iter().cloned());
        out.into_iter().collect()
    }

    pub fn get_template_formats(&self) -> Vec<String> {
        if self.use_all_cli {
            return self.get_all_template_formats();
        }
        if self.raw_cli.is_some() {
            return self.values_cli.iter().cloned().collect();
        }
        self.get_all_template_formats()
    }
}
