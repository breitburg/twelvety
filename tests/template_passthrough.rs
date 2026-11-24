//! Port of upstream `test/TemplatePassthroughTest.js`.

use twelvety::Value;
use twelvety::file_system_search::FileSystemSearch;
use twelvety::template_config::TemplateConfig;
use twelvety::template_passthrough::TemplatePassthrough;
use twelvety::testing::quiet_logger;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};

struct Pass {
    tc: TemplateConfig,
    pass: TemplatePassthrough,
    search: FileSystemSearch,
}

impl Pass {
    fn output_path(&self) -> String {
        self.pass.get_output_path(&self.tc.directories, None)
    }

    fn output_path_for(&self, file: &str) -> String {
        self.pass.get_output_path(&self.tc.directories, Some(file))
    }

    fn files(&self, glob: &str) -> Vec<String> {
        self.pass.get_files(&self.tc.directories, &self.search, glob).unwrap()
    }

    fn file_map(&self) -> Vec<(String, String)> {
        let config = self.tc.get_config();
        self.pass
            .get_file_map(&self.tc.directories, &self.search, &self.tc.exists_cache, &config, &quiet_logger())
            .unwrap()
    }
}

fn get_template_passthrough_with(input_path: &str, output_path: Value, output_dir: &str, input_dir: &str) -> Pass {
    let mut tc = TemplateConfig::new(Some(""));
    tc.set_logger(quiet_logger());
    let mut dirs = ProjectDirectories::new();
    dirs.set_via_config_object(&ConfigDirs {
        input: Some(input_dir.into()),
        output: Some(output_dir.into()),
        ..Default::default()
    })
    .unwrap();
    tc.set_directories(dirs);
    tc.init().unwrap();
    Pass {
        tc,
        pass: TemplatePassthrough::new(input_path, output_path, Value::object()),
        search: FileSystemSearch::new(),
    }
}

/// `{ inputPath: path, outputPath: true }`
fn get_template_passthrough(input_path: &str, output_dir: &str, input_dir: &str) -> Pass {
    get_template_passthrough_with(input_path, Value::Bool(true), output_dir, input_dir)
}

fn mapped(input_path: &str, output_path: &str, output_dir: &str, input_dir: &str) -> Pass {
    get_template_passthrough_with(input_path, Value::str(output_path), output_dir, input_dir)
}

/// "Constructor"
#[test]
fn constructor() {
    let pass = get_template_passthrough("avatar.png", "_site", ".");
    assert_eq!(pass.pass.output_path, Value::Bool(true));
    assert_eq!(pass.output_path(), "_site/avatar.png");
}

/// "Constructor, input directory in inputPath is stripped"
#[test]
fn constructor_input_directory_in_input_path_is_stripped() {
    let pass = get_template_passthrough("src/avatar.png", "_site", "src");
    assert_eq!(pass.pass.output_path, Value::Bool(true));
    assert_eq!(pass.output_path(), "_site/avatar.png");

    let pass2 = mapped("src/avatar.png", "avatar.png", "_site", "src");
    assert_eq!(pass2.pass.output_path, Value::str("avatar.png"));
    assert_eq!(pass2.output_path(), "_site/avatar.png");
}

/// "Constructor, input directory in inputPath is stripped, duplicate directory names"
#[test]
fn constructor_input_directory_in_input_path_is_stripped_duplicate_directory_names() {
    let pass = get_template_passthrough("src/src/avatar.png", "_site", "src");
    assert_eq!(pass.output_path(), "_site/src/avatar.png");

    let pass2 = mapped("src/src/avatar.png", "src/avatar.png", "_site", "src");
    assert_eq!(pass2.pass.output_path, Value::str("src/avatar.png"));
    assert_eq!(pass2.output_path(), "_site/src/avatar.png");
}

/// "Constructor, input directory (object param, directory)"
#[test]
fn constructor_input_directory_object_param_directory() {
    let pass = mapped("src/test", "test", "_site", "src");
    assert_eq!(pass.pass.output_path, Value::str("test"));
    assert_eq!(pass.output_path(), "_site/test");
}

/// "Constructor, input directory, path missing input directory"
#[test]
fn constructor_input_directory_path_missing_input_directory() {
    let pass = get_template_passthrough("avatar.png", "_site", "src");
    assert_eq!(pass.output_path(), "_site/avatar.png");

    let pass2 = mapped("avatar.png", "avatar.png", "_site", "src");
    assert_eq!(pass2.pass.output_path, Value::str("avatar.png"));
    assert_eq!(pass2.output_path(), "_site/avatar.png");
}

// "Constructor not Dry Run" / "Constructor Dry Run": dry run is a `TemplatePassthroughManager`
// property in the port (covered by TemplatePassthroughManagerTest).

/// "Origin path isn’t included in output when targeting a directory"
#[test]
fn origin_path_isnt_included_in_output_when_targeting_a_directory() {
    let pass = get_template_passthrough("img", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/img");
}

/// "Origin path isn’t included in output when targeting a directory several levels deep"
#[test]
fn origin_path_isnt_included_in_output_when_targeting_a_directory_several_levels_deep() {
    let pass = get_template_passthrough("img", "_site", "test/stubs/subdir");
    assert_eq!(pass.output_path(), "_site/img");
}

/// "Target directory’s subdirectory structure is retained"
#[test]
fn target_directorys_subdirectory_structure_is_retained() {
    let pass = get_template_passthrough("subdir/img", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/subdir/img");

    let pass2 = mapped("subdir/img", "subdir/img", "_site", "test/stubs");
    assert_eq!(pass2.output_path(), "_site/subdir/img");
}

/// "Origin path isn’t included in output when targeting a file"
#[test]
fn origin_path_isnt_included_in_output_when_targeting_a_file() {
    let pass = get_template_passthrough("avatar.png", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/avatar.png");
}

/// "Origin path isn’t included in output when targeting a file several levels deep"
#[test]
fn origin_path_isnt_included_in_output_when_targeting_a_file_several_levels_deep() {
    let pass = get_template_passthrough("avatar.png", "_site", "test/stubs/subdir/img");
    assert_eq!(pass.output_path(), "_site/avatar.png");
}

/// "Full input file path and deep input path"
#[test]
fn full_input_file_path_and_deep_input_path() {
    for (input_path, output_dir, input_dir) in [
        ("test/views/avatar.png", "_site", "test/views/"),
        ("test/views/avatar.png", "_site", "test/views"),
        ("test/views/avatar.png", "_site/", "test/views"),
        ("test/views/avatar.png", "./_site", "./test/views"),
        ("./test/views/avatar.png", "./_site/", "./test/views/"),
        ("./test/views/avatar.png", "_site", "test/views/"),
    ] {
        let tp = get_template_passthrough(input_path, output_dir, input_dir);
        assert_eq!(tp.output_path(), "_site/avatar.png", "{input_path} {output_dir} {input_dir}");
    }
}

/// ".htaccess"
#[test]
fn htaccess() {
    let pass = get_template_passthrough(".htaccess", "_site", ".");
    assert_eq!(pass.output_path(), "_site/.htaccess");
}

/// ".htaccess with input dir"
#[test]
fn htaccess_with_input_dir() {
    let pass = get_template_passthrough(".htaccess", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/.htaccess");
}

/// "getFiles where not glob and file does not exist"
#[test]
fn get_files_where_not_glob_and_file_does_not_exist() {
    let pass = get_template_passthrough(".htaccess", "_site", "test/stubs");
    assert_eq!(pass.files(".htaccess"), Vec::<String>::new());
}

/// "getFiles where not glob and directory does not exist"
#[test]
fn get_files_where_not_glob_and_directory_does_not_exist() {
    let input_path = "./test/stubs/template-passthrough/static/not-exists/";
    let pass = get_template_passthrough(input_path, "_site", "test/stubs");
    assert_eq!(pass.files(input_path), Vec::<String>::new());
}

/// "getFileMap returns [] for a missing non-glob source (no glob fall-through, no flatten)"
#[test]
fn get_file_map_returns_empty_for_a_missing_non_glob_source() {
    let pass = get_template_passthrough("./test/stubs/template-passthrough/static/not-exists/", "_site", "test/stubs");
    assert_eq!(pass.file_map(), vec![]);
}

/// "getFileMap returns [] when a glob source matches nothing"
#[test]
fn get_file_map_returns_empty_when_a_glob_source_matches_nothing() {
    let pass = get_template_passthrough("./test/stubs/template-passthrough/static/not-exists/**", "_site", "test/stubs");
    assert_eq!(pass.file_map(), vec![]);
}

/// "getFiles with glob"
#[test]
fn get_files_with_glob() {
    let input_path = "./test/stubs/template-passthrough/static/**";
    let pass = get_template_passthrough(input_path, "_site", "test/views");
    let mut files = pass.files(input_path);
    files.sort();
    let mut expected = vec![
        "./test/stubs/template-passthrough/static/test.css".to_string(),
        "./test/stubs/template-passthrough/static/test.js".to_string(),
        "./test/stubs/template-passthrough/static/nested/test-nested.css".to_string(),
    ];
    expected.sort();
    assert_eq!(files, expected);

    let find = |suffix: &str| files.iter().find(|f| f.ends_with(suffix)).unwrap().clone();
    assert_eq!(pass.output_path_for(&find("test.css")), "_site/test/stubs/template-passthrough/static/test.css");
    assert_eq!(pass.output_path_for(&find("test.js")), "_site/test/stubs/template-passthrough/static/test.js");
    assert_eq!(
        pass.output_path_for(&find("test-nested.css")),
        "_site/test/stubs/template-passthrough/static/nested/test-nested.css"
    );
}

/// "getFiles with glob 2"
#[test]
fn get_files_with_glob_2() {
    let input_path = "./test/stubs/template-passthrough/static/**/*.js";
    let pass = get_template_passthrough(input_path, "_site", "test/views");
    let files = pass.files(input_path);
    assert_eq!(files, vec!["./test/stubs/template-passthrough/static/test.js".to_string()]);
    assert_eq!(pass.output_path_for(&files[0]), "_site/test/stubs/template-passthrough/static/test.js");
}

/// "Directory where outputPath is true"
#[test]
fn directory_where_output_path_is_true() {
    let pass = get_template_passthrough("./static", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/static");
}

/// "Nested directory where outputPath is remapped"
#[test]
fn nested_directory_where_output_path_is_remapped() {
    let pass = mapped("./static/nested", "./test", "_site", "test/stubs");
    assert_eq!(pass.pass.output_path, Value::str("./test"));
    assert_eq!(pass.output_path(), "_site/test");
}

/// "Glob pattern"
#[test]
fn glob_pattern() {
    let pass = mapped("./test/stubs/template-passthrough/static/*.js", "./directory/", "_site", "test/stubs");
    assert_eq!(pass.pass.output_path, Value::str("./directory/"));
    assert_eq!(pass.output_path_for("./test/stubs/template-passthrough/static/test.js"), "_site/directory/test.js");
}

/// "Output paths match with different templatePassthrough methods"
#[test]
fn output_paths_match_with_different_template_passthrough_methods() {
    let pass1 = mapped("./static/nested", "./test", "_site", "test/stubs");
    let pass2 = get_template_passthrough("avatar.png", "_site/test", ".");
    assert_eq!(
        pass1.pass.get_output_path_for_glob_file(&pass1.tc.directories, "avatar.png"),
        pass2.output_path()
    );
}

/// "File renamed"
#[test]
fn file_renamed() {
    let pass = mapped("./test/stubs/template-passthrough/static/test.js", "./rename.js", "_site", "test/stubs");
    assert_eq!(pass.output_path(), "_site/rename.js");
}

/// "Bug with incremental file copying to a directory output, issue #2278 #1038"
#[test]
fn bug_with_incremental_file_copying_to_a_directory_output_issue_2278_1038() {
    let pass = mapped("./test/stubs/public/test.css", "/", "test/stubs", ".");
    assert_eq!(pass.output_path(), "test/stubs/test.css");
}

/// "Bug with incremental dir copying to a directory output, issue #2278 #1038"
#[test]
fn bug_with_incremental_dir_copying_to_a_directory_output_issue_2278_1038() {
    let pass = mapped("./test/stubs/public/", "/", "test/stubs", ".");
    assert_eq!(pass.output_path(), "./test/stubs/");
}
