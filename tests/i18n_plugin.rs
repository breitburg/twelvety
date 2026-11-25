//! Port of upstream `test/I18nPluginTest.js`.

mod common;

use twelvety::Value;
use twelvety::plugins::i18n::{
    get_language_code_from_input_path, get_language_code_from_url, is_lang_code, swap_language_code,
};
use twelvety::template::PageResult;
use twelvety::testing::json;
use twelvety::user_config::UserConfig;

use common::{options, to_json};

fn add_i18n(cfg: &mut UserConfig, plugin_options: &str) -> anyhow::Result<()> {
    cfg.add_plugin("I18nPlugin", twelvety::plugins::i18n::plugin, json(plugin_options))
}

/// Build `./test/stubs-empty/` with programmatic configuration
fn build_empty(config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static) -> anyhow::Result<Vec<PageResult>> {
    to_json("./test/stubs-empty/", Some("./test/stubs-empty/_site"), options(config))
}

fn content_by_url(results: &[PageResult], url: &str) -> String {
    results
        .iter()
        .find(|r| r.url.as_str() == Some(url))
        .unwrap_or_else(|| panic!("no page for {url}"))
        .content
        .clone()
}

/// "Comparator.isLangCode"
#[test]
fn comparator_is_lang_code() {
    assert!(!is_lang_code("", None));

    assert!(is_lang_code("en", None));
    assert!(is_lang_code("en-us", None));

    assert!(!is_lang_code("dee", None));
    assert!(!is_lang_code("en_us", None));
    assert!(!is_lang_code("d", None));
    assert!(!is_lang_code("deed", None));
    assert!(!is_lang_code("deede", None));
    assert!(!is_lang_code("deedee", None));

    assert!(is_lang_code("zh-hant-tw", None));
    assert!(is_lang_code("es-419", None));

    // Valid BCP 47 syntax, but not a real script, region or plain language (#3766, #3352)
    assert!(!is_lang_code("sv-feed", None));
    assert!(!is_lang_code("en-sitemap", None));
    assert!(!is_lang_code("om-meg", None));

    // Real three-letter language codes that are also common words
    assert!(!is_lang_code("new", None));
    assert!(!is_lang_code("cat", None));
    assert!(!is_lang_code("fil", None));
    assert!(!is_lang_code("new-us", None));

    // Codes that `Intl` renames (Tagalog to Filipino, Twi to Akan) are still valid ISO 639-1
    assert!(is_lang_code("tl", None));
    assert!(is_lang_code("tw", None));

    // Withdrawn ISO 639-1 codes and uppercase
    assert!(!is_lang_code("in", None));
    assert!(!is_lang_code("iw", None));
    assert!(!is_lang_code("sh", None));
    assert!(!is_lang_code("EN", None));
}

/// "LangUtils ignores three-letter language codes in URLs and paths"
#[test]
fn lang_utils_ignores_three_letter_language_codes_in_urls_and_paths() {
    assert_eq!(get_language_code_from_url("/new/", None), "");
    assert_eq!(get_language_code_from_input_path("./new/es/index.md", None).as_deref(), Some("es"));
    assert_eq!(swap_language_code("/new/", "en", None), "/new/");
}

/// "LangUtils.swapLanguageCode"
#[test]
fn lang_utils_swap_language_code() {
    assert_eq!(swap_language_code("/", "", None), "/"); // skip
    assert_eq!(swap_language_code("/", "en", None), "/"); // skip
    assert_eq!(swap_language_code("/es/", "en", None), "/en/");
    assert_eq!(swap_language_code("/es/", "not", None), "/es/"); // skip
    assert_eq!(swap_language_code("/not-a-lang/", "en", None), "/not-a-lang/"); // skip
    assert_eq!(swap_language_code("/es/es/es/", "en", None), "/en/es/es/"); // first only
}

// "contentMap Event from Twelvety" and "locale_url and locale_links Filters": the
// `test/stubs-i18n` fixture includes a JavaScript template (`en-us/index.12ty.cjs`), which the
// port doesn’t support (skipped).

/// "errorMode default (strict)"
#[test]
fn error_mode_default_strict() {
    let result = to_json(
        "./test/stubs-i18n/",
        Some("./test/stubs-i18n/_site"),
        options(|cfg| add_i18n(cfg, r#"{"_test": "this is from errorMode default (strict)", "defaultLanguage": "en"}"#)),
    );
    assert!(result.is_err());
}

fn locale_url_preserves_fragments(error_mode: &'static str) {
    const URLS: [&str; 7] = ["/#heading", "/en/#heading", "/en#heading", "/#", "/#heading%20one#two", "/?a=1", "/en?a=1#heading"];
    let results = build_empty(move |cfg| {
        add_i18n(cfg, &format!(r#"{{"defaultLanguage": "en", "errorMode": "{error_mode}"}}"#))?;
        let mut template = URLS.iter().map(|u| format!(r#"{{{{ "{u}" | locale_url }}}}"#)).collect::<Vec<_>>().join("\n");
        template.push_str("\n{{ \"/#heading\" | locale_url(\"en-us\") }}");
        let template: &'static str = Box::leak(template.into_boxed_str());
        for lang in ["en", "es", "en-us"] {
            cfg.add_template(&format!("{lang}/index.njk"), template, Value::object())?;
        }
        cfg.add_template("index.njk", r#"{{ "/#heading" | locale_url }}"#, Value::object())
    })
    .unwrap();
    for lang in ["en", "es", "en-us"] {
        assert_eq!(
            content_by_url(&results, &format!("/{lang}/")),
            format!("/{lang}/#heading\n/{lang}/#heading\n/{lang}/#heading\n/{lang}/\n/{lang}/#heading%20one#two\n/{lang}/?a=1\n/{lang}/?a=1#heading\n/en-us/#heading"),
            "{error_mode} {lang}"
        );
    }
    assert_eq!(content_by_url(&results, "/"), "/en/#heading");
}

/// "locale_url preserves fragments (strict)"
#[test]
fn locale_url_preserves_fragments_strict() {
    locale_url_preserves_fragments("strict");
}

/// "locale_url preserves fragments (allow-fallback)"
#[test]
fn locale_url_preserves_fragments_allow_fallback() {
    locale_url_preserves_fragments("allow-fallback");
}

/// "locale_url preserves fragments (never)"
#[test]
fn locale_url_preserves_fragments_never() {
    locale_url_preserves_fragments("never");
}

fn locale_url_preserves_fragments_on_a_non_localized_fallback(error_mode: &'static str) {
    let results = build_empty(move |cfg| {
        add_i18n(cfg, &format!(r#"{{"defaultLanguage": "en", "errorMode": "{error_mode}"}}"#))?;
        cfg.add_template("shared.njk", "Shared page", Value::object())?;
        cfg.add_template(
            "es/index.njk",
            "{{ \"/shared/#heading\" | locale_url }}\n{{ \"/shared/?a=1\" | locale_url }}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/es/"), "/shared/#heading\n/shared/?a=1");
}

/// "locale_url preserves fragments on a non-localized fallback (allow-fallback)"
#[test]
fn locale_url_preserves_fragments_on_a_non_localized_fallback_allow_fallback() {
    locale_url_preserves_fragments_on_a_non_localized_fallback("allow-fallback");
}

/// "locale_url preserves fragments on a non-localized fallback (never)"
#[test]
fn locale_url_preserves_fragments_on_a_non_localized_fallback_never() {
    locale_url_preserves_fragments_on_a_non_localized_fallback("never");
}

/// "locale_url still rejects a non-localized fallback with a fragment in strict mode"
#[test]
fn locale_url_still_rejects_a_non_localized_fallback_with_a_fragment_in_strict_mode() {
    let result = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        cfg.add_template("shared.njk", "Shared page", Value::object())?;
        cfg.add_template("es/index.njk", r#"{{ "/shared/#heading" | locale_url }}"#, Value::object())
    });
    assert!(result.is_err());
}

fn locale_url_rejects_a_url_that_matches_no_page(error_mode: &'static str) {
    let result = build_empty(move |cfg| {
        add_i18n(cfg, &format!(r#"{{"defaultLanguage": "en", "errorMode": "{error_mode}"}}"#))?;
        cfg.add_template("es/index.njk", r#"{{ "/missing/" | locale_url }}"#, Value::object())
    });
    assert!(result.is_err());
}

/// "locale_url rejects a URL that matches no page (strict)"
#[test]
fn locale_url_rejects_a_url_that_matches_no_page_strict() {
    locale_url_rejects_a_url_that_matches_no_page("strict");
}

/// "locale_url rejects a URL that matches no page (allow-fallback)"
#[test]
fn locale_url_rejects_a_url_that_matches_no_page_allow_fallback() {
    locale_url_rejects_a_url_that_matches_no_page("allow-fallback");
}

/// "locale_url leaves fragment-only and external links unchanged"
#[test]
fn locale_url_leaves_fragment_only_and_external_links_unchanged() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "errorMode": "never"}"#)?;
        cfg.add_template(
            "es/index.njk",
            "{{ \"#heading\" | locale_url }}\n{{ \"https://example.com/#heading\" | locale_url }}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(results[0].content, "#heading\nhttps://example.com/#heading");
}

/// "locale_url swaps a language code already in a nested URL (#2791)"
#[test]
fn locale_url_swaps_a_language_code_already_in_a_nested_url_2791() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        for lang in ["en", "ro", "it"] {
            cfg.add_template(&format!("{lang}/my-article.njk"), r#"{{ "/en/my-article/" | locale_url }}"#, Value::object())?;
        }
        Ok(())
    })
    .unwrap();
    for lang in ["en", "ro", "it"] {
        assert_eq!(content_by_url(&results, &format!("/{lang}/my-article/")), format!("/{lang}/my-article/"));
    }
}

/// "locale_url ignores file names that look like language codes (#3766)"
#[test]
fn locale_url_ignores_file_names_that_look_like_language_codes_3766() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "errorMode": "never"}"#)?;
        for lang in ["en", "sv"] {
            let prefix = if lang == "en" { String::new() } else { format!("/{lang}") };
            cfg.add_template(&format!("{lang}/index.njk"), r#"{{ "/site.webmanifest" | locale_url }}"#, Value::object())?;
            cfg.add_template(
                &format!("{lang}-manifest.njk"),
                "",
                json(&format!(r#"{{"permalink": "{prefix}/site.webmanifest"}}"#)),
            )?;
            cfg.add_template(&format!("{lang}-feed.njk"), "", json(&format!(r#"{{"permalink": "/{lang}/feed/feed.xml"}}"#)))?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/"), "/site.webmanifest");
    assert_eq!(content_by_url(&results, "/sv/"), "/sv/site.webmanifest");
}

/// "Comparator.isLangCode with an explicit languages list"
#[test]
fn comparator_is_lang_code_with_an_explicit_languages_list() {
    let languages = vec!["en".to_string(), "ca-valencia".to_string()];
    assert!(is_lang_code("en", Some(&languages)));
    assert!(is_lang_code("ca-valencia", Some(&languages)));
    assert!(!is_lang_code("to-do", Some(&languages)));
    assert!(!is_lang_code("es", Some(&languages)));
}

/// "languages option limits detected language codes"
#[test]
fn languages_option_limits_detected_language_codes() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "languages": ["ca-valencia"]}"#)?;
        cfg.add_template("en/index.njk", "{{ page.lang }}", Value::object())?;
        cfg.add_template("ca-valencia/index.njk", r#"{{ page.lang }} {{ "/" | locale_url }}"#, Value::object())?;
        cfg.add_template("to-do.njk", "{{ page.lang }}", Value::object())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/"), "en");
    assert_eq!(content_by_url(&results, "/ca-valencia/"), "ca-valencia /ca-valencia/");
    assert_eq!(content_by_url(&results, "/to-do/"), "en");
}

/// "languages option must be an array"
#[test]
fn languages_option_must_be_an_array() {
    let err = build_empty(|cfg| add_i18n(cfg, r#"{"defaultLanguage": "en", "languages": "en,es"}"#)).unwrap_err();
    assert!(format!("{err:#}").contains("must be an array"), "{err:#}");
}

/// "languages option sets defaultLanguage to its first entry when missing"
#[test]
fn languages_option_sets_default_language_to_its_first_entry_when_missing() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"languages": ["es", "en"]}"#)?;
        cfg.add_template("about.njk", "{{ page.lang }}", Value::object())
    })
    .unwrap();
    assert_eq!(results[0].content, "es");
}

fn add_posts(cfg: &mut UserConfig, ext: &str, content: &'static str, posts: &[(&str, &str)]) -> anyhow::Result<()> {
    for lang in ["en", "es"] {
        for (slug, date) in posts {
            cfg.add_template(
                &format!("{lang}/posts/{slug}.{ext}"),
                content,
                json(&format!(r#"{{"tags": "posts", "date": "{date}"}}"#)),
            )?;
        }
    }
    Ok(())
}

/// "getPreviousCollectionItem and getNextCollectionItem skip translations when the collection has every language (#3207)"
#[test]
fn previous_and_next_collection_item_skip_translations_3207() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "languages": ["en", "es"]}"#)?;
        add_posts(
            cfg,
            "liquid",
            r#"{% assign p = collections.posts | getPreviousCollectionItem %}{% assign n = collections.posts | getNextCollectionItem %}{{ p.url | default: "NONE" }},{{ n.url | default: "NONE" }}"#,
            &[("first", "2024-01-01"), ("second", "2024-01-02")],
        )
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/posts/first/"), "NONE,/en/posts/second/");
    assert_eq!(content_by_url(&results, "/en/posts/second/"), "/en/posts/first/,NONE");
    assert_eq!(content_by_url(&results, "/es/posts/first/"), "NONE,/es/posts/second/");
    assert_eq!(content_by_url(&results, "/es/posts/second/"), "/es/posts/first/,NONE");
}

/// "getCollectionItemIndex is relative to the default language pages (#3207)"
#[test]
fn get_collection_item_index_is_relative_to_the_default_language_pages_3207() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "languages": ["en", "es"]}"#)?;
        add_posts(
            cfg,
            "liquid",
            "{{ collections.posts | getCollectionItemIndex }}",
            &[("first", "2024-01-01"), ("second", "2024-01-02")],
        )
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/posts/first/"), "0");
    assert_eq!(content_by_url(&results, "/es/posts/first/"), "0");
    assert_eq!(content_by_url(&results, "/en/posts/second/"), "1");
    assert_eq!(content_by_url(&results, "/es/posts/second/"), "1");
}

fn resolve_localized_item_from_collections_all(ext: &'static str, content: &'static str) {
    let results = build_empty(move |cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "languages": ["en", "es"]}"#)?;
        add_posts(cfg, ext, content, &[("first", "2024-01-01"), ("second", "2024-01-02"), ("third", "2024-01-03")])
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/posts/first/"), "NONE,/es/posts/second/");
    assert_eq!(content_by_url(&results, "/en/posts/second/"), "/es/posts/first/,/es/posts/third/");
    assert_eq!(content_by_url(&results, "/es/posts/third/"), "/es/posts/second/,NONE");
}

/// "getPreviousCollectionItem and getNextCollectionItem resolve a localized item from collections.all (liquid)"
#[test]
fn previous_and_next_collection_item_resolve_a_localized_item_liquid() {
    resolve_localized_item_from_collections_all(
        "liquid",
        r#"{% assign p = collections.posts | getPreviousCollectionItem: page, "es" %}{% assign n = collections.posts | getNextCollectionItem: page, "es" %}{{ p.url | default: "NONE" }},{{ n.url | default: "NONE" }}"#,
    );
}

/// "getPreviousCollectionItem and getNextCollectionItem resolve a localized item from collections.all (njk)"
#[test]
fn previous_and_next_collection_item_resolve_a_localized_item_njk() {
    resolve_localized_item_from_collections_all(
        "njk",
        r#"{% set p = collections.posts | getPreviousCollectionItem(page, "es") %}{% set n = collections.posts | getNextCollectionItem(page, "es") %}{{ p.url if p else "NONE" }},{{ n.url if n else "NONE" }}"#,
    );
}

// "… (12ty.js)" needs JavaScript templates and "… (custom)" needs
// `augmentFunctionContext` on a JavaScript `compile` function (skipped).

/// "locale_links and locale_url match paginated pages by URL"
#[test]
fn locale_links_and_locale_url_match_paginated_pages_by_url() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en", "errorMode": "never"}"#)?;
        let content = r#"{% for link in page.url | locale_links %}{{ link.url }} {% endfor %}|{{ "/en/story/gamma/" | locale_url }}"#;
        for (lang, stories) in [("en", r#"["alpha", "beta", "gamma"]"#), ("fr", r#"["alpha", "gamma"]"#)] {
            cfg.add_template(
                &format!("{lang}/story.njk"),
                content,
                json(&format!(
                    r#"{{"pagination": {{"data": "stories", "size": 1, "alias": "story"}}, "stories": {stories}, "permalink": "/{lang}/story/{{{{ story }}}}/"}}"#
                )),
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/story/alpha/"), "/fr/story/alpha/ |/en/story/gamma/");
    assert_eq!(content_by_url(&results, "/en/story/beta/"), "|/en/story/gamma/");
    assert_eq!(content_by_url(&results, "/fr/story/alpha/"), "/en/story/alpha/ |/fr/story/gamma/");
    assert_eq!(content_by_url(&results, "/fr/story/gamma/"), "/en/story/gamma/ |/fr/story/gamma/");
}

/// "locale_links matches paginated pages when the default language has no URL prefix"
#[test]
fn locale_links_matches_paginated_pages_when_the_default_language_has_no_url_prefix() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        for lang in ["en", "de", "es"] {
            let prefix = if lang == "en" { String::new() } else { format!("/{lang}") };
            // es has no matching translations
            let permalink = if lang == "es" {
                "/es/noticias/{{ pagination.pageNumber }}/".to_string()
            } else {
                format!("{prefix}/blog/{{{{ pagination.pageNumber }}}}/")
            };
            cfg.add_template(
                &format!("{lang}/blog.njk"),
                "{% for link in page.url | locale_links %}{{ link.url }} {% endfor %}",
                json(&format!(r#"{{"pagination": {{"data": "posts", "size": 1}}, "posts": [1, 2], "permalink": "{permalink}"}}"#)),
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/blog/1/"), "/de/blog/1/ ");
    assert_eq!(content_by_url(&results, "/de/blog/0/"), "/blog/0/ ");
    assert_eq!(content_by_url(&results, "/es/noticias/0/"), "");
}

/// "buildawesomeTranslationKey groups paginated translations with different URLs"
#[test]
fn buildawesome_translation_key_groups_paginated_translations_with_different_urls() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        cfg.add_global_data(
            "tagTranslations",
            json(
                r#"[
                    {"buildawesomeTranslationKey": "wood", "en": "wood", "de": "holz"},
                    {"twelvetyPaginationGroupNumber": "metal", "en": "metal", "de": "metall"}
                ]"#,
            ),
        );
        for lang in ["en", "de"] {
            cfg.add_template(
                &format!("{lang}/tags.njk"),
                r#"{% for link in page.url | locale_links %}{{ link.url }} {% endfor %}|{{ "/en/tags/wood/" | locale_url }}"#,
                json(&format!(
                    r#"{{"pagination": {{"data": "tagTranslations", "size": 1, "alias": "tag"}}, "permalink": "/{lang}/tags/{{{{ tag.{lang} }}}}/"}}"#
                )),
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/tags/wood/"), "/de/tags/holz/ |/en/tags/wood/");
    assert_eq!(content_by_url(&results, "/de/tags/holz/"), "/en/tags/wood/ |/de/tags/holz/");
    assert_eq!(content_by_url(&results, "/de/tags/metall/"), "/en/tags/metal/ |/de/tags/holz/");
}

/// "locale_links labels use native language names"
#[test]
fn locale_links_labels_use_native_language_names() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        for lang in ["en", "es", "ka", "zu", "ja", "tl"] {
            cfg.add_template(
                &format!("{lang}/index.njk"),
                "{% for link in page.url | locale_links %}{{ link.lang }}:{{ link.label }} {% endfor %}",
                Value::object(),
            )?;
        }
        Ok(())
    })
    .unwrap();
    // Capitalized for `es`, but not Georgian, `isiZulu` or scripts without case
    assert_eq!(content_by_url(&results, "/en/"), "es:Español ja:日本語 ka:ქართული tl:Filipino zu:isiZulu ");
}

/// "locale_links includeCurrent option"
#[test]
fn locale_links_include_current_option() {
    let results = build_empty(|cfg| {
        add_i18n(cfg, r#"{"defaultLanguage": "en"}"#)?;
        for lang in ["en", "es"] {
            cfg.add_template(
                &format!("{lang}/index.njk"),
                "{% for link in page.url | locale_links({ includeCurrent: true }) %}{{ link.lang }}:{{ link.label }} {% endfor %}|{% for link in page.url | locale_links %}{{ link.lang }} {% endfor %}",
                Value::object(),
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(content_by_url(&results, "/en/"), "en:English es:Español |es ");
}
