//! Ported from upstream `test/GetCollectionItemTest.js`.

use twelvety::Value;
use twelvety::filters::get_collection_item;
use twelvety::testing::json;

#[test]
fn get_collection_item_test() {
    let first = json(r#"{"inputPath": "hello.md", "outputPath": "/hello/"}"#);
    let second = json(r#"{"inputPath": "hello2.md", "outputPath": "/hello2/"}"#);
    let third = json(r#"{"inputPath": "hello3.md", "outputPath": "/hello3/"}"#);
    let collections = Value::array(vec![first.clone(), second.clone(), third.clone()]);

    assert_eq!(get_collection_item(&collections, &first, 0), first);
    assert_eq!(get_collection_item(&collections, &second, 0), second);
    assert_eq!(get_collection_item(&collections, &third, 0), third);

    assert_eq!(get_collection_item(&collections, &first, -1), Value::Undefined);
    assert_eq!(get_collection_item(&collections, &second, -1), first);
    assert_eq!(get_collection_item(&collections, &third, -1), second);

    assert_eq!(get_collection_item(&collections, &first, 1), second);
    assert_eq!(get_collection_item(&collections, &second, 1), third);
    assert_eq!(get_collection_item(&collections, &third, 1), Value::Undefined);
}
