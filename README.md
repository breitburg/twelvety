# Twelvety

A Rust port of [Build Awesome](https://github.com/11ty/buildawesome), a simpler static site generator. It transforms a directory of templates (Liquid, Nunjucks, Markdown, HTML and custom formats) into HTML.

The port follows the upstream code closely. Most upstream modules have a Rust counterpart with the same responsibilities and method names in snake_case. Where JavaScript patterns don't translate directly (dynamic objects, Proxies, async callbacks), the code uses Rust equivalents.

## Usage

```sh
cargo build --release
./target/release/twelvety                      # build ./ into ./_site
./target/release/twelvety --input=src --output=dist
./target/release/twelvety --serve --port=8080  # dev server with live reload
./target/release/twelvety --watch
./target/release/twelvety --to=json            # print rendered results as JSON
./target/release/twelvety --help
```

The CLI supports the upstream flags: `--input`, `--output`, `--formats`, `--config`, `--pathprefix`, `--port`, `--to`, `--quiet`, `--dryrun`, `--watch`, `--serve`, `--ignore-initial`, `--version` and `--help`.

## Configuration

Upstream configuration files are JavaScript modules. Twelvety supports two alternatives.

### Declarative configuration files

These cover the serializable subset of the configuration API: `buildawesome.config.{json,toml,yaml}`, `twelvety.config.{json,toml,yaml}` or `.twelvety.json`.

```toml
pathPrefix = "/docs/"
templateFormats = ["md", "njk", "html", "liquid"]
markdownTemplateEngine = "njk"
passthroughCopy = ["src/img", { "src/fonts" = "fonts" }]
dataExtensions = ["yaml", "toml"]

[dir]
input = "src"
output = "dist"

[[plugins]]
name = "id-attribute"

[[plugins]]
name = "html-base"
options = { baseHref = "https://example.com/" }
```

Built-in plugins: `html-base`, `id-attribute`, `i18n`, `input-path-to-url`, `render` and `preserve-closing-tags`.

### The Rust API

The Rust API is the equivalent of `twelvetyConfig`. JavaScript callbacks are Rust closures.

```rust
use twelvety::{Twelvety, Options, Value};

let options = Options::default().with_config(|cfg| {
    cfg.add_filter("shout", |_ctx, input, _args| {
        Ok(Value::str(input.to_js_string().to_uppercase()))
    });
    cfg.add_shortcode("year", |_ctx, _args| Ok("2026".into()));
    cfg.add_collection("posts", |api| Ok(api.get_filtered_by_glob("src/posts/*.md")))?;
    cfg.add_passthrough_copy("src/img");
    cfg.add_plugin("html-base", twelvety::plugins::html_base::plugin, Value::object())?;
    Ok(())
});
let mut twelvety = Twelvety::new(Some("src"), Some("dist"), options);
twelvety.write()?;
```

## Architecture

| Upstream (JS) | Rust |
| --- | --- |
| `Core.js`, `CoreMinimal.js` | `src/core.rs` |
| `TemplateConfig.js`, `UserConfig.js`, `defaultConfig*.js` | `src/template_config.rs`, `src/user_config.rs`, `src/default_config.rs`, `src/config_file.rs` |
| `TemplateWriter.js`, `TemplateMap.js`, `TemplateCollection.js` | `src/template_writer.rs`, `src/template_map.rs`, `src/template_collection.rs` |
| `Template.js`, `TemplateContent.js`, `TemplateLayout*.js`, `TemplatePermalink.js` | `src/template.rs`, `src/template_content.rs`, `src/template_layout.rs`, `src/template_permalink.rs` |
| `TemplateRender.js`, `TemplatePipeline.js`, `ExtensionMap.js` | `src/template_render.rs`, `src/template_pipeline.rs`, `src/extension_map.rs` |
| `Data/*` (TemplateData, ComputedData, …) | `src/data/*` |
| `GlobalDependencyMap.js`, `Util/TemplateDepGraph.js` | `src/global_dependency_map.rs` |
| `Engines/*` | `src/engines/*` |
| `Plugins/*` | `src/plugins/*` |
| `Files.js`, `FileSystemSearch.js`, `TemplatePassthrough*.js` | `src/files.rs`, `src/file_system_search.rs`, `src/template_passthrough.rs` |
| `@11ty/eleventy-utils` (TemplatePath, Merge) | `src/util/template_path.rs`, `src/util/merge.rs` |

Notable adaptations:

- **Data model:** `src/value.rs` is a dynamic `Value` that models JavaScript values: `undefined`, `null`, numbers as `f64`, dates, and reference-counted copy-on-write objects and arrays. Collection items are lazy host objects (`src/page_entry.rs`), so `templateContent` becomes available after rendering, like the getter upstream.
- **Liquid:** a native Liquid engine in `src/liquid/` that follows liquidjs semantics (JS truthiness, lenient undefined variables, the standard tags and filters, `include`/`render`, shortcodes).
- **Nunjucks:** built on [minijinja](https://github.com/mitsuhiko/minijinja) with a compatibility pass for Nunjucks-only syntax (shortcodes, `for k, v in obj`, `elseif`, `===`) and Nunjucks-style escaping and output.
- **Markdown:** uses the [markdown-it](https://crates.io/crates/markdown-it) Rust port, configured like Twelvety's markdown-it defaults.
- **HTML transforms:** the HTML transformer (`<base>`, InputPathToUrl, the id attribute and html-relative copies) runs on [lol_html](https://github.com/cloudflare/lol-html) instead of posthtml.
- **Computed data:** dependencies are found by parsing template strings. Rust function entries declare their dependencies explicitly instead of relying on Proxy read tracking.

## Not supported

These need a JavaScript runtime: JavaScript configuration files, `*.12ty.js` templates, JavaScript data files (`*.12tydata.js`, `_data/*.js`) and JavaScript front matter.

## Tests

```sh
cargo test
```

The integration tests in `tests/` are ported from the upstream test suite and use the upstream fixtures in `test/`.

## License

MIT, like upstream.
