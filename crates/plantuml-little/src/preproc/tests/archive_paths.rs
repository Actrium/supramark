use super::{extract_archive_to_temp, make_temp_dir, write_zip};
use std::fs;
use zip::CompressionMethod;

#[test]
fn extraction_skips_parent_entries_and_continues() {
    let dir = make_temp_dir("archive-parents");
    let archive = dir.join("archive.zip");
    write_zip(
        &archive,
        CompressionMethod::Stored,
        &[
            ("../.", ""),
            (r"..\.", ""),
            ("valid.puml", "component Kept"),
        ],
    );

    let extracted = extract_archive_to_temp(&archive).unwrap();
    assert_eq!(fs::read_dir(&extracted).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(extracted.join("valid.puml")).unwrap(),
        "component Kept"
    );
    fs::remove_dir_all(extracted).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn extraction_rebases_rooted_entries_inside_its_directory() {
    let dir = make_temp_dir("archive-rooted");
    let archive = dir.join("archive.zip");
    let rooted = std::path::absolute(&dir).unwrap();
    // The rooted file entry names an existing fixture directory. Opening that
    // path as a file would fail if extraction used the name verbatim.
    write_zip(
        &archive,
        CompressionMethod::Stored,
        &[(rooted.to_str().unwrap(), "component Rooted")],
    );

    let extracted = extract_archive_to_temp(&archive).unwrap();
    let filesystem_root = rooted.ancestors().last().unwrap();
    let relative = rooted.strip_prefix(filesystem_root).unwrap();
    assert_eq!(
        fs::read_to_string(extracted.join(relative)).unwrap(),
        "component Rooted"
    );
    assert!(rooted.is_dir());
    fs::remove_dir_all(extracted).unwrap();
    fs::remove_dir_all(dir).unwrap();
}
