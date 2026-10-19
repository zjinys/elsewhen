use super::*;

#[test]
fn project_evidence_is_bounded_and_skips_sensitive_files() {
    let directory = temp_path("evidence-test");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("README.md"), "# Evidence fixture").unwrap();
    fs::write(
        directory.join("credentials.json"),
        "{\"private_key\": \"do-not-send\"}",
    )
    .unwrap();
    for index in 0..40 {
        fs::write(
            directory.join(format!("module-{index}.rs")),
            format!("fn item_{index}() {{}}\n{}", "x".repeat(1500)),
        )
        .unwrap();
    }

    let evidence =
        collect_project_evidence(&directory, &collect_project_files(&directory).unwrap().0)
            .unwrap();

    assert!(evidence.chars().count() <= MAX_AI_EVIDENCE_CHARS);
    assert!(evidence.matches("\n### ").count() <= MAX_AI_EVIDENCE_FILES);
    assert!(!evidence.contains("do-not-send"));
    assert!(evidence.contains("Evidence fixture"));
    let _ = fs::remove_dir_all(directory);
}
