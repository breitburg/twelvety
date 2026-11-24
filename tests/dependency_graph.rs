//! Ported from upstream `test/DependencyGraphTest.js` (the `dependency-graph` package’s `DepGraph`).

use twelvety::global_dependency_map::DepGraph;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn nodes_dont_require_dependencies() {
    let mut graph = DepGraph::default();
    graph.add_node("all");
    graph.add_node("template-a");
    graph.add_node("template-b");
    graph.add_node("template-c");

    let order = graph.overall_order();
    for node in ["all", "template-a", "template-b", "template-c"] {
        assert!(order.iter().any(|n| n == node));
    }
    // in order of addNode
    assert_eq!(graph.overall_order(), strings(&["all", "template-a", "template-b", "template-c"]));
}

#[test]
fn relationships() {
    let mut graph = DepGraph::default();
    graph.add_node("all");
    graph.add_node("template-a");
    graph.add_node("template-b");
    graph.add_node("template-c");
    graph.add_node("userCollection");

    graph.add_dependency("all", "template-a");
    graph.add_dependency("all", "template-b");
    graph.add_dependency("all", "template-c");
    graph.add_dependency("userCollection", "all");

    assert_eq!(
        graph.overall_order(),
        strings(&["template-a", "template-b", "template-c", "all", "userCollection"])
    );
}

#[test]
fn edges_are_removed_when_nodes_are_deleted() {
    let mut graph = DepGraph::default();
    graph.add_node("template-a");
    graph.add_node("template-b");
    graph.add_dependency("template-a", "template-b");
    assert_eq!(graph.overall_order(), strings(&["template-b", "template-a"]));

    assert_eq!(graph.dependencies_of("template-a"), strings(&["template-b"]));

    graph.remove_node("template-b");
    assert!(graph.dependencies_of("template-a").is_empty());

    graph.add_node("template-b");
    assert!(graph.dependencies_of("template-a").is_empty());

    assert_eq!(graph.overall_order(), strings(&["template-a", "template-b"]));
}
