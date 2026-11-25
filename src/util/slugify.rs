//! Port of `@sindresorhus/slugify` (with `@sindresorhus/transliterate`).

#[derive(Clone, Debug)]
pub struct SlugifyOptions {
    pub separator: String,
    pub lowercase: bool,
    pub decamelize: bool,
    pub custom_replacements: Vec<(String, String)>,
    pub preserve_leading_underscore: bool,
    pub preserve_trailing_dash: bool,
    pub preserve_characters: Vec<char>,
}

impl Default for SlugifyOptions {
    fn default() -> Self {
        SlugifyOptions {
            separator: "-".into(),
            lowercase: true,
            decamelize: true,
            custom_replacements: vec![],
            preserve_leading_underscore: false,
            preserve_trailing_dash: false,
            preserve_characters: vec![],
        }
    }
}

const BUILTIN_REPLACEMENTS: &[(&str, &str)] =
    &[("&", " and "), ("🦄", " unicorn "), ("♥", " love ")];

const TRANSLITERATE_REPLACEMENTS: &[(&str, &str)] = &[
    ("Ä", "Ae"),
    ("ä", "ae"),
    ("Ö", "Oe"),
    ("ö", "oe"),
    ("Ü", "Ue"),
    ("ü", "ue"),
    ("ß", "ss"),
    ("ẞ", "Ss"),
    ("Æ", "AE"),
    ("æ", "ae"),
    ("Ø", "O"),
    ("ø", "o"),
    ("Å", "A"),
    ("å", "a"),
    ("Œ", "OE"),
    ("œ", "oe"),
    ("Þ", "TH"),
    ("þ", "th"),
    ("Ð", "D"),
    ("ð", "d"),
];

pub fn transliterate(s: &str, custom: &[(String, String)]) -> String {
    let mut out = s.to_string();
    for (from, to) in custom {
        out = out.replace(from.as_str(), to);
    }
    for (from, to) in TRANSLITERATE_REPLACEMENTS {
        out = out.replace(from, to);
    }
    // keep ASCII as-is, transliterate the rest
    let mut result = String::with_capacity(out.len());
    for c in out.chars() {
        if c.is_ascii() {
            result.push(c);
        } else {
            result.push_str(deunicode::deunicode_char(c).unwrap_or(""));
        }
    }
    result
}

fn decamelize(s: &str) -> String {
    // ([A-Z]{2,})(\d+) -> $1 $2 ; ([a-z\d]+)([A-Z]{2,}) -> $1 $2 ; ([a-z\d])([A-Z]) -> $1 $2 ; ([A-Z]+)([A-Z][a-rt-z\d]+) -> $1 $2
    let re1 = regex::Regex::new(r"([A-Z]{2,})(\d+)").unwrap();
    let re2 = regex::Regex::new(r"([a-z\d]+)([A-Z]{2,})").unwrap();
    let re3 = regex::Regex::new(r"([a-z\d])([A-Z])").unwrap();
    let re4 = regex::Regex::new(r"([A-Z]+)([A-Z][a-rt-z\d]+)").unwrap();
    let s = re1.replace_all(s, "$1 $2");
    let s = re2.replace_all(&s, "$1 $2");
    let s = re3.replace_all(&s, "$1 $2");
    re4.replace_all(&s, "$1 $2").to_string()
}

fn remove_moot_separators(s: &str, separator: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for part in s.split(separator) {
        if part.is_empty() {
            last_was_sep = true;
            continue;
        }
        if !out.is_empty() || last_was_sep && !out.is_empty() {
            out.push_str(separator);
        }
        out.push_str(part);
        last_was_sep = false;
    }
    out
}

pub fn slugify(input: &str, options: &SlugifyOptions) -> String {
    let should_prepend_underscore = options.preserve_leading_underscore && input.starts_with('_');
    let should_append_dash = options.preserve_trailing_dash && input.ends_with('-');

    let mut replacements: Vec<(String, String)> = BUILTIN_REPLACEMENTS
        .iter()
        .filter(|(k, _)| !options.custom_replacements.iter().any(|(c, _)| c == k))
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    replacements.extend(options.custom_replacements.iter().cloned());

    let mut s = transliterate(input, &replacements);
    if options.decamelize {
        s = decamelize(&s);
    }
    if options.lowercase {
        s = s.to_lowercase();
    }
    let mut out = String::new();
    let mut in_sep = false;
    for c in s.chars() {
        let keep = if options.lowercase {
            c.is_ascii_lowercase() || c.is_ascii_digit()
        } else {
            c.is_ascii_alphanumeric()
        } || options.preserve_characters.contains(&c);
        if keep {
            out.push(c);
            in_sep = false;
        } else if !in_sep {
            out.push_str(&options.separator);
            in_sep = true;
        }
    }
    let mut s = out.replace('\\', "");
    if !options.separator.is_empty() {
        s = remove_moot_separators(&s, &options.separator);
    }
    if should_prepend_underscore {
        s = format!("_{s}");
    }
    if should_append_dash {
        s.push('-');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        let opts = SlugifyOptions {
            decamelize: false,
            ..Default::default()
        };
        assert_eq!(slugify("Hello World", &opts), "hello-world");
        assert_eq!(slugify("This is a test", &opts), "this-is-a-test");
        assert_eq!(slugify("Déjà Vu!", &opts), "deja-vu");
        assert_eq!(
            slugify("Fußgängerübergänge", &opts),
            "fussgaengeruebergaenge"
        );
        assert_eq!(slugify("I ♥ Dogs", &opts), "i-love-dogs");
        assert_eq!(slugify("  --foo--bar--  ", &opts), "foo-bar");
        assert_eq!(slugify("fooBar", &SlugifyOptions::default()), "foo-bar");
        assert_eq!(slugify("fooBar", &opts), "foobar");
    }
}
