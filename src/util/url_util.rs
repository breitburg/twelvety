//! Port of `Util/UrlUtil.js`.

/// `new URL(url)` succeeds
pub fn is_valid_url(url: &str) -> bool {
    url::Url::parse(url).is_ok()
}

pub fn get_directory_from_url(url: &str) -> String {
    if url.ends_with('/') {
        return url.to_string();
    }
    let mut parts: Vec<&str> = url.split('/').collect();
    parts.pop();
    format!("{}/", parts.join("/"))
}

/// Characters `encodeURI` leaves alone (besides ASCII alphanumerics)
const ENCODE_URI_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b';')
    .remove(b',')
    .remove(b'/')
    .remove(b'?')
    .remove(b':')
    .remove(b'@')
    .remove(b'&')
    .remove(b'=')
    .remove(b'+')
    .remove(b'$')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'#');

/// JavaScript’s `encodeURI`
pub fn encode_uri(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, ENCODE_URI_SET).to_string()
}
