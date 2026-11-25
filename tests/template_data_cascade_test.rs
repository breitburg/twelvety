//! Port of upstream `test/TemplateTest-DataCascade.js`.
//!
//! The fixtures use JavaScript data files (unsupported in the port), so each test runs
//! against a scratch copy of the fixture with equivalent JSON data files.

mod template_support;

use template_support::fixture_copy;
use twelvety::testing::{TestOptions, get_data};

fn cascade(fixture: &str, sub: &str, template: &str, files: &[(&str, &str)]) -> twelvety::Value {
    let dir = fixture_copy(
        &format!("./test/stubs-data-cascade/{fixture}"),
        &format!("data-cascade-{fixture}"),
        files,
    );
    let input = format!("{dir}/{sub}");
    get_data(
        &format!("{input}{template}"),
        TestOptions::new(&input, "dist"),
    )
    .unwrap()
}

// Prior to and including 0.10.0 this mismatched the documentation)! (Issue #915)
#[test]
fn layout_front_matter_does_not_override_template_data_files() {
    let data = cascade(
        "layout-data-files",
        "",
        "test.njk",
        &[("test.data.json", r#"{"shared": "datafile"}"#)],
    );
    assert_eq!(data.get("shared").as_str(), Some("datafile"));
}

#[test]
fn layout_front_matter_should_not_override_global_data_issue_915() {
    let data = cascade(
        "global-versus-layout",
        "",
        "test.njk",
        &[("_data/cascade.json", r#""from-global-data""#)],
    );
    assert_eq!(data.get("cascade").as_str(), Some("from-layout-file"));
}

#[test]
fn template_data_files_more_specific_than_layout_front_matter_issue_915() {
    let data = cascade(
        "layout-versus-tmpldatafile",
        "",
        "test.njk",
        &[("test.data.json", r#"{"cascade": "template-data-file"}"#)],
    );
    assert_eq!(data.get("cascade").as_str(), Some("template-data-file"));
}

#[test]
fn directory_data_files_more_specific_than_layout_front_matter_issue_915() {
    let data = cascade(
        "layout-versus-dirdatafile",
        "src/",
        "test.njk",
        &[("src/src.data.json", r#"{"cascade": "dir-data-file"}"#)],
    );
    assert_eq!(data.get("cascade").as_str(), Some("dir-data-file"));
}
