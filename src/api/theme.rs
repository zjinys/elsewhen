//! 主题偏好 FRB 门面。

use super::*;
use anyhow::Result;
use crate::storage::Store;

/// 主题偏好 DTO（设置页「外观」：模式 + 预设 + 字体 + 正文字号；存 app_meta。
/// 后三项为编辑器内容区覆盖层，None = 跟随全局，见两层覆盖模型）
#[derive(Clone, Debug)]
pub struct ThemePrefsDto {
    /// "light" | "dark" | "system"
    pub mode: String,
    /// 预设名，如 "amber" | "indigo" | "aqua" | "violet"
    pub preset: String,
    /// 字体名，如 "inter" | "notoSansSc" | "notoSerifSc" | "system"
    pub font: String,
    /// 知识库正文字号（12.0–24.0，默认 16.0）
    pub font_size: f64,
    /// 编辑器（内容区）字体覆盖；None = 跟随全局
    pub editor_font: Option<String>,
    /// 编辑器（内容区）字号覆盖；None = 跟随全局
    pub editor_font_size: Option<f64>,
    /// 编辑器（内容区）行距覆盖；None = 跟随全局
    pub editor_line_height: Option<f64>,
}

/// 读取主题偏好（默认深色 + 琥珀 + Inter + 16px，保留现有观感；
/// 编辑器覆盖层无则 None，UI 回落全局）
pub fn get_theme_prefs() -> Result<ThemePrefsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let mode = store
        .get_meta("theme_mode")?
        .unwrap_or_else(|| "dark".to_string());
    let preset = store
        .get_meta("theme_preset")?
        .unwrap_or_else(|| "amber".to_string());
    let font = store
        .get_meta("theme_font")?
        .unwrap_or_else(|| "inter".to_string());
    let font_size = store
        .get_meta("theme_font_size")?
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(16.0);
    let editor_font = store.get_meta("theme_editor_font")?;
    let editor_font_size = store
        .get_meta("theme_editor_font_size")?
        .and_then(|v| v.parse::<f64>().ok());
    let editor_line_height = store
        .get_meta("theme_editor_line_height")?
        .and_then(|v| v.parse::<f64>().ok());
    Ok(ThemePrefsDto {
        mode,
        preset,
        font,
        font_size,
        editor_font,
        editor_font_size,
        editor_line_height,
    })
}

/// 保存主题偏好（设置页「外观」保存）。编辑器覆盖层参数 None = 跟随全局，
/// 会从 app_meta 删除对应键（覆盖态回归继承态）。
pub fn update_theme_prefs(
    mode: String,
    preset: String,
    font: String,
    font_size: f64,
    editor_font: Option<String>,
    editor_font_size: Option<f64>,
    editor_line_height: Option<f64>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_meta("theme_mode", &mode)?;
    store.set_meta("theme_preset", &preset)?;
    store.set_meta("theme_font", &font)?;
    store.set_meta("theme_font_size", &font_size.to_string())?;
    match editor_font {
        Some(f) => store.set_meta("theme_editor_font", &f)?,
        None => store.remove_meta("theme_editor_font")?,
    }
    match editor_font_size {
        Some(v) => store.set_meta("theme_editor_font_size", &v.to_string())?,
        None => store.remove_meta("theme_editor_font_size")?,
    }
    match editor_line_height {
        Some(v) => store.set_meta("theme_editor_line_height", &v.to_string())?,
        None => store.remove_meta("theme_editor_line_height")?,
    }
    Ok(())
}
