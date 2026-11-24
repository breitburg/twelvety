//! Port of `Plugins/I18nPlugin.js` (plus `Filters/LocaleCollection.js`).
//!
//! `Intl.Locale`/`Intl.DisplayNames` are replaced by an ISO 639-1 table.

use std::sync::{Arc, RwLock};

use anyhow::{Result, bail};
use indexmap::IndexMap;

use crate::filters::{find_collection_item_index, get_collection_item};
use crate::template_config::ConfigHandle;
use crate::user_config::{CallContext, UserConfig};
use crate::value::{Map, Value};

const WITHDRAWN_LANGUAGES: &[&str] = &["bh", "in", "iw", "ji", "jw", "mo", "sh"];

/// ISO 639-1 codes with their native names
const LANGUAGES: &[(&str, &str)] = &[
    ("aa", "Afar"), ("ab", "Аҧсшәа"), ("af", "Afrikaans"), ("ak", "Akan"), ("am", "አማርኛ"),
    ("an", "Aragonés"), ("ar", "العربية"), ("as", "অসমীয়া"), ("av", "Авар"), ("ay", "Aymar"),
    ("az", "Azərbaycan"), ("ba", "Башҡорт"), ("be", "Беларуская"), ("bg", "Български"),
    ("bi", "Bislama"), ("bm", "Bamanakan"), ("bn", "বাংলা"), ("bo", "བོད་སྐད་"), ("br", "Brezhoneg"),
    ("bs", "Bosanski"), ("ca", "Català"), ("ce", "Нохчийн"), ("ch", "Chamoru"), ("co", "Corsu"),
    ("cr", "Cree"), ("cs", "Čeština"), ("cu", "Church Slavic"), ("cv", "Чӑваш"), ("cy", "Cymraeg"),
    ("da", "Dansk"), ("de", "Deutsch"), ("dv", "Dhivehi"), ("dz", "རྫོང་ཁ"), ("ee", "Eʋegbe"),
    ("el", "Ελληνικά"), ("en", "English"), ("eo", "Esperanto"), ("es", "Español"), ("et", "Eesti"),
    ("eu", "Euskara"), ("fa", "فارسی"), ("ff", "Pulaar"), ("fi", "Suomi"), ("fj", "Fijian"),
    ("fo", "Føroyskt"), ("fr", "Français"), ("fy", "Frysk"), ("ga", "Gaeilge"), ("gd", "Gàidhlig"),
    ("gl", "Galego"), ("gn", "Guarani"), ("gu", "ગુજરાતી"), ("gv", "Gaelg"), ("ha", "Hausa"),
    ("he", "עברית"), ("hi", "हिन्दी"), ("ho", "Hiri Motu"), ("hr", "Hrvatski"), ("ht", "Kreyòl ayisyen"),
    ("hu", "Magyar"), ("hy", "Հայերեն"), ("hz", "Herero"), ("ia", "Interlingua"), ("id", "Indonesia"),
    ("ie", "Interlingue"), ("ig", "Igbo"), ("ii", "ꆈꌠꉙ"), ("ik", "Inupiaq"), ("io", "Ido"),
    ("is", "Íslenska"), ("it", "Italiano"), ("iu", "Inuktitut"), ("ja", "日本語"), ("jv", "Jawa"),
    ("ka", "ქართული"), ("kg", "Kongo"), ("ki", "Gikuyu"), ("kj", "Kuanyama"), ("kk", "Қазақ тілі"),
    ("kl", "Kalaallisut"), ("km", "ខ្មែរ"), ("kn", "ಕನ್ನಡ"), ("ko", "한국어"), ("kr", "Kanuri"),
    ("ks", "کٲشُر"), ("ku", "Kurdî"), ("kv", "Komi"), ("kw", "Kernewek"), ("ky", "Кыргызча"),
    ("la", "Latina"), ("lb", "Lëtzebuergesch"), ("lg", "Luganda"), ("li", "Limburgs"), ("ln", "Lingála"),
    ("lo", "ລາວ"), ("lt", "Lietuvių"), ("lu", "Tshiluba"), ("lv", "Latviešu"), ("mg", "Malagasy"),
    ("mh", "Marshallese"), ("mi", "Māori"), ("mk", "Македонски"), ("ml", "മലയാളം"), ("mn", "Монгол"),
    ("mr", "मराठी"), ("ms", "Melayu"), ("mt", "Malti"), ("my", "မြန်မာ"), ("na", "Nauru"),
    ("nb", "Norsk bokmål"), ("nd", "isiNdebele"), ("ne", "नेपाली"), ("ng", "Ndonga"), ("nl", "Nederlands"),
    ("nn", "Norsk nynorsk"), ("no", "Norsk"), ("nr", "South Ndebele"), ("nv", "Navajo"), ("ny", "Nyanja"),
    ("oc", "Occitan"), ("oj", "Ojibwa"), ("om", "Oromoo"), ("or", "ଓଡ଼ିଆ"), ("os", "Ирон"),
    ("pa", "ਪੰਜਾਬੀ"), ("pi", "Pali"), ("pl", "Polski"), ("ps", "پښتو"), ("pt", "Português"),
    ("qu", "Runasimi"), ("rm", "Rumantsch"), ("rn", "Ikirundi"), ("ro", "Română"), ("ru", "Русский"),
    ("rw", "Kinyarwanda"), ("sa", "संस्कृत भाषा"), ("sc", "Sardu"), ("sd", "سنڌي"), ("se", "Davvisámegiella"),
    ("sg", "Sängö"), ("si", "සිංහල"), ("sk", "Slovenčina"), ("sl", "Slovenščina"), ("sm", "Samoan"),
    ("sn", "chiShona"), ("so", "Soomaali"), ("sq", "Shqip"), ("sr", "Српски"), ("ss", "Swati"),
    ("st", "Sesotho"), ("su", "Basa Sunda"), ("sv", "Svenska"), ("sw", "Kiswahili"), ("ta", "தமிழ்"),
    ("te", "తెలుగు"), ("tg", "Тоҷикӣ"), ("th", "ไทย"), ("ti", "ትግርኛ"), ("tk", "Türkmen dili"),
    ("tl", "Filipino"), ("tn", "Setswana"), ("to", "Lea fakatonga"), ("tr", "Türkçe"), ("ts", "Tsonga"),
    ("tt", "Татар"), ("tw", "Twi"), ("ty", "Tahitian"), ("ug", "ئۇيغۇرچە"), ("uk", "Українська"),
    ("ur", "اردو"), ("uz", "Oʻzbek"), ("ve", "Venda"), ("vi", "Tiếng Việt"), ("vo", "Volapük"),
    ("wa", "Walon"), ("wo", "Wolof"), ("xh", "IsiXhosa"), ("yi", "ייִדיש"), ("yo", "Èdè Yorùbá"),
    ("za", "Zhuang"), ("zh", "中文"), ("zu", "isiZulu"),
];

#[derive(Clone, Debug, Default)]
pub struct I18nOptions {
    pub default_language: String,
    pub languages: Option<Vec<String>>,
    pub url_filter: String,
    pub links_filter: String,
    pub error_mode: String,
}

/// Region subtags with a display name (`Intl.DisplayNames` type `region`)
const REGIONS: &[&str] = &[
    "AC", "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AN", "AO", "AQ", "AR", "AS", "AT", "AU", "AW",
    "AX", "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ",
    "BR", "BS", "BT", "BU", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK",
    "CL", "CM", "CN", "CO", "CP", "CQ", "CR", "CS", "CU", "CV", "CW", "CX", "CY", "CZ", "DD", "DE",
    "DG", "DJ", "DK", "DM", "DO", "DY", "DZ", "EA", "EC", "EE", "EG", "EH", "ER", "ES", "ET", "EU",
    "EZ", "FI", "FJ", "FK", "FM", "FO", "FR", "FX", "GA", "GB", "GD", "GE", "GF", "GG", "GH", "GI",
    "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM", "HN", "HR", "HT",
    "HU", "HV", "IC", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM", "JO",
    "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI",
    "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK", "ML",
    "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC",
    "NE", "NF", "NG", "NH", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG",
    "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY", "QA", "QO", "RE", "RH", "RO", "RS",
    "RU", "RW", "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO",
    "SR", "SS", "ST", "SU", "SV", "SX", "SY", "SZ", "TA", "TC", "TD", "TF", "TG", "TH", "TJ", "TK",
    "TL", "TM", "TN", "TO", "TP", "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UK", "UM", "UN", "US",
    "UY", "UZ", "VA", "VC", "VD", "VE", "VG", "VI", "VN", "VU", "WF", "WS", "XA", "XB", "XK", "YD",
    "YE", "YT", "YU", "ZA", "ZM", "ZR", "ZW", "ZZ", "001", "002", "003", "005", "009", "011", "013",
    "014", "015", "017", "018", "019", "021", "029", "030", "034", "035", "039", "053", "054",
    "057", "061", "142", "143", "145", "150", "151", "154", "155", "202", "419"
];

/// Script subtags with a display name (`Intl.DisplayNames` type `script`)
const SCRIPTS: &[&str] = &[
    "Adlm", "Afak", "Aghb", "Ahom", "Arab", "Aran", "Armi", "Armn", "Avst", "Bali", "Bamu", "Bass",
    "Batk", "Beng", "Berf", "Bhks", "Blis", "Bopo", "Brah", "Brai", "Bugi", "Buhd", "Cakm", "Cans",
    "Cari", "Cham", "Cher", "Chrs", "Cirt", "Copt", "Cpmn", "Cprt", "Cyrl", "Cyrs", "Deva", "Diak",
    "Dogr", "Dsrt", "Dupl", "Egyd", "Egyh", "Egyp", "Elba", "Elym", "Ethi", "Gara", "Geok", "Geor",
    "Glag", "Gong", "Gonm", "Goth", "Gran", "Grek", "Gujr", "Gukh", "Guru", "Hanb", "Hang", "Hani",
    "Hano", "Hans", "Hant", "Hatr", "Hebr", "Hira", "Hluw", "Hmng", "Hmnp", "Hrkt", "Hung", "Inds",
    "Ital", "Jamo", "Java", "Jpan", "Jurc", "Kali", "Kana", "Kawi", "Khar", "Khmr", "Khoj", "Kits",
    "Knda", "Kore", "Kpel", "Krai", "Kthi", "Lana", "Laoo", "Latf", "Latg", "Latn", "Lepc", "Limb",
    "Lina", "Linb", "Lisu", "Loma", "Lyci", "Lydi", "Mahj", "Maka", "Mand", "Mani", "Marc", "Maya",
    "Medf", "Mend", "Merc", "Mero", "Mlym", "Modi", "Mong", "Moon", "Mroo", "Mtei", "Mult", "Mymr",
    "Nagm", "Nand", "Narb", "Nbat", "Newa", "Nkgb", "Nkoo", "Nshu", "Ogam", "Olck", "Onao", "Orkh",
    "Orya", "Osge", "Osma", "Ougr", "Palm", "Pauc", "Perm", "Phag", "Phli", "Phlp", "Phlv", "Phnx",
    "Plrd", "Prti", "Qaag", "Rjng", "Rohg", "Roro", "Runr", "Samr", "Sara", "Sarb", "Saur", "Sgnw",
    "Shaw", "Shrd", "Sidd", "Sidt", "Sind", "Sinh", "Sogd", "Sogo", "Sora", "Soyo", "Sund", "Sunu",
    "Sylo", "Syrc", "Syre", "Syrj", "Syrn", "Tagb", "Takr", "Tale", "Talu", "Taml", "Tang", "Tavt",
    "Tayo", "Telu", "Teng", "Tfng", "Tglg", "Thaa", "Thai", "Tibt", "Tirh", "Tnsa", "Todr", "Tols",
    "Toto", "Tutg", "Ugar", "Vaii", "Visp", "Vith", "Wara", "Wcho", "Wole", "Xpeo", "Xsux", "Yezi",
    "Yiii", "Zanb", "Zinh", "Zmth", "Zsye", "Zsym", "Zxxx", "Zyyy", "Zzzz"
];

/// `Comparator.isLangCode`
pub fn is_lang_code(code: &str, languages: Option<&[String]>) -> bool {
    if let Some(langs) = languages {
        return langs.iter().any(|l| l == code);
    }
    let parts: Vec<&str> = code.split('-').collect();
    let s = parts[0];
    if s.len() != 2 || s != s.to_lowercase() || WITHDRAWN_LANGUAGES.contains(&s) {
        return false;
    }
    if !LANGUAGES.iter().any(|(c, _)| *c == s) {
        return false;
    }
    // Only a real script (4 letters) and/or region (2 letters or 3 digits), so names like
    // `sv-feed` or `en-sitemap` don’t match. Subtags are case-insensitive (`Intl.Locale`).
    let mut rest = &parts[1..];
    if let Some(script) = rest.first().filter(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_alphabetic())) {
        let titlecase = format!("{}{}", script[..1].to_ascii_uppercase(), script[1..].to_ascii_lowercase());
        if !SCRIPTS.contains(&titlecase.as_str()) {
            return false;
        }
        rest = &rest[1..];
    }
    if let Some(region) = rest.first() {
        let is_region = (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()))
            || (region.len() == 3 && region.chars().all(|c| c.is_ascii_digit()));
        if !is_region || !REGIONS.contains(&region.to_ascii_uppercase().as_str()) {
            return false;
        }
        rest = &rest[1..];
    }
    rest.is_empty()
}

fn url_has_lang_code(url: &str, code: &str, languages: Option<&[String]>) -> bool {
    if !is_lang_code(code, languages) {
        return false;
    }
    url.split('/').any(|e| e == code)
}

pub fn get_language_code_from_input_path(filepath: &str, languages: Option<&[String]>) -> Option<String> {
    filepath
        .split('/')
        .find(|e| is_lang_code(e, languages))
        .map(|s| s.to_string())
}

pub fn get_language_code_from_url(url: &str, languages: Option<&[String]>) -> String {
    let s: Vec<&str> = url.split('/').collect();
    if s.len() > 1 && is_lang_code(s[1], languages) {
        s[1].to_string()
    } else {
        String::new()
    }
}

fn swap_language_code_no_check(s: &str, lang_code: &str, languages: Option<&[String]>) -> String {
    let mut found = false;
    s.split('/')
        .map(|e| {
            if !found && is_lang_code(e, languages) {
                found = true;
                lang_code.to_string()
            } else {
                e.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn swap_language_code(s: &str, lang_code: &str, languages: Option<&[String]>) -> String {
    if !is_lang_code(lang_code, languages) {
        return s.to_string();
    }
    swap_language_code_no_check(s, lang_code, languages)
}

fn get_native_language_name(lang_code: &str) -> String {
    let code = lang_code.split('-').next().unwrap_or(lang_code);
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| n.to_string())
        .unwrap_or_else(|| code.to_string())
}

fn get_locale_urls_map(url_to_input_path: &Value, remove_ext: &dyn Fn(&str) -> String, options: &I18nOptions) -> IndexMap<String, Vec<Value>> {
    let langs = options.languages.as_deref();
    let mut filemap: IndexMap<String, Vec<Value>> = IndexMap::new();
    if let Value::Object(map) = url_to_input_path {
        for (url, entry) in map.iter() {
            let original = entry.get("inputPath").to_output_string();
            let group = entry.get("groupNumber");
            let filepath = remove_ext(&original);
            let mut replaced = swap_language_code_no_check(&filepath, "__11ty_i18n", langs);
            if group.is_nullish() {
                let url_lang = get_language_code_from_url(url, langs);
                let rest = if url_lang.is_empty() { url.clone() } else { url[url_lang.len() + 1..].to_string() };
                replaced.push_str(&format!("_url:{rest}"));
            } else {
                replaced.push_str(&format!("_group:{}", group.to_js_string()));
            }
            let mut lang = get_language_code_from_input_path(&original, langs).unwrap_or_default();
            if lang.is_empty() {
                lang = get_language_code_from_url(url, langs);
            }
            if lang.is_empty() {
                lang = options.default_language.clone();
            }
            let mut m = Map::new();
            m.insert("url".into(), Value::str(url));
            if !lang.is_empty() {
                m.insert("lang".into(), Value::str(&lang));
                m.insert("label".into(), Value::str(get_native_language_name(&lang)));
            }
            filemap.entry(replaced).or_default().push(Value::from_map(m));
        }
    }
    for entries in filemap.values_mut() {
        entries.sort_by(|a, b| a.get("lang").to_output_string().cmp(&b.get("lang").to_output_string()));
    }
    let mut url_map: IndexMap<String, Vec<Value>> = IndexMap::new();
    for entries in filemap.values() {
        for entry in entries {
            let url = entry.get("url").to_output_string();
            url_map.entry(url.clone()).or_insert_with(|| {
                entries
                    .iter()
                    .filter(|e| e.get("lang").is_truthy() || e.get("url").to_output_string() != url)
                    .cloned()
                    .collect()
            });
        }
    }
    url_map
}

#[derive(Default)]
struct ContentMaps {
    input_path_to_url: Value,
    url_to_input_path: Value,
    locale_urls_map: IndexMap<String, Vec<Value>>,
}

/// State shared with `get*CollectionItem` filters (via the `locale_page` filter)
pub fn resolve_root_page(ctx: &CallContext, handle: &ConfigHandle, page: &Value, lang: &Value) -> Value {
    let Some(cfg) = handle.get() else {
        return page.clone();
    };
    let Some(filter) = cfg.get_filter("locale_page") else {
        return page.clone();
    };
    filter(ctx, page.clone(), &[lang.clone()]).unwrap_or_else(|_| page.clone())
}

fn get_root_collection(ctx: &CallContext, handle: &ConfigHandle, collection: &Value) -> Value {
    let mut filtered = vec![];
    for item in collection.to_vec() {
        let page = item.get("data").get("page");
        if !page.is_truthy()
            || resolve_root_page(ctx, handle, &page, &Value::Undefined)
                .get("url")
                .strict_equals(&page.get("url"))
        {
            filtered.push(item);
        }
    }
    Value::array(filtered)
}

pub fn get_collection_item_index(ctx: &CallContext, handle: &ConfigHandle, collection: &Value, page: &Value) -> Value {
    let root = get_root_collection(ctx, handle, collection);
    let root_page = resolve_root_page(ctx, handle, page, &Value::Undefined);
    find_collection_item_index(&root, &root_page)
        .map(Value::from)
        .unwrap_or_default()
}

/// `Filters/GetLocaleCollectionItem.js`
pub fn get_locale_collection_item(
    ctx: &CallContext,
    handle: &ConfigHandle,
    collection: &Value,
    page_override: &Value,
    lang_code: &Value,
    modifier: i64,
) -> Value {
    let lang_code = if lang_code.is_truthy() {
        lang_code.clone()
    } else {
        ctx.page().get("lang")
    };
    let root_page = resolve_root_page(ctx, handle, page_override, &Value::Undefined);
    let root_collection = get_root_collection(ctx, handle, collection);
    let modified_root_item = get_collection_item(&root_collection, &root_page, modifier);
    if !modified_root_item.is_truthy() {
        return Value::Undefined;
    }
    let modified_locale_page =
        resolve_root_page(ctx, handle, &modified_root_item.get("data").get("page"), &lang_code);
    if !modified_locale_page.has("__locale_page_resolved") {
        return modified_root_item;
    }
    let all = ctx.collections().get("all");
    get_collection_item(&all, &modified_locale_page, 0)
}

pub fn plugin(config: &mut UserConfig, opts: &Value) -> Result<()> {
    let mut options = I18nOptions {
        default_language: opts.get("defaultLanguage").as_str().unwrap_or("").to_string(),
        languages: None,
        url_filter: opts.get("filters").get("url").as_str().unwrap_or("locale_url").to_string(),
        links_filter: opts.get("filters").get("links").as_str().unwrap_or("locale_links").to_string(),
        error_mode: opts.get("errorMode").as_str().unwrap_or("strict").to_string(),
    };
    match opts.get("languages") {
        Value::Undefined => {}
        Value::Array(a) => {
            options.languages = Some(a.iter().map(|v| v.to_output_string()).collect());
        }
        _ => bail!("The `languages` option in Eleventy’s Internationalization (I18N) plugin must be an array of language codes."),
    }
    if options.default_language.is_empty() {
        options.default_language = options
            .languages
            .as_ref()
            .and_then(|l| l.first().cloned())
            .unwrap_or_default();
    }
    if options.default_language.is_empty() {
        bail!("You must specify a `defaultLanguage` (or `languages`) in Eleventy’s Internationalization (I18N) plugin.");
    }
    if let Some(langs) = &options.languages {
        let mut all = vec![options.default_language.clone()];
        for l in langs {
            if !all.contains(l) {
                all.push(l.clone());
            }
        }
        options.languages = Some(all);
    }
    let options = Arc::new(options);

    let extension_remover: Arc<RwLock<Option<Arc<dyn Fn(&str) -> String + Send + Sync>>>> =
        Arc::new(RwLock::new(None));
    let er = extension_remover.clone();
    config.on("buildawesome.extensionmap", move |arg| {
        if let Value::Function(f) = arg.get("removeTemplateExtension") {
            let f = f.clone();
            *er.write().unwrap() = Some(Arc::new(move |p: &str| {
                f(&[Value::str(p)]).map(|v| v.to_output_string()).unwrap_or_else(|_| p.to_string())
            }));
        }
        Ok(())
    });

    let maps: Arc<RwLock<ContentMaps>> = Arc::new(RwLock::new(ContentMaps::default()));
    let (m, o, er) = (maps.clone(), options.clone(), extension_remover.clone());
    config.on("buildawesome.contentmap", move |arg| {
        let remover = er.read().unwrap().clone();
        let remove = |p: &str| -> String {
            match &remover {
                Some(f) => f(p),
                None => p.to_string(),
            }
        };
        let url_to_input_path = arg.get("urlToInputPath");
        let locale = get_locale_urls_map(&url_to_input_path, &remove, &o);
        let mut maps = m.write().unwrap();
        maps.input_path_to_url = arg.get("inputPathToUrl");
        maps.url_to_input_path = url_to_input_path;
        maps.locale_urls_map = locale;
        Ok(())
    });

    let o = options.clone();
    config.add_global_data(
        "eleventyComputed.page.lang",
        Value::function(move |_| {
            let o = o.clone();
            Ok(Value::function(move |args| {
                let data = args.first().cloned().unwrap_or_default();
                let url = data.get("page").get("url").to_output_string();
                let code = get_language_code_from_url(&url, o.languages.as_deref());
                Ok(Value::str(if code.is_empty() { o.default_language.clone() } else { code }))
            }))
        }),
    );

    let (m, o) = (maps.clone(), options.clone());
    let url_filter_name = options.url_filter.clone();
    config.add_filter(&url_filter_name, move |ctx, original_url, args| {
        let original = original_url.to_output_string();
        let mut url = original.clone();
        let mut suffix = String::new();
        if url.starts_with('/') && !url.starts_with("//") {
            if let Ok(parsed) = url::Url::parse("http://localhost/").and_then(|b| b.join(&url)) {
                url = percent_encoding::percent_decode_str(parsed.path()).decode_utf8_lossy().to_string();
                suffix = format!(
                    "{}{}",
                    // Empty `?`/`#` are dropped (like `URL#search` and `URL#hash`)
                    parsed.query().filter(|q| !q.is_empty()).map(|q| format!("?{q}")).unwrap_or_default(),
                    parsed.fragment().filter(|f| !f.is_empty()).map(|f| format!("#{f}")).unwrap_or_default()
                );
            }
        }
        let langs = o.languages.as_deref();
        let lang_code = match args.first() {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => {
                let c = get_language_code_from_url(&ctx.page().get("url").to_output_string(), langs);
                if c.is_empty() { o.default_language.clone() } else { c }
            }
        };
        let maps = m.read().unwrap();
        let entries = maps
            .locale_urls_map
            .get(&url)
            .or_else(|| if !url.ends_with('/') { maps.locale_urls_map.get(&format!("{url}/")) } else { None });
        if let Some(entries) = entries {
            for e in entries {
                let eurl = e.get("url").to_output_string();
                if url_has_lang_code(&eurl, &lang_code, langs) {
                    return Ok(Value::str(eurl + &suffix));
                }
            }
        }
        let prepended = format!("/{lang_code}{url}");
        if maps.locale_urls_map.contains_key(&prepended)
            || (!prepended.ends_with('/') && maps.locale_urls_map.contains_key(&format!("{prepended}/")))
        {
            return Ok(Value::str(prepended + &suffix));
        }
        let exists = maps.url_to_input_path.get(&url).is_truthy()
            || (!url.ends_with('/') && maps.url_to_input_path.get(&format!("{url}/")).is_truthy());
        if exists {
            if o.error_mode == "strict" {
                bail!("Localized file for URL {prepended} was not found in your project. A non-localized version does exist—are you sure you meant to use the `{}` filter for this? You can bypass this error using the `errorMode` option in the I18N plugin (current value: \"{}\").", o.url_filter, o.error_mode);
            }
        } else if o.error_mode != "never" {
            bail!("Localized file for URL {prepended} was not found in your project! You will need to add it if you want to link to it using the `{}` filter. You can bypass this error using the `errorMode` option in the I18N plugin (current value: \"{}\").", o.url_filter, o.error_mode);
        }
        Ok(Value::str(original))
    });

    let m = maps.clone();
    config.add_filter(&options.links_filter.clone(), move |ctx, url_override, args| {
        let url = if url_override.is_truthy() {
            url_override.to_output_string()
        } else {
            ctx.page().get("url").to_output_string()
        };
        let include_current = args.first().map(|o| o.get("includeCurrent").is_truthy()).unwrap_or(false);
        let maps = m.read().unwrap();
        let entries = maps.locale_urls_map.get(&url).cloned().unwrap_or_default();
        if include_current {
            return Ok(Value::array(entries));
        }
        Ok(Value::array(
            entries
                .into_iter()
                .filter(|e| e.get("url").to_output_string() != url)
                .collect(),
        ))
    });

    let (m, o) = (maps.clone(), options.clone());
    config.add_filter("locale_page", move |ctx, page_override, args| {
        let lang = match args.first() {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => o.default_language.clone(),
        };
        let page = if page_override.is_truthy() { page_override } else { ctx.page() };
        let maps = m.read().unwrap();
        let page_url = page.get("url").to_output_string();
        let mut url = None;
        if let Some(entries) = maps.locale_urls_map.get(&page_url) {
            for e in entries {
                if e.get("lang").as_str() == Some(lang.as_str()) {
                    url = Some(e.get("url"));
                }
            }
        }
        let langs = o.languages.as_deref();
        let input_path = swap_language_code(&page.get("inputPath").to_output_string(), &lang, langs);
        let urls = maps.input_path_to_url.get(&input_path);
        let Some(url) = url else {
            return Ok(page);
        };
        if !urls.is_array() || urls.len() == 0 {
            return Ok(page);
        }
        let mut result = Map::new();
        result.insert("url".into(), url);
        result.insert("inputPath".into(), Value::str(input_path));
        result.insert(
            "filePathStem".into(),
            Value::str(swap_language_code(&page.get("filePathStem").to_output_string(), &lang, langs)),
        );
        result.insert("__locale_page_resolved".into(), Value::Bool(true));
        Ok(Value::from_map(result))
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_codes() {
        assert!(is_lang_code("en", None));
        assert!(is_lang_code("en-US", None));
        assert!(is_lang_code("zh-Hant-TW", None));
        assert!(!is_lang_code("posts", None));
        assert!(!is_lang_code("xx", None));
        assert!(!is_lang_code("EN", None));
    }
}
