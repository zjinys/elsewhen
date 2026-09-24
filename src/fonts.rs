//! 跨平台系统字体枚举（fontdb）。
//!
//! 替代旧的 `fc-list` 方案：`fc-list` 只在 Linux 可用，且要 spawn 外部命令；
//! fontdb 直接扫描字体数据库——Linux 解析 fontconfig 配置（/etc/fonts/*.conf
//! 及用户目录），macOS / Windows 扫描标准系统字体目录——三平台一份代码、
//! 零外部进程。fontdb 0.23 已随 iced 进依赖树，本模块只把它提为直接依赖。

use fontdb::{Database, Source, Style, Weight};

/// 返回系统字体字面列表 `(family, file, style)`，由桥层（api）聚合成 DTO。
///
/// 每张字面（bold/italic/light…）一条；同一家族的普通/粗体/斜体字面保留，
/// 家族级去重与首选样式排序交给 Dart 侧既有逻辑（`_styleRank` 优先 Regular）。
pub fn system_font_faces() -> Vec<(String, String, String)> {
    let mut db = Database::new();
    db.load_system_fonts();

    db.faces()
        .filter_map(|face| {
            // 只保留文件字面（fontdb 也可能持有内存字面，跳过）。
            let Source::File(path) = &face.source else {
                return None;
            };
            let family = face.families.first().map(|f| f.0.clone())?;
            Some((
                family,
                path.to_string_lossy().to_string(),
                style_label(face.style, face.weight),
            ))
        })
        .collect()
}

/// 把 fontdb 的结构化样式转成 fc-list 风格标签串，Dart 侧 `_styleRank` 可直接复用。
fn style_label(style: Style, weight: Weight) -> String {
    let weight_name = match weight.0 {
        0..=250 => "Thin",
        251..=350 => "Light",
        351..=450 => "Regular",
        451..=550 => "Medium",
        551..=650 => "SemiBold",
        651..=750 => "Bold",
        751..=850 => "ExtraBold",
        _ => "Black",
    };
    match style {
        Style::Normal => weight_name.to_string(),
        Style::Italic => format!("{weight_name} Italic"),
        Style::Oblique => format!("{weight_name} Oblique"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_label_maps_weights_and_slants() {
        assert_eq!(style_label(Style::Normal, Weight(400)), "Regular");
        assert_eq!(style_label(Style::Normal, Weight(700)), "Bold");
        assert_eq!(style_label(Style::Italic, Weight(400)), "Regular Italic");
        assert_eq!(style_label(Style::Normal, Weight(500)), "Medium");
        assert_eq!(style_label(Style::Normal, Weight(300)), "Light");
        assert_eq!(style_label(Style::Normal, Weight(900)), "Black");
    }

    #[test]
    fn system_font_faces_returns_file_backed_faces() {
        let faces = system_font_faces();
        // 本机（CI/开发者机器）应能枚举到至少一个系统字体；
        // 极端精简容器里可能为空——只验证字段完整性与格式。
        for (family, file, style) in &faces {
            assert!(!family.is_empty());
            assert!(file.ends_with(".ttf") || file.ends_with(".otf") || file.contains('/'));
            assert!(!style.is_empty());
        }
    }
}