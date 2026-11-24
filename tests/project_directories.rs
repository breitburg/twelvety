//! Port of upstream `test/ProjectDirectoriesTest.js`.

use twelvety::util::exists_cache::ExistsCache;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn input(d: &mut ProjectDirectories, dir: &str) {
    d.set_input(Some(dir), None).unwrap();
}

fn via_config(d: &mut ProjectDirectories, c: ConfigDirs) {
    d.set_via_config_object(&c).unwrap();
}

/// "Implied input"
#[test]
fn implied_input() {
    let d = ProjectDirectories::new();
    assert_eq!(d.input(), "./");
    assert_eq!(d.input_file, None);
}

/// "Input matches"
#[test]
fn input_matches() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/");
    assert_eq!(d.input(), "./test/");
    assert_eq!(d.input_file, None);
}

/// "Normalized input (has trailing slash)"
#[test]
fn normalized_input_has_trailing_slash() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "test/");
    assert_eq!(d.input(), "./test/");
    assert_eq!(d.input_file, None);
}

/// "Normalized input (no trailing slash)"
#[test]
fn normalized_input_no_trailing_slash() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "test");
    assert_eq!(d.input(), "./test/");
    assert_eq!(d.input_file, None);
}

/// "Input must exist"
#[test]
fn input_must_exist() {
    let mut d = ProjectDirectories::new();
    assert!(d.set_input(Some("does-not-exist"), None).is_err());
}

/// "Input as file"
#[test]
fn input_as_file() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "test/stubs/index.html");
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.input_file, s("./test/stubs/index.html"));
}

/// "Input as file (deep)"
#[test]
fn input_as_file_deep() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "test/stubs/img/stub.md");
    assert_eq!(d.input(), "./test/stubs/img/");
    assert_eq!(d.input_file, s("./test/stubs/img/stub.md"));
}

/// "Input as file (deep with inputDir)"
#[test]
fn input_as_file_deep_with_input_dir() {
    let mut d = ProjectDirectories::new();
    d.set_input(Some("test/stubs/img/stub.md"), Some("test/stubs")).unwrap();
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.input_file, s("./test/stubs/img/stub.md"));
}

/// "Data directory, implied input and default data"
#[test]
fn data_directory_implied_input_and_default_data() {
    let d = ProjectDirectories::new();
    assert_eq!(d.data(), "./_data/");
}

/// "Data directory matches, explicit input"
#[test]
fn data_directory_matches_explicit_input() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/");
    assert_eq!(d.data(), "./test/_data/");
}

/// "Data directory matches, explicit input and data"
#[test]
fn data_directory_matches_explicit_input_and_data() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/");
    d.set_data(Some("mydata"));
    assert_eq!(d.data(), "./test/mydata/");
}

/// "Data directory matches, explicit input and data (order reversed)"
#[test]
fn data_directory_matches_explicit_input_and_data_order_reversed() {
    let mut d = ProjectDirectories::new();
    d.set_data(Some("mydata"));
    input(&mut d, "./test/");
    assert_eq!(d.data(), "./test/mydata/");
}

/// "includes implied, layouts are not"
#[test]
fn includes_implied_layouts_are_not() {
    let d = ProjectDirectories::new();
    assert_eq!(d.layouts(), None);
    assert_eq!(d.includes(), "./_includes/");
}

/// "Layouts/includes, explicit"
#[test]
fn layouts_includes_explicit() {
    let mut d = ProjectDirectories::new();
    d.set_layouts(Some("layouts"));
    d.set_includes(Some("includes"));
    assert_eq!(d.layouts(), s("./layouts/"));
    assert_eq!(d.includes(), "./includes/");

    input(&mut d, "test");
    assert_eq!(d.layouts(), s("./test/layouts/"));
    assert_eq!(d.includes(), "./test/includes/");

    d.set_layouts(Some("../layouts"));
    d.set_includes(Some("../includes"));
    assert_eq!(d.layouts(), s("./layouts/"));
    assert_eq!(d.includes(), "./includes/");
}

/// "Output, implied"
#[test]
fn output_implied() {
    let d = ProjectDirectories::new();
    assert_eq!(d.output(), "./_site/");
}

/// "Content/template/input paths"
#[test]
fn content_template_input_paths() {
    let mut d = ProjectDirectories::new();
    assert_eq!(d.get_input_path("test.md"), "./test.md");
    assert_eq!(d.get_input_path("./test.md"), "./test.md");
    assert_eq!(d.get_layout_path("./layout.html"), "./_includes/layout.html");

    input(&mut d, "test");
    assert_eq!(d.get_input_path("test.md"), "./test/test.md");
    assert_eq!(d.get_input_path("./test.md"), "./test/test.md");
    assert_eq!(d.get_layout_path("./layout.html"), "./test/_includes/layout.html");
}

/// "Project file paths"
#[test]
fn project_file_paths() {
    let d = ProjectDirectories::new();
    assert_eq!(d.get_project_path("eleventy.config.js"), "./eleventy.config.js");
    assert_eq!(d.get_project_path("./eleventy.config.js"), "./eleventy.config.js");
}

/// "Input could be a glob!"
#[test]
fn input_could_be_a_glob() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/*.md");
    assert_eq!(d.input(), "./test/");
    assert_eq!(d.input_file, None);
}

/// "Setting values via config object"
#[test]
fn setting_values_via_config_object() {
    let mut d = ProjectDirectories::new();
    via_config(&mut d, ConfigDirs { input: s("test/stubs"), output: s("dist"), ..Default::default() });
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.input_file, None);
    assert_eq!(d.output(), "./dist/");
    assert_eq!(d.data(), "./test/stubs/_data/");
    assert_eq!(d.includes(), "./test/stubs/_includes/");
    assert_eq!(d.layouts(), None);
}

/// "Setting values via config object (input relative dirs)"
#[test]
fn setting_values_via_config_object_input_relative_dirs() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            data: s("globaldata"),
            layouts: s("mylayouts"),
            includes: s("components"),
            ..Default::default()
        },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.input_file, None);
    assert_eq!(d.output(), "./_site/");
    assert_eq!(d.data(), "./test/stubs/globaldata/");
    assert_eq!(d.includes(), "./test/stubs/components/");
    assert_eq!(d.layouts(), s("./test/stubs/mylayouts/"));
}

/// "Setting values via config object (input relative dirs, parent dirs)"
#[test]
fn setting_values_via_config_object_input_relative_dirs_parent_dirs() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            data: s("../globaldata"),
            includes: s("../components"),
            ..Default::default()
        },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./_site/");
    assert_eq!(d.data(), "./test/globaldata/");
    assert_eq!(d.includes(), "./test/components/");
    assert_eq!(d.layouts(), None);
}

/// "Setting values via config object (eleventy-base-blog example)"
#[test]
fn setting_values_via_config_object_eleventy_base_blog_example() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("src"),
            includes: s("../_includes"),
            data: s("../_data"),
            output: s("_site"),
            ..Default::default()
        },
    );
    assert_eq!(d.input(), "./src/");
    assert_eq!(d.input_file, None);
    assert_eq!(d.output(), "./_site/");
    assert_eq!(d.data(), "./_data/");
    assert_eq!(d.includes(), "./_includes/");
    assert_eq!(d.layouts(), None);
}

/// "Setting values via config object (empty string/false value)"
#[test]
fn setting_values_via_config_object_empty_string_false_value() {
    let mut d = ProjectDirectories::new();
    // `data: false` is represented as an empty string
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            output: s("_site"),
            data: s(""),
            includes: s(""),
            ..Default::default()
        },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./_site/");
    assert_eq!(d.data(), "./test/stubs/");
    assert_eq!(d.includes(), "./test/stubs/");
    assert_eq!(d.layouts(), None);
}

/// "Setting values via config object (layouts is set but falsy)"
#[test]
fn setting_values_via_config_object_layouts_is_set_but_falsy() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs { input: s("test/stubs"), output: s("_site"), layouts: s(""), ..Default::default() },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./_site/");
    assert_eq!(d.data(), "./test/stubs/_data/");
    assert_eq!(d.includes(), "./test/stubs/_includes/");
    assert_eq!(d.layouts(), s("./test/stubs/"));
}

/// "Setting values via config object (output is falsy)"
#[test]
fn setting_values_via_config_object_output_is_falsy() {
    let mut d = ProjectDirectories::new();
    via_config(&mut d, ConfigDirs { input: s("test/stubs"), output: s(""), ..Default::default() });
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./");
    assert_eq!(d.data(), "./test/stubs/_data/");
    assert_eq!(d.includes(), "./test/stubs/_includes/");
    assert_eq!(d.layouts(), None);
}

/// "Setting values via config object (dots)"
#[test]
fn setting_values_via_config_object_dots() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            output: s("."),
            includes: s("."),
            layouts: s("."),
            data: s("."),
        },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./");
    assert_eq!(d.data(), "./test/stubs/");
    assert_eq!(d.includes(), "./test/stubs/");
}

/// "CLI values should override all others (both)"
#[test]
fn cli_values_should_override_all_others_both() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "src");
    d.set_output(Some("dist"));
    d.freeze();
    via_config(&mut d, ConfigDirs { input: s("test/stubs"), includes: s("myincludes"), ..Default::default() });
    assert_eq!(d.input(), "./src/");
    assert_eq!(d.output(), "./dist/");
    assert_eq!(d.data(), "./src/_data/");
    assert_eq!(d.includes(), "./src/myincludes/");
    assert_eq!(d.layouts(), None);
}

/// "CLI values should override all others (just input)"
#[test]
fn cli_values_should_override_all_others_just_input() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "src");
    d.freeze();
    via_config(
        &mut d,
        ConfigDirs { input: s("test/stubs"), includes: s("myincludes"), output: s("dist"), ..Default::default() },
    );
    assert_eq!(d.input(), "./src/");
    assert_eq!(d.output(), "./dist/");
    assert_eq!(d.data(), "./src/_data/");
    assert_eq!(d.includes(), "./src/myincludes/");
    assert_eq!(d.layouts(), None);
}

/// "CLI values should override all others (just output)"
#[test]
fn cli_values_should_override_all_others_just_output() {
    let mut d = ProjectDirectories::new();
    d.set_output(Some("dist"));
    d.freeze();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            includes: s("myincludes"),
            output: s("someotherdir"),
            ..Default::default()
        },
    );
    assert_eq!(d.input(), "./test/stubs/");
    assert_eq!(d.output(), "./dist/");
    assert_eq!(d.data(), "./test/stubs/_data/");
    assert_eq!(d.includes(), "./test/stubs/myincludes/");
    assert_eq!(d.layouts(), None);
}

/// "getLayoutPath (layouts dir)"
#[test]
fn get_layout_path_layouts_dir() {
    let mut d = ProjectDirectories::new();
    via_config(
        &mut d,
        ConfigDirs {
            input: s("test/stubs"),
            layouts: s("mylayouts"),
            includes: s("components"),
            ..Default::default()
        },
    );
    assert_eq!(d.get_layout_path("layout.html"), "./test/stubs/mylayouts/layout.html");
    assert_eq!(d.get_layout_path_relative_to_input_directory("layout.html"), "mylayouts/layout.html");
}

/// "getLayoutPath (includes dir)"
#[test]
fn get_layout_path_includes_dir() {
    let mut d = ProjectDirectories::new();
    via_config(&mut d, ConfigDirs { input: s("test/stubs"), includes: s("components"), ..Default::default() });
    assert_eq!(d.get_layout_path("layout.html"), "./test/stubs/components/layout.html");
    assert_eq!(d.get_layout_path_relative_to_input_directory("layout.html"), "components/layout.html");
}

/// "isFileIn*Folder"
#[test]
fn is_file_in_folder() {
    let d = ProjectDirectories::new();
    assert!(d.is_file_in_project_folder("test.njk"));
    assert!(!d.is_file_in_project_folder("../test.njk"));
    assert!(!d.is_file_in_output_folder("test.njk"));
    assert!(!d.is_file_in_output_folder("../test.njk"));
    assert!(!d.is_file_in_output_folder("../_site/test.html"));
    assert!(d.is_file_in_output_folder("_site/test.html"));
}

/// "isFileInOutputFolder (change output folder)"
#[test]
fn is_file_in_output_folder_change_output_folder() {
    let mut d = ProjectDirectories::new();
    d.set_output(Some("yolo"));
    assert!(!d.is_file_in_output_folder("test.njk"));
    assert!(!d.is_file_in_output_folder("../test.njk"));
    assert!(!d.is_file_in_output_folder("_site/test.html"));
    assert!(!d.is_file_in_output_folder("../_site/test.html"));
    assert!(d.is_file_in_output_folder("yolo/test.html"));
}

/// "isTemplateFile with project root relative paths"
#[test]
fn is_template_file_with_project_root_relative_paths() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "test/stubs");
    let cwd = std::env::current_dir().unwrap();
    assert!(d.is_template_file("./test/stubs/index.njk"));
    assert!(d.is_template_file("test/stubs/sub/index.njk"));
    assert!(d.is_template_file(&format!("{}/test/stubs/index.njk", cwd.display())));
    assert!(!d.is_template_file("./test/stubs/_includes/base.njk"));
    assert!(!d.is_template_file("./test/stubs/_data/site.json"));
    assert!(!d.is_template_file("./test/stubs-other/index.njk"));
    assert!(!d.is_template_file("./index.njk"));
}

/// "isTemplateFile with the default input directory"
#[test]
fn is_template_file_with_the_default_input_directory() {
    let d = ProjectDirectories::new();
    assert!(d.is_template_file("index.njk"));
    assert!(d.is_template_file("./sub/index.njk"));
    assert!(!d.is_template_file("./_includes/base.njk"));
    assert!(!d.is_template_file("./_data/site.json"));
    assert!(!d.is_template_file("../index.njk"));
}

/// "Configured directories must match the casing on disk #4224"
#[test]
fn configured_directories_must_match_the_casing_on_disk_4224() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/_issues/4224/");
    d.check_case(&ExistsCache::new()).unwrap();

    d.set_includes(Some("_Includes"));
    let err = d.check_case(&ExistsCache::new()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "The `includes` path \"./test/_issues/4224/_Includes/\" doesn’t match the casing on disk: \"./test/_issues/4224/_includes/\""
    );

    // Linux fails earlier because the folder doesn’t exist
    let mut other = ProjectDirectories::new();
    let result = other
        .set_input(Some("./test/_issues/4224/Sub/"), None)
        .and_then(|_| other.check_case(&ExistsCache::new()));
    assert!(result.unwrap_err().to_string().contains("`input`"));
}

/// "Input-relative directories configured as project-root relative throw"
#[test]
fn input_relative_directories_configured_as_project_root_relative_throw() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/");
    d.check_input_relative(&ExistsCache::new()).unwrap();

    d.set_includes(Some("stubs"));
    d.set_layouts(Some("stubs"));
    d.check_input_relative(&ExistsCache::new()).unwrap();

    d.set_includes(Some("test/stubs"));
    assert_eq!(
        d.check_input_relative(&ExistsCache::new()).unwrap_err().to_string(),
        "The `includes` directory is relative to the input directory: \"test/stubs\" resolves to \"./test/test/stubs/\", which doesn’t exist. Did you mean \"./stubs/\" (resolves to \"./test/stubs/\")?"
    );

    d.set_includes(Some("stubs"));
    d.set_layouts(Some("./test/stubs/"));
    assert!(d.check_input_relative(&ExistsCache::new()).unwrap_err().to_string().contains("`layouts`"));

    d.set_layouts(Some("stubs"));
    d.set_data(Some("test/stubs"));
    assert!(d.check_input_relative(&ExistsCache::new()).unwrap_err().to_string().contains("`data`"));

    // Missing in both places is not an error
    d.set_data(Some("does-not-exist"));
    d.check_input_relative(&ExistsCache::new()).unwrap();
}

/// "Directories relative to the project root"
#[test]
fn directories_relative_to_the_project_root() {
    let mut d = ProjectDirectories::new();
    input(&mut d, "./test/");
    d.set_includes(Some("test/stubs"));
    assert_eq!(d.includes(), "./test/test/stubs/");

    d.set_relative_to_project_root(true);
    assert_eq!(d.includes(), "./test/stubs/");

    d.set_layouts(Some("test/stubs/_includes"));
    d.set_data(Some(""));
    assert_eq!(d.layouts(), s("./test/stubs/_includes/"));
    assert_eq!(d.data(), "./");
    d.check_input_relative(&ExistsCache::new()).unwrap();

    let mut defaults = ProjectDirectories::new();
    input(&mut defaults, "./test/stubs/");
    defaults.set_relative_to_project_root(true);
    assert_eq!(defaults.includes(), "./test/stubs/_includes/");
    assert_eq!(defaults.data(), "./test/stubs/_data/");
    assert_eq!(defaults.layouts(), None);

    defaults.set_includes(Some("test/stubs/_includes"));
    input(&mut defaults, "./test/noop/");
    assert_eq!(defaults.includes(), "./test/stubs/_includes/");
    assert_eq!(defaults.data(), "./test/noop/_data/");
}
