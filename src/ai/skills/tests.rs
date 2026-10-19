//! skills 状态与解析测试。

use super::*;

use super::*;

fn fixture_dir(tag: &str) -> (std::path::PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "elsewhen-skills-test-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let skills = dir.join("skills");
    std::fs::create_dir_all(skills.join("proxy-pick")).unwrap();
    (dir, skills)
}

#[test]
fn discovers_and_parses_frontmatter() {
    let (dir, skills) = fixture_dir("discover");
    std::fs::write(
            skills.join("proxy-pick/SKILL.md"),
            "---\nname: proxy-pick\ndescription: \"切换 Clash Verge 代理节点。\n测速与挑选。\"\n---\n\n# 指南\n步骤在此。",
        )
        .unwrap();
    let found = discover(&dir).unwrap();
    assert_eq!(found.len(), 1);
    let skill = &found[0];
    assert_eq!(skill.id, "proxy-pick");
    assert_eq!(skill.title, "proxy-pick");
    assert!(skill.brief.contains("切换 Clash"));
    let body = load_body(&dir, "proxy-pick").unwrap();
    assert!(body.contains("# 指南"));
    assert!(
        !body.contains("name: proxy-pick"),
        "frontmatter 不应进入正文"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn enable_toggle_roundtrip() {
    let (dir, skills) = fixture_dir("toggle");
    std::fs::write(skills.join("proxy-pick/SKILL.md"), "just body").unwrap();
    assert!(is_enabled(&dir, "proxy-pick"));
    set_enabled(&dir, "proxy-pick", false).unwrap();
    assert!(!is_enabled(&dir, "proxy-pick"));
    set_enabled(&dir, "proxy-pick", true).unwrap();
    assert!(is_enabled(&dir, "proxy-pick"));
    let _ = std::fs::remove_dir_all(&dir);
}
