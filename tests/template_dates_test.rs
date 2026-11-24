//! Port of upstream `test/TemplateTest-Dates.js`.

mod template_support;

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use template_support::{rendered_data, virtual_options, templates};
use twelvety::template::Template;
use twelvety::testing::{TestOptions, get_new_template};

fn virtual_dates(path: &str) -> &'static str {
    match path {
        "dates/file2.md" => "---\ntags: dateTestTag\ndate: \"2016-01-01\"\n---\n",
        "dates/file2b.md" => "---\ntags: dateTestTag\ndate: 2016-01-01\n---\n",
        "dates/2018-01-01-file5.md" => "---\ntags: dateTestTag\n---\n",
        "dates/2019-01-01-folder/2020-01-01-file.md" => "---\n---\n",
        _ => unreachable!(),
    }
}

fn virtual_template(path: &str) -> Arc<Template> {
    get_new_template(
        &format!("./test/stubs/{path}"),
        virtual_options("./test/stubs/", "./dist", templates(&[(path, virtual_dates(path))]), |_| Ok(())),
    )
    .unwrap()
}

fn file_template(path: &str) -> Arc<Template> {
    get_new_template(path, TestOptions::new("./test/stubs/", "./dist")).unwrap()
}

fn mapped_date(tmpl: &Arc<Template>) -> DateTime<Utc> {
    let data = rendered_data(tmpl, 0);
    let date = tmpl.get_mapped_date(&data.get("date"), &data.get("page")).unwrap().unwrap();
    assert!(date.timestamp_millis() != 0);
    date
}

fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
}

#[test]
fn get_mapped_date_empty_assume_created() {
    mapped_date(&file_template("./test/stubs/dates/file1.md"));
}

#[test]
fn get_mapped_date_explicit_date_yaml_string() {
    assert_eq!(mapped_date(&virtual_template("dates/file2.md")), utc(2016, 1, 1));
}

#[test]
fn get_mapped_date_explicit_date_yaml_date() {
    assert_eq!(mapped_date(&virtual_template("dates/file2b.md")), utc(2016, 1, 1));
}

#[test]
fn get_mapped_date_yaml_date_and_string_should_be_the_same() {
    assert_eq!(
        mapped_date(&virtual_template("dates/file2.md")),
        mapped_date(&virtual_template("dates/file2b.md"))
    );
}

#[test]
fn get_mapped_date_modified_date() {
    mapped_date(&file_template("./test/stubs/dates/file3.md"));
}

#[test]
fn get_mapped_date_created_date() {
    mapped_date(&file_template("./test/stubs/dates/file4.md"));
}

#[test]
fn get_mapped_date_falls_back_to_filename_date() {
    assert_eq!(mapped_date(&virtual_template("dates/2018-01-01-file5.md")), utc(2018, 1, 1));
}

#[test]
fn get_mapped_date_found_multiple_dates_picks_first() {
    assert_eq!(
        mapped_date(&virtual_template("dates/2019-01-01-folder/2020-01-01-file.md")),
        utc(2019, 1, 1)
    );
}
