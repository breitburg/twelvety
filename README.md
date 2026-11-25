# Twelvety

[![Test](https://github.com/breitburg/twelvety/actions/workflows/test.yml/badge.svg)](https://github.com/breitburg/twelvety/actions/workflows/test.yml)

Twelvety is a simple static site generator written in Rust. You give it a folder of templates (Markdown, Liquid, Nunjucks or plain HTML) and it turns them into a folder of HTML files you can host anywhere.

It is a port of [Build Awesome](https://github.com/11ty/buildawesome) and follows the original closely, so most of its documentation applies here too. The main difference is that there is no JavaScript: configuration is a JSON, TOML or YAML file, or Rust code if you use Twelvety as a library.

## Installation

You need a recent Rust toolchain. Install the `twelvety` command with Cargo:

```sh
cargo install --git https://github.com/breitburg/twelvety
```

Or clone the repository and build it yourself with `cargo build --release`. The binary ends up in `target/release/twelvety`.

## Getting started

Make a new folder for your site and add a page called `index.md`:

```md
---
title: Home
---
# Welcome

This is my new site.
```

Run `twelvety` in that folder. It finds every template, renders it and writes the result to `_site`. In this case it creates `_site/index.html`.

While you work on the site, run `twelvety --serve` instead. It starts a local server at http://localhost:8080, rebuilds when you save a file and reloads the page in your browser.

### Layouts

Most pages share the same surrounding HTML. Put it in a layout in the `_includes` folder, for example `_includes/base.njk`:

```html
<!doctype html>
<html>
<head><title>{{ title }}</title></head>
<body>
{{ content | safe }}
</body>
</html>
```

Then add `layout: base.njk` to the front matter of a page. The rendered page is inserted where `content` appears.

### Data

The block between the `---` lines at the top of a page is front matter. Its values, like `title`, are available in the page and in its layout.

For data that many pages use, add a JSON file to the `_data` folder. A file called `_data/site.json` containing `{ "name": "My site" }` makes `{{ site.name }}` available everywhere.

To set data for a whole folder, add a JSON file named after it. For example, `posts/posts.json` applies to every page in `posts`:

```json
{ "layout": "base.njk", "tags": "post" }
```

### Collections

Pages with the same tag are grouped into a collection. With the file above, every post is in `collections.post`, so the home page can list them:

```md
{% for post in collections.post %}
- [{{ post.data.title }}]({{ post.url }})
{% endfor %}
```

Each page gets a clean URL based on its path. `posts/hello.md` becomes `/posts/hello/`. You can choose a different one with `permalink` in the front matter.

## Configuration

To change the defaults, add a `twelvety.config.toml` (or `.json` or `.yaml`) to the root of your project:

```toml
templateFormats = ["md", "njk", "html", "liquid"]
markdownTemplateEngine = "njk"
passthroughCopy = ["src/img", { "src/fonts" = "fonts" }]

[dir]
input = "src"
output = "dist"

[[plugins]]
name = "html-base"
options = { baseHref = "https://example.com/" }
```

This example reads the site from `src` and writes it to `dist`. It also copies images and fonts without processing them, and the `html-base` plugin points root-relative links at a base URL.

The built-in plugins are `html-base`, `id-attribute`, `i18n`, `input-path-to-url`, `render` and `preserve-closing-tags`.

Run `twelvety --help` to see all command-line options. They include `--input` and `--output` to choose the folders, `--watch` to rebuild without a server and `--to=json` to print the rendered pages instead of writing them.

## Using it as a library

You can also run Twelvety from Rust. That lets you add your own filters, shortcodes and collections as closures:

```rust
use twelvety::{Options, Twelvety, Value};

let options = Options::default().with_config(|cfg| {
    cfg.add_filter("shout", |_ctx, input, _args| {
        Ok(Value::str(input.to_js_string().to_uppercase()))
    });
    cfg.add_shortcode("year", |_ctx, _args| Ok("2026".into()));
    cfg.add_collection("posts", |api| Ok(api.get_filtered_by_glob("src/posts/*.md")))?;
    cfg.add_passthrough_copy("src/img");
    Ok(())
});
let mut site = Twelvety::new(Some("src"), Some("dist"), options);
site.write()?;
```

## What isn't supported

Anything that needs a JavaScript runtime is not supported. This includes JavaScript configuration files, `*.12ty.js` templates, JavaScript data files and JavaScript front matter.

## How the port is organized

Most upstream source files have a Rust module with the same role, and functions keep their names in snake_case. For example, `TemplateMap.js` is `src/template_map.rs`, and the files in `Engines/` and `Plugins/` are in `src/engines/` and `src/plugins/`.

Some parts had to work differently in Rust:

- Upstream relies on JavaScript's dynamic values. Here, `src/value.rs` defines a `Value` type that behaves like them.
- Liquid has its own engine in `src/liquid/`, which follows liquidjs.
- Nunjucks templates run on [minijinja](https://github.com/mitsuhiko/minijinja), with an extra pass that handles Nunjucks-only syntax.
- Markdown uses [markdown-it](https://crates.io/crates/markdown-it).
- HTML transforms use [lol_html](https://github.com/cloudflare/lol-html) instead of posthtml.

The tests in `tests/` are ported from the upstream test suite and use its fixtures in `test/`. Run them with `cargo test`.

## License

MIT, like upstream.
