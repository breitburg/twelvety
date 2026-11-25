//! Ported from upstream `test/TemplateRenderHTMLTest.js`.

use twelvety::Value;
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context, json};

fn get_new_template_render(name: &str) -> TemplateRender {
    TemplateRender::new(get_context(TestOptions::default()).unwrap(), name).unwrap()
}

#[test]
fn html() {
    let tr = get_new_template_render("html");
    assert_eq!(tr.get_engine_name(), "html");
}

#[test]
fn html_render() {
    let tr = get_new_template_render("html");
    let f = tr
        .get_compiled_template("<p>Paragraph</p>", false, None)
        .unwrap();
    assert_eq!(f(&Value::Undefined).unwrap(), "<p>Paragraph</p>");
    assert_eq!(f(&Value::object()).unwrap(), "<p>Paragraph</p>");
}

#[test]
fn html_render_parses_html_using_liquid_engine_default_with_data() {
    let tr = get_new_template_render("html");
    let f = tr
        .get_compiled_template("<h1>{{title}}</h1>", false, None)
        .unwrap();
    assert_eq!(
        f(&json(r#"{"title": "My Title"}"#)).unwrap().trim(),
        "<h1>My Title</h1>"
    );
}

#[test]
fn html_render_set_html_engine_to_false_dont_parse() {
    let mut tr = get_new_template_render("html");
    tr.set_preprocessing_engine(None).unwrap();

    let f = tr
        .get_compiled_template("<h1>{{title}}</h1>", false, None)
        .unwrap();
    assert_eq!(f(&Value::Undefined).unwrap().trim(), "<h1>{{title}}</h1>");
}

#[test]
fn html_render_pass_in_an_override_liquid() {
    let mut tr = get_new_template_render("html");
    tr.set_preprocessing_engine(Some("liquid".into())).unwrap();
    let f = tr
        .get_compiled_template("<h1>{{title}}</h1>", false, None)
        .unwrap();
    assert_eq!(
        f(&json(r#"{"title": "My Title"}"#)).unwrap().trim(),
        "<h1>My Title</h1>"
    );
}
