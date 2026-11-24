//! Port of `Util/DirContains.js`.

use super::node_path;

pub fn dir_contains(parent_folder: &str, sub_folder: &str) -> bool {
    let relative = node_path::relative(
        &node_path::resolve(&[parent_folder]),
        &node_path::resolve(&[sub_folder]),
    );
    relative != ".." && !relative.starts_with("../") && !node_path::is_absolute(&relative)
}
