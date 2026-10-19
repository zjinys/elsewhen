use super::*;

#[test]
fn project_scan_is_bounded_and_marks_truncation() {
    let directory = temp_path("scan-limit");
    fs::create_dir_all(&directory).unwrap();
    for index in 0..=MAX_PROJECT_SCAN_FILES {
        fs::write(directory.join(format!("file-{index}.rs")), "fn main() {}").unwrap();
    }

    let summary = analyze_project(&directory).unwrap();

    assert_eq!(summary.files, MAX_PROJECT_SCAN_FILES);
    assert!(summary.scan_truncated);
    assert!(summary.content_md.contains("候选清单达到上限：是"));
    let _ = fs::remove_dir_all(directory);
}
