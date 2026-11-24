//! Port of `GlobalDependencyMap.js` and `Util/TemplateDepGraph.js`.
//!
//! The dependency graph decides the order templates are processed in: templates
//! that consume collections are processed after all templates that publish to
//! those collections.

use std::collections::{HashMap, HashSet};

use indexmap::{IndexMap, IndexSet};

use crate::util::template_path;

pub const COLLECTION_PREFIX: &str = "__collection:";
const STAGES: [&str; 4] = ["[basic]", "[userconfig]", "[keys]", "all"];

/// Port of the `dependency-graph` package's `DepGraph` (with `circular: true`)
#[derive(Clone, Default, Debug)]
pub struct DepGraph {
    nodes: IndexSet<String>,
    outgoing: HashMap<String, Vec<String>>,
    incoming: HashMap<String, Vec<String>>,
}

impl DepGraph {
    pub fn has_node(&self, node: &str) -> bool {
        self.nodes.contains(node)
    }

    pub fn add_node(&mut self, node: &str) {
        if self.nodes.insert(node.to_string()) {
            self.outgoing.insert(node.to_string(), vec![]);
            self.incoming.insert(node.to_string(), vec![]);
        }
    }

    pub fn remove_node(&mut self, node: &str) {
        if !self.nodes.shift_remove(node) {
            return;
        }
        self.outgoing.remove(node);
        self.incoming.remove(node);
        for edges in self.outgoing.values_mut() {
            edges.retain(|e| e != node);
        }
        for edges in self.incoming.values_mut() {
            edges.retain(|e| e != node);
        }
    }

    pub fn add_dependency(&mut self, from: &str, to: &str) {
        self.add_node(from);
        self.add_node(to);
        let out = self.outgoing.get_mut(from).unwrap();
        if !out.iter().any(|e| e == to) {
            out.push(to.to_string());
            self.incoming.get_mut(to).unwrap().push(from.to_string());
        }
    }

    pub fn remove_dependency(&mut self, from: &str, to: &str) {
        if let Some(out) = self.outgoing.get_mut(from) {
            out.retain(|e| e != to);
        }
        if let Some(inc) = self.incoming.get_mut(to) {
            inc.retain(|e| e != from);
        }
    }

    pub fn direct_dependencies_of(&self, node: &str) -> Vec<String> {
        self.outgoing.get(node).cloned().unwrap_or_default()
    }

    pub fn direct_dependants_of(&self, node: &str) -> Vec<String> {
        self.incoming.get(node).cloned().unwrap_or_default()
    }

    fn dfs(&self, edges: &HashMap<String, Vec<String>>, start: &str, visited: &mut HashSet<String>, result: &mut Vec<String>) {
        if visited.contains(start) {
            return;
        }
        let mut in_path: HashSet<String> = HashSet::new();
        let mut todo: Vec<(String, bool)> = vec![(start.to_string(), false)];
        while let Some((node, processed)) = todo.last().cloned() {
            if !processed {
                if visited.contains(&node) || in_path.contains(&node) {
                    todo.pop();
                    continue;
                }
                in_path.insert(node.clone());
                let last = todo.len() - 1;
                todo[last].1 = true;
                if let Some(node_edges) = edges.get(&node) {
                    for e in node_edges.iter().rev() {
                        todo.push((e.clone(), false));
                    }
                }
            } else {
                todo.pop();
                in_path.remove(&node);
                visited.insert(node.clone());
                result.push(node);
            }
        }
    }

    /// All transitive dependencies of `node`
    pub fn dependencies_of(&self, node: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut result = vec![];
        self.dfs(&self.outgoing, node, &mut visited, &mut result);
        result.retain(|n| n != node);
        result
    }

    /// All transitive dependants of `node`
    pub fn dependants_of(&self, node: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut result = vec![];
        self.dfs(&self.incoming, node, &mut visited, &mut result);
        result.retain(|n| n != node);
        result
    }

    pub fn overall_order(&self) -> Vec<String> {
        let mut result = vec![];
        let mut visited = HashSet::new();
        let starts: Vec<&String> = self
            .nodes
            .iter()
            .filter(|n| self.incoming.get(*n).map(|i| i.is_empty()).unwrap_or(true))
            .collect();
        for n in starts {
            self.dfs(&self.outgoing, n, &mut visited, &mut result);
        }
        // circular: visit remaining nodes
        for n in self.nodes.iter() {
            if !visited.contains(n) {
                self.dfs(&self.outgoing, n, &mut visited, &mut result);
            }
        }
        result
    }
}

/// Port of `TemplateDepGraph`
#[derive(Clone, Debug)]
pub struct TemplateDepGraph {
    pub graph: DepGraph,
    config_collection_names: IndexSet<String>,
    tag_consumers: IndexMap<String, Vec<String>>,
    tag_consumer_edges: IndexSet<String>,
}

impl Default for TemplateDepGraph {
    fn default() -> Self {
        Self::new()
    }
}

fn ckey(name: &str) -> String {
    format!("{COLLECTION_PREFIX}{name}")
}

impl TemplateDepGraph {
    pub fn new() -> Self {
        let mut g = TemplateDepGraph {
            graph: DepGraph::default(),
            config_collection_names: IndexSet::new(),
            tag_consumers: IndexMap::new(),
            tag_consumer_edges: IndexSet::new(),
        };
        let mut previous: Option<String> = None;
        for stage in STAGES.iter().rev() {
            let key = ckey(stage);
            if let Some(prev) = &previous {
                g.uses(prev, &key);
            }
            previous = Some(key);
        }
        g
    }

    pub fn uses(&mut self, from: &str, to: &str) {
        self.graph.add_dependency(from, to);
    }

    fn add_tag(&mut self, tag_name: &str, tag_type: &str) {
        if tag_name == "all"
            || (tag_name.starts_with('[') && tag_name.ends_with(']'))
            || self.config_collection_names.contains(tag_name)
        {
            return;
        }
        self.uses(&ckey(&format!("[{tag_type}]")), &ckey(tag_name));
    }

    pub fn add_config_collection_name(&mut self, name: &str) {
        if name == "all" {
            return;
        }
        self.config_collection_names.insert(name.to_string());
    }

    fn cleanup_collection_names(&self, names: &[String]) -> Vec<String> {
        let mut s: IndexSet<String> = names.iter().cloned().collect();
        if s.contains("[userconfig]") {
            return names.to_vec();
        }
        if names.iter().any(|n| self.config_collection_names.contains(n)) {
            s.insert("[userconfig]".into());
        }
        s.into_iter().collect()
    }

    pub fn add_template(&mut self, file_path: &str, consumes: &[String], publishes_to: &[String]) {
        if consumes.is_empty() {
            self.uses(&ckey("[basic]"), file_path);
        }
        let consumes = self.cleanup_collection_names(consumes);
        if !consumes.is_empty() {
            self.tag_consumers.insert(file_path.to_string(), consumes.clone());
        } else {
            self.tag_consumers.shift_remove(file_path);
        }
        let mut publishes = self.cleanup_collection_names(publishes_to);
        let consumes_all = consumes.iter().any(|c| c == "all");
        if consumes_all {
            publishes.retain(|p| p != "all");
        }
        for name in &publishes {
            if !consumes_all {
                let tag_type = if consumes.iter().any(|c| c == "[userconfig]") {
                    "keys"
                } else {
                    "basic"
                };
                self.add_tag(name, tag_type);
            }
            self.uses(&ckey(name), file_path);
        }
        for name in &consumes {
            self.uses(file_path, &ckey(name));
            if let Some(idx) = STAGES.iter().position(|s| s == name) {
                if idx > 0 {
                    if let Some(next) = STAGES.get(idx + 1) {
                        self.uses(&ckey(next), file_path);
                    }
                }
            }
        }
    }

    fn is_basic_tag(&self, tag_name: &str) -> bool {
        if tag_name == "all"
            || (tag_name.starts_with('[') && tag_name.ends_with(']'))
            || self.config_collection_names.contains(tag_name)
        {
            return false;
        }
        let node = ckey(tag_name);
        if !self.graph.has_node(&node) {
            return true;
        }
        let deps = self.graph.dependencies_of(&node);
        !deps.contains(&ckey("[userconfig]")) && !deps.contains(&ckey("[keys]"))
    }

    fn add_tag_consumers_before_config_collections(&mut self) {
        let userconfig_key = ckey("[userconfig]");
        let edges: Vec<String> = self.tag_consumer_edges.iter().cloned().collect();
        for file in edges {
            if self.graph.has_node(&file) {
                self.graph.remove_dependency(&userconfig_key, &file);
            }
        }
        self.tag_consumer_edges.clear();
        let consumers: Vec<(String, Vec<String>)> = self
            .tag_consumers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut basic_cache: HashMap<String, bool> = HashMap::new();
        for (file, consumes) in consumers {
            if !self.graph.has_node(&file) {
                continue;
            }
            let all_basic = consumes.iter().all(|t| {
                if let Some(v) = basic_cache.get(t) {
                    return *v;
                }
                let v = self.is_basic_tag(t);
                basic_cache.insert(t.clone(), v);
                v
            });
            if all_basic {
                self.uses(&userconfig_key, &file);
                self.tag_consumer_edges.insert(file);
            }
        }
    }

    fn remove_basic_edges_from_keys_tags(&mut self) {
        let basic_key = ckey("[basic]");
        let keys_key = ckey("[keys]");
        for tag_node in self.graph.direct_dependencies_of(&keys_key) {
            if !tag_node.starts_with(&format!("{COLLECTION_PREFIX}[")) {
                self.graph.remove_dependency(&basic_key, &tag_node);
            }
        }
    }

    pub fn unfiltered_order(&mut self) -> Vec<String> {
        let names: Vec<String> = self.config_collection_names.iter().cloned().collect();
        for name in names {
            self.uses(&ckey("[keys]"), &ckey(&name));
            self.uses(&ckey(&name), &ckey("[userconfig]"));
        }
        self.remove_basic_edges_from_keys_tags();
        self.add_tag_consumers_before_config_collections();
        self.graph.overall_order()
    }

    pub fn overall_order(&mut self) -> Vec<String> {
        let unfiltered = self.unfiltered_order();
        let keys_key = ckey("[keys]");
        let mut filtered: Vec<String> = unfiltered
            .into_iter()
            .filter(|e| {
                if *e == keys_key {
                    return true;
                }
                !e.starts_with(&format!("{COLLECTION_PREFIX}[")) && !e.ends_with(']')
            })
            .collect();
        let all_key = ckey("all");
        if filtered.last() != Some(&all_key) {
            filtered.push(all_key);
        }
        filtered
    }
}

/// Port of `GlobalDependencyMap` (the subset needed for full builds)
#[derive(Clone, Debug, Default)]
pub struct GlobalDependencyMap {
    pub map: TemplateDepGraph,
    data_files: HashMap<String, IndexSet<String>>,
    pub is_incremental: bool,
}

impl GlobalDependencyMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_collection(entry: &str) -> bool {
        entry.starts_with(COLLECTION_PREFIX)
    }

    pub fn get_tag_name(entry: &str) -> Option<String> {
        entry.strip_prefix(COLLECTION_PREFIX).map(|s| s.to_string())
    }

    pub fn get_collection_key_for_entry(entry: &str) -> String {
        ckey(entry)
    }

    /// `PathNormalizer.fullNormalization`
    pub fn normalize_node(node: &str) -> String {
        let normalized = node.strip_prefix("file://").unwrap_or(node);
        let rel = if normalized.starts_with('/') {
            template_path::relative_path(normalized)
        } else {
            normalized.to_string()
        };
        template_path::strip_leading_dot_slash(&rel.replace('\\', "/"))
    }

    pub fn add_collection_api_names(&mut self, names: &[String]) {
        for name in names {
            self.map.add_config_collection_name(name);
        }
    }

    pub fn add_new_node_relationships(&mut self, from: &str, consumes: &[String], publishes: &[String]) {
        let from = Self::normalize_node(from);
        let consumes: Vec<String> = consumes.iter().filter(|s| !s.is_empty()).cloned().collect();
        let publishes: Vec<String> = publishes.iter().filter(|s| !s.is_empty()).cloned().collect();
        self.map.add_template(&from, &consumes, &publishes);
    }

    pub fn add_dependency(&mut self, from: &str, to: &[String]) {
        let from = Self::normalize_node(from);
        self.map.graph.add_node(&from);
        for t in to {
            let t = Self::normalize_node(t);
            self.map.graph.add_node(&t);
            if from != t {
                self.map.graph.add_dependency(&from, &t);
            }
        }
    }

    pub fn add_data_files(&mut self, template_path: &str, files: &[String]) {
        self.data_files
            .entry(Self::normalize_node(template_path))
            .or_default()
            .extend(files.iter().map(|f| Self::normalize_node(f)));
    }

    pub fn get_dependencies(&self, node: &str) -> Vec<String> {
        let node = Self::normalize_node(node);
        if !self.map.graph.has_node(&node) {
            return vec![];
        }
        self.map
            .graph
            .dependencies_of(&node)
            .into_iter()
            .filter(|n| !Self::is_collection(n))
            .collect()
    }

    /// Is `comparison_file` used by `input_path` (directly or transitively)?
    pub fn is_file_relevant_to(&self, input_path: &str, comparison_file: &str) -> bool {
        let input = Self::normalize_node(input_path);
        let cmp = Self::normalize_node(comparison_file);
        if input == cmp {
            return true;
        }
        if self.data_files.get(&input).map(|s| s.contains(&cmp)).unwrap_or(false) {
            return true;
        }
        if !self.map.graph.has_node(&input) {
            return false;
        }
        self.map.graph.dependencies_of(&input).contains(&cmp)
    }

    /// Templates that use `node` (e.g. an include or layout file)
    pub fn get_dependants_for(&self, node: &str) -> Vec<String> {
        let node = Self::normalize_node(node);
        let mut out: Vec<String> = vec![];
        if self.map.graph.has_node(&node) {
            out.extend(
                self.map
                    .graph
                    .dependants_of(&node)
                    .into_iter()
                    .filter(|n| !Self::is_collection(n)),
            );
        }
        for (template, files) in &self.data_files {
            if files.contains(&node) {
                out.push(template.clone());
            }
        }
        out
    }

    pub fn get_template_order(&mut self) -> Vec<String> {
        self.map.overall_order()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_consumers_after_publishers() {
        let mut g = TemplateDepGraph::new();
        g.add_template("list.md", &["posts".into()], &["all".into()]);
        g.add_template("post1.md", &[], &["all".into(), "posts".into()]);
        g.add_template("post2.md", &[], &["all".into(), "posts".into()]);
        let order = g.overall_order();
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("post1.md") < pos("list.md"));
        assert!(pos("post2.md") < pos("list.md"));
        assert!(pos("__collection:posts") < pos("list.md"));
    }
}
