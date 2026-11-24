//! Ported from upstream `test/TemplateDepGraphTest.js`.

use twelvety::global_dependency_map::TemplateDepGraph;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn using_new_template_dep_graph() {
    let mut graph = TemplateDepGraph::new();

    graph.add_template("template-paginated-over-all.njk", &strings(&["all"]), &[]);
    graph.add_template("template-paginated-over-userconfig.njk", &strings(&["[userconfig]"]), &[]);
    graph.add_template("template-1.njk", &[], &strings(&["all", "posts"]));
    graph.add_template("template-2.njk", &[], &strings(&["all", "posts", "dog"]));
    graph.add_template("template-paginated-collections.njk", &strings(&["[keys]"]), &[]);
    graph.add_config_collection_name("myCollection");

    assert_eq!(
        graph.unfiltered_order(),
        strings(&[
            "template-1.njk",
            "template-2.njk",
            "__collection:posts",
            "__collection:dog",
            "__collection:[basic]",
            "__collection:[userconfig]",
            "template-paginated-over-userconfig.njk",
            "__collection:myCollection",
            "__collection:[keys]",
            "template-paginated-collections.njk",
            "__collection:all",
            "template-paginated-over-all.njk",
        ])
    );

    assert_eq!(
        graph.overall_order(),
        strings(&[
            "template-1.njk",
            "template-2.njk",
            "__collection:posts",
            "__collection:dog",
            "template-paginated-over-userconfig.njk",
            "__collection:myCollection",
            "__collection:[keys]",
            "template-paginated-collections.njk",
            "__collection:all",
            "template-paginated-over-all.njk",
            "__collection:all",
        ])
    );
}
