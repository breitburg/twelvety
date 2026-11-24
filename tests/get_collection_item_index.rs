//! Ported from upstream `test/GetCollectionItemIndexTest.js` (no i18n plugin: `page.lang` is unset).

use twelvety::Value;
use twelvety::filters::find_collection_item_index;
use twelvety::testing::json;

#[test]
fn get_collection_item_index_test() {
    let first = json(r#"{"inputPath": "hello.md", "outputPath": "/hello/"}"#);
    let second = json(r#"{"inputPath": "hello2.md", "outputPath": "/hello2/"}"#);
    let third = json(r#"{"inputPath": "hello3.md", "outputPath": "/hello3/"}"#);
    let collections = Value::array(vec![first.clone(), second.clone(), third.clone()]);

    assert_eq!(find_collection_item_index(&collections, &first), Some(0));
    assert_eq!(find_collection_item_index(&collections, &second), Some(1));
    assert_eq!(find_collection_item_index(&collections, &third), Some(2));

    let unknown = json(r#"{"inputPath": "unknown.md", "outputPath": "/unknown/"}"#);
    assert_eq!(find_collection_item_index(&collections, &unknown), None);
}
