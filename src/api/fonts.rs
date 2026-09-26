//! 系统字体枚举 FRB 门面。

// ---------------------------------------------------------------------------
// 系统字体枚举（fontdb，跨平台；替代 fc-list，见 src/fonts.rs）
// ---------------------------------------------------------------------------

/// 一条系统字体字面。Dart 侧按家族去重、保留首选样式（Regular 优先）。
#[derive(Debug, Clone)]
pub struct SystemFontFace {
    pub family: String,
    pub file: String,
    pub style: String,
}

/// 枚举系统字体（Linux/macOS/Windows；fontdb 直接扫描，不 spawn 外部命令）。
pub fn list_system_fonts() -> Vec<SystemFontFace> {
    crate::fonts::system_font_faces()
        .into_iter()
        .map(|(family, file, style)| SystemFontFace {
            family,
            file,
            style,
        })
        .collect()
}
