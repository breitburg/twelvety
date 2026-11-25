//! Ported from upstream `test/TemplateRenderTest.js`.

use twelvety::template_pipeline as pipeline;
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context};

fn get_new_template_render(name: &str, input_dir: Option<&str>) -> anyhow::Result<TemplateRender> {
    let options = TestOptions {
        input: input_dir.map(String::from),
        ..Default::default()
    };
    TemplateRender::new(get_context(options)?, name)
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn basic() {
    assert!(get_new_template_render("sldkjfkldsj", None).is_err());
}

#[test]
fn includes_dir() {
    let tr = get_new_template_render("liquid", Some("./test/stubs")).unwrap();
    assert_eq!(tr.get_includes_dir(), "./test/stubs/_includes/");
}

#[test]
fn invalid_override() {
    let mut tr = get_new_template_render("liquid", Some("./test/stubs")).unwrap();
    assert!(tr.set_engine_override("lslkdjf").is_err());
}

#[test]
fn valid_override() {
    let mut tr = get_new_template_render("liquid", Some("./test/stubs")).unwrap();
    tr.set_engine_override("njk").unwrap();
    assert_eq!(tr.get_engine_name(), "njk");
    assert!(tr.is_engine("njk"));
}

#[test]
fn parse_template_syntax_into_pipeline_steps_execution_order() {
    // `null`, `undefined` and `false` all map to the empty string in the port
    assert_eq!(pipeline::parse("").unwrap(), Vec::<String>::new());
    assert_eq!(pipeline::parse("html").unwrap(), Vec::<String>::new());
    assert_eq!(pipeline::parse("html,html").unwrap(), Vec::<String>::new());
    assert_eq!(pipeline::parse("html,md,md").unwrap(), strings(&["md"]));
    assert_eq!(
        pipeline::parse("liquid,md").unwrap(),
        strings(&["liquid", "md"])
    );
    assert_eq!(
        pipeline::parse("md,liquid").unwrap(),
        strings(&["liquid", "md"])
    );
    assert_eq!(pipeline::parse("liquid").unwrap(), strings(&["liquid"]));
    assert_eq!(pipeline::parse("njk").unwrap(), strings(&["njk"]));
    assert_eq!(
        pipeline::parse("liquid,html").unwrap(),
        strings(&["liquid"])
    );
    assert_eq!(
        pipeline::parse("liquid,md,html").unwrap(),
        strings(&["liquid", "md"])
    );
    assert_eq!(pipeline::parse("njk,njk").unwrap(), strings(&["njk"]));

    assert!(pipeline::parse("njk,liquid").is_err());
    assert!(pipeline::parse("liquid,njk,html").is_err());
}

#[test]
fn validate_pipeline_steps_against_the_allow_list() {
    assert!(pipeline::validate(&strings(&["md"])).is_ok());
    assert!(pipeline::validate(&strings(&["liquid", "md"])).is_ok());
    assert!(pipeline::validate(&strings(&["liquid", "html"])).is_ok());
    assert!(pipeline::validate(&strings(&["njk"])).is_ok());

    let err = |steps: &[&str]| pipeline::validate(&strings(steps)).unwrap_err().to_string();
    assert!(err(&["md", "html"]).contains("Unsupported template pipeline"));
    assert!(err(&["md", "liquid"]).contains("Unsupported template pipeline"));
    assert!(err(&["liquid", "njk", "md"]).contains("Only one template engine"));
}

#[test]
fn preprocessing_a_format_with_another_format_throws() {
    let ctx = get_context(TestOptions::default().with_config(|cfg| {
        cfg.set_html_template_engine("md");
        Ok(())
    }))
    .unwrap();
    let err = TemplateRender::new(ctx, "html")
        .err()
        .expect("init should fail");
    assert_eq!(
        err.to_string(),
        "The `htmlTemplateEngine` configuration property must be a template engine, received the output format: md"
    );
}

#[test]
fn make_sure_get_engines_list_returns_a_string() {
    let tr = get_new_template_render("liquid", Some("./test/stubs")).unwrap();
    assert_eq!(tr.get_engines_list(Some("njk,md")).unwrap(), "njk,md");
}
