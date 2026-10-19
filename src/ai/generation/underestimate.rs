//! 用量低估锚定：真实>启发式时记录进程内比值缓存。

/// usage 锚定（proposal 2026-10-19-token-estimation-calibration §3.0）：
/// 键=(conversation_id, model)，值=(真实 prompt_tokens, 当时启发式估)。
/// 只在真实 > 启发式（低估）时记录；进程内缓存，重启即回落启发式——
/// 锚定是优化不是安全依赖，宁高不低红线不受影响。
static USAGE_UNDERESTIMATE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<(String, String), (u64, usize)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

pub(crate) fn record_underestimate(
    conversation_id: &str,
    model: &str,
    real: u64,
    heuristic: usize,
) {
    if model.is_empty() || heuristic == 0 || real <= heuristic as u64 {
        return;
    }
    // 会话级单槽：只保留最近一次配对，模型换挡或翻篇后自然被新值覆盖。
    USAGE_UNDERESTIMATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            (conversation_id.to_string(), model.to_string()),
            (real, heuristic),
        );
}

/// 预算收紧系数：最近一次低估比（real/heuristic），clamp [1.0, 1.5]。
/// 比率越大说明启发式对该 provider 的分词偏差越大；无记录返回 1.0。
pub(crate) fn usage_underestimate_ratio(conversation_id: &str, model: &str) -> f64 {
    USAGE_UNDERESTIMATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(conversation_id.to_string(), model.to_string()))
        .map(|(real, heuristic)| (*real as f64 / *heuristic as f64).clamp(1.0, 1.5))
        .unwrap_or(1.0)
}

#[cfg(test)]
mod usage_anchor_tests {
    use super::*;

    #[test]
    fn anchor_records_only_underestimates_and_clamps() {
        // 高估（真实 < 启发式）不记录：宁高不低，锚定只负责上调修正。
        record_underestimate("conv-anchor", "m1", 100, 400);
        assert_eq!(usage_underestimate_ratio("conv-anchor", "m1"), 1.0);

        // 低估记录：真实 1500 / 启发式 1000 = 1.5。
        record_underestimate("conv-anchor", "m1", 1500, 1000);
        assert_eq!(usage_underestimate_ratio("conv-anchor", "m1"), 1.5);

        // 严重低估也 clamp 到 1.5（预算最多收紧三分之一）。
        record_underestimate("conv-anchor", "m2", 9000, 1000);
        assert_eq!(usage_underestimate_ratio("conv-anchor", "m2"), 1.5);

        // 轻微低估落在区间内且按原比率生效。
        record_underestimate("conv-anchor", "m3", 1050, 1000);
        assert!((usage_underestimate_ratio("conv-anchor", "m3") - 1.05).abs() < 1e-9);

        // 其他会话/模型不受影响。
        assert_eq!(usage_underestimate_ratio("conv-other", "m1"), 1.0);
    }
}
