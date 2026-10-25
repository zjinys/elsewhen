//! `deliver` 状态机（R6-1）与错误码分类（R6-2）。
//!
//! 投递的**真相只有一份**：`sync_wire_outbox` 各列。这个模块是**读侧视图**，
//! 把列翻译成一个人能读的状态，而不是再存一份 state 列 —— 两个真相必然打架
//! （列说 backoff、时间说早该重试），而打架时谁也说不清哪个算数。
//!
//! ```text
//! idle → uploading → awaiting_receipt → done
//!                        ↓
//!              backoff(reason, until) → blocked(reason)
//! ```

use crate::storage::Store;

/// 一次投递的对外状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeliverState {
    /// 没有待投递的信封：无事可做。
    Idle,
    /// 有 pending 行但还没上过 relay（`relay_id IS NULL`）。
    Uploading,
    /// 已上 relay（`relay_id` 非空），等对端回执。
    AwaitingReceipt,
    /// 退避中：`available_at_ms > now`，`reason` 是分类码（见 `DeliveryFailure`）。
    Backoff { reason: String, until_ms: i64 },
    /// 阻塞：需要人看（配额满 / 协议错位）。
    ///
    /// `until_ms` 是「到点会再试一次」的时刻，不是「等人工解除」：阻塞的语义是
    /// 「这条路的节奏不对，放慢到人看得见」，不是停摆。面板要能说清「配额满，
    /// 15 秒后再试」而不是干说「配额满」（m2）。
    Blocked { reason: String, until_ms: i64 },
}

impl DeliverState {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Uploading => "uploading",
            Self::AwaitingReceipt => "awaiting_receipt",
            Self::Backoff { .. } => "backoff",
            Self::Blocked { .. } => "blocked",
        }
    }
}

/// 失败分类。设计稿 §3.5 的五类，一个不多一个不少。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryFailure {
    /// 507：配额满。**不是**「停止重试」——R2-4 实测 15 s 后配额通常已回落，
    /// 停下就等于永久停摆，所以是 `blocked` 展示 + 15 s 后自动再试。
    Quota,
    /// 413 一类：不该发生（实现 bug 或两端上限不同步）。上 UI，10 分钟后才再试
    /// 一次 —— 快速重试对它毫无意义，只会把毒信循环伪装成「正在重试」。
    Protocol,
    /// 409：nonce / revision 撞了，内部重放。重取 stamp 后单次重试，不进状态机。
    Replay,
    /// 5xx / 网络 / 超时：指数退避，上限 10 分钟。
    Transport,
    /// 摘要 / AEAD 不符：丢片重取，**只退避，永远不升级成 `blocked`**。
    ///
    /// （改前这里写的是「累计 3 次升级为 blocked」，但那件事从来没被实现 ——
    /// 计数逻辑不存在，`is_blocking()` 也不含它。摘要对不上通常意味着我们自己
    /// 发坏了，重发才是对的答案，没有「该让用户处理」的状态。）
    Integrity,
}

impl DeliveryFailure {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Quota => "relay_quota",
            Self::Protocol => "protocol",
            Self::Replay => "replay",
            Self::Transport => "transport",
            Self::Integrity => "integrity",
        }
    }

    /// 这类失败该不该进 `blocked`（要人看），还是老老实实退避。
    ///
    /// 调用方必须真的据此写 `blocked_reason`（见 `Store::block_sync_wire`）：
    /// 一个只有读侧、没有写侧的开关就是死代码（review M3）。
    pub(crate) fn is_blocking(&self) -> bool {
        matches!(self, Self::Quota | Self::Protocol)
    }
}

/// 本仓库的错误是 stringly typed（`anyhow` + 文本 context），因此按错误链
/// 文本分类，而不是 downcast。分类只用于**决定退避与展示**，绝不据此丢弃数据：
/// 分错了最坏结果是重试节奏不好，而不是丢信。
pub(crate) fn classify_delivery_failure(error: &anyhow::Error) -> DeliveryFailure {
    if error.is::<crate::sync::transport_relay::RelayQuotaExceeded>() {
        return DeliveryFailure::Quota;
    }
    let text = format!("{error:#}");
    let lower = text.to_lowercase();
    if lower.contains("relay_quota") || text.contains("507") {
        return DeliveryFailure::Quota;
    }
    if lower.contains("too large") || lower.contains("413") || text.contains("sync_relay_payload_too_large") {
        return DeliveryFailure::Protocol;
    }
    if lower.contains("data integrity") || lower.contains("digest") || text.contains("sync_wire_payload_mismatch") {
        return DeliveryFailure::Integrity;
    }
    if lower.contains("nonce") || lower.contains("revision changed") || lower.contains("409") {
        return DeliveryFailure::Replay;
    }
    DeliveryFailure::Transport
}

/// 退避时长：`2^attempts` 秒起，按分类封顶。
///
/// 上限 10 分钟（设计稿）而不是现在的 60 秒：抖动期的网络失败如果 60 秒就
/// 重来一轮，每拍都在打 relay，日志与电量都不划算；而真正该快的失败走它们
/// 自己的分支，不受这里的指数曲线影响：`Quota` 15 s、`Replay` **1 s 上限**
/// （它照样进 backoff —— 「内部重放单次重试」是旧口径，实际是重放类失败落
/// 一条 1 s 后退避的行，下一次 tick 再试）。
pub(crate) fn backoff_ms(failure: DeliveryFailure, attempts: i64) -> i64 {
    let cap = match failure {
        DeliveryFailure::Quota => crate::sync::engine::QUOTA_BACKOFF_MS,
        DeliveryFailure::Replay => 1_000,
        DeliveryFailure::Protocol => 10 * 60 * 1_000,
        DeliveryFailure::Transport | DeliveryFailure::Integrity => 10 * 60 * 1_000,
    };
    // 阻塞类**不看 attempts**，直接给定时长：`Protocol` 原来给 0，配上
    // `block_sync_wire` 就是「标成阻塞、下一拍立刻重试」，等于没 block。
    // 指数退避是给「可能马上就好」的失败用的（网络抖动），不是给结构性错位的。
    if failure.is_blocking() {
        return cap;
    }
    (1_000_i64 << attempts.clamp(0, 10)).min(cap)
}

/// 结构化投递日志（R6-3）。`ELSEWHEN_DEBUG` 门控，**只输出 subject 与长度**：
/// 密文、明文、密钥、远端响应体一概不出现 —— 日志最容易变成泄密通道，
/// 而 subject 与长度这两项已经足够定位「哪一封信卡在哪一步」。
pub(crate) fn wire_log(event: &str, subject: &str, bytes: usize, detail: &str) {
    wire_emit(wire_line(event, subject, bytes, detail));
}

/// 把已经按契约排好的行交给门控 + stderr。汇总类日志（多封信一行）没有单一
/// `subject`/`bytes` 可填，只能在外面先把行拼好（`push_summary_line`）。
pub(crate) fn wire_emit(line: String) {
    if std::env::var_os("ELSEWHEN_DEBUG").is_none() {
        return;
    }
    eprintln!("{line}");
}

/// 日志行本身（与门控分开，才能被断言）。
///
/// `detail` 里的 `=` 会被换成 `:`：断言「一行恰好四个字段」靠数 `=`，而 detail
/// 常常是错误文本（`key=value`、URL query）。不在这里统一处理，就得让每个调用点
/// 各自记住不许带 `=` —— 那正是「同一逻辑写两遍」的开头。
pub(crate) fn wire_line(event: &str, subject: &str, bytes: usize, detail: &str) -> String {
    let detail = detail.replace('=', ":");
    format!("[wire] event={event} subject={subject} bytes={bytes} detail={detail}")
}

/// 已送出（拿到 `relay_id`）之后到下一轮重发之间的最小间隔。
///
/// 这不是退避：信已经在中继上了，重发只是为了兜住「响应丢了」的极端情况，
/// 因此只需压住「每拍都把同一封再发一遍」的浪费（`seq_hint` 去重让它不致命，
/// 但白烧配额与带宽）。
///
/// 上限 **5 分钟**（原来 60 s）：成功路径的唯一含义就是「这封信已在中继上」，
/// 正常退役靠**对端的回执**而不是靠重发。60 s 对一条本就不该被重发的信令太密
/// ——`snapshot-start` 当初就因此每分钟重投一次而且永远退不掉（review B1）。
/// 真丢了响应五分钟內也能补上，代价远小于每分钟撞一次中继。
pub(crate) fn resend_spacing_ms(attempts: i64) -> i64 {
    // `clamp` 的上界是**位移量**，不是时长：原来 clamp(0,6) 最多只能到 64 s，
    // 真正在起作用的是 `min(60_000)` 那一行 —— 只把上限改成 5 分钟而不同步
    // 放开位移量，等于什么也没改（这条就是被回归用例抓住的）。
    (1_000_i64 << attempts.clamp(0, 9)).min(5 * 60_000)
}

/// 一个对端的投递视图（给配对面板与设置页）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeliverStatus {
    pub peer_device_id: String,
    pub state: DeliverState,
    /// pending 行数（还没被回执确认的信封）。
    pub pending: usize,
    /// 其中已上 relay、只等回执的数量。
    pub awaiting_receipt: usize,
}

impl Store {
    /// 每个对端一行：状态由 outbox 现有列推导，不落新列。
///
/// 注意 backoff 的判定多一个 `last_error <> ''`：**「已送出、等下一轮重发」的
/// 最小间隔不是退避**（见 `resend_spacing_ms`）。只看时间会把刚发出的信
/// 显示成「正在重试」，那正是状态机要消灭的那种假话。
    pub(crate) fn deliver_statuses(&self, now_ms: i64) -> anyhow::Result<Vec<DeliverStatus>> {
        let mut stmt = self.connection.prepare(
            "SELECT recipient_device_id, COUNT(*),
                    SUM(CASE WHEN relay_id IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN blocked_reason IS NOT NULL THEN 1 ELSE 0 END),
                    SUM(CASE WHEN available_at_ms > ?1 AND last_error <> '' THEN 1 ELSE 0 END),
                    MIN(CASE WHEN blocked_reason IS NULL THEN available_at_ms END),
                    MAX(NULLIF(blocked_reason,'')),
                    MAX(NULLIF(last_error,'')),
                    MIN(CASE WHEN blocked_reason IS NOT NULL THEN available_at_ms END)
             FROM sync_wire_outbox WHERE state='pending'
             GROUP BY recipient_device_id ORDER BY recipient_device_id",
        )?;
        let rows = stmt.query_map([now_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<i64>>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (
                peer,
                pending,
                on_relay,
                blocked,
                backoff,
                until_ms,
                blocked_reason,
                last_error,
                blocked_until,
            ) = row?;
            // blocked 的原因用 `blocked_reason`（具体到「在等空间」），
            // backoff 的原因用 `last_error`（分类码）—— 两者混用会让面板
            // 把「配额满」显示成「网络抖动」，那正是要避免的误导。
            let state = if blocked > 0 {
                DeliverState::Blocked {
                    reason: blocked_reason.unwrap_or_else(|| "blocked".into()),
                    // 阻塞行也要给「到点会再试」的时刻：面板显示「配额满，15 秒后
                    // 再试」比干说「配额满」有用得多，且不必让用户以为要人工干预。
                    until_ms: blocked_until.unwrap_or(now_ms),
                }
            } else if backoff > 0 {
                DeliverState::Backoff {
                    reason: last_error.unwrap_or_else(|| "transport".into()),
                    until_ms: until_ms.unwrap_or(now_ms),
                }
            } else if on_relay >= pending {
                DeliverState::AwaitingReceipt
            } else {
                DeliverState::Uploading
            };
            out.push(DeliverStatus {
                peer_device_id: peer,
                state,
                pending: pending as usize,
                awaiting_receipt: on_relay as usize,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_kind_maps_to_its_own_state() {
        let cases = [
            ("relay_quota exceeded", DeliveryFailure::Quota),
            ("relay said 507 insufficient quota", DeliveryFailure::Quota),
            ("chunk too large for relay (413)", DeliveryFailure::Protocol),
            ("payload digest mismatch", DeliveryFailure::Integrity),
            ("nonce already used", DeliveryFailure::Replay),
            ("connection reset by peer", DeliveryFailure::Transport),
        ];
        for (text, expected) in cases {
            let error = anyhow::anyhow!(text.to_string());
            assert_eq!(
                classify_delivery_failure(&error),
                expected,
                "分类错会让退避节奏与面板文案一起错：{text}"
            );
        }
    }

    #[test]
    fn backoff_grows_then_stops_at_ten_minutes() {
        let mut previous = 0;
        for attempts in 0..14 {
            let delay = backoff_ms(DeliveryFailure::Transport, attempts);
            assert!(delay >= previous, "退避必须单调不减（attempts={attempts}）");
            assert!(
                delay <= 10 * 60 * 1_000,
                "网络抖动不该把重试推到十分钟以上（attempts={attempts}）"
            );
            previous = delay;
        }
        assert_eq!(previous, 10 * 60 * 1_000);
        // 该快的各自封顶，不受 10 分钟上限影响。
        assert_eq!(backoff_ms(DeliveryFailure::Quota, 9), 15_000);
        assert_eq!(backoff_ms(DeliveryFailure::Replay, 9), 1_000);
    }

    /// 阻塞类不看 attempts，直接给定时长。
    ///
    /// `Protocol` 曾经给 0：配上 `block_sync_wire` 就是「标成阻塞、下一拍立刻
    /// 重试」，等于没 block —— 而这正是 review M3 说 `blocked` 名不副实的一半。
    #[test]
    fn blocking_failures_get_a_fixed_delay_not_exponential() {
        for attempts in [0_i64, 1, 5, 40] {
            assert_eq!(
                backoff_ms(DeliveryFailure::Protocol, attempts),
                10 * 60 * 1_000,
                "协议错位必须放慢到十分钟（attempts={attempts}）"
            );
            assert_eq!(
                backoff_ms(DeliveryFailure::Quota, attempts),
                15_000,
                "配额靠对端 ack 腾空间，等太久等于白停（attempts={attempts}）"
            );
        }
        // 非阻塞类仍是指数退避：第一次只等 1 秒，别把小抖动拖成十分钟。
        assert_eq!(backoff_ms(DeliveryFailure::Transport, 0), 1_000);
        assert_eq!(backoff_ms(DeliveryFailure::Integrity, 0), 1_000);
        assert!(
            !DeliveryFailure::Integrity.is_blocking(),
            "摘要不符是我们自己发坏了，重发才对；把它标成阻塞等于甩锅给用户"
        );
    }

    #[test]
    fn log_line_carries_subject_and_length_but_never_payload() {
        // 一条日志要能回答「哪一封信、多大、卡在哪」，而**不能**成为泄密通道：
        // 载荷/密钥/远端响应体一律不进来，因此这里断言行里只有这四个字段。
        let line = wire_line("push", "delta-9f2c", 1188, "relay_quota");
        assert_eq!(
            line,
            "[wire] event=push subject=delta-9f2c bytes=1188 detail=relay_quota"
        );
        assert_eq!(line.matches('=').count(), 4, "多一个字段就是多一处泄密面");
    }

    #[test]
    fn detail_never_adds_a_fifth_field() {
        // detail 常是错误文本或 URL（`key=value`、`?a=1`），而「四个字段」是靠数 `=`
        // 守的。净化必须发生在这一处，否则每个调用点都得自己记得不许带 `=`。
        let line = wire_line(
            "blob-recycle",
            "9f2c1ab0",
            0,
            "deferred quic_connect=timeout",
        );
        assert_eq!(
            line,
            "[wire] event=blob-recycle subject=9f2c1ab0 bytes=0 detail=deferred quic_connect:timeout"
        );
        assert_eq!(line.matches('=').count(), 4, "detail 里的 `=` 是伪字段");
    }

    #[test]
    fn stderr_log_lines_only_come_from_wire_line() {
        // 「只有一个日志格式」靠约定守不住：m8 就是四处裸 `eprintln!` 各自排版，
        // 结果谁也不知道该在哪里加字段（多一个 `=` 就多一处泄密面）。
        // 这里直接扫源码：同步链路里除本文件外一律不许出现 `eprintln!`。
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for directory in [root.join("sync"), root.join("storage/sync")] {
            collect_eprintln(&directory, &root, &mut offenders);
        }
        assert!(
            offenders.is_empty(),
            "只用 wire_log 打同步日志，不要另开格式：{offenders:?}"
        );
    }

    /// 递归找 `eprintln!`（跳过测试文件——测试断言 stderr 不算日志格式）。
    fn collect_eprintln(directory: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 测试与 e2e harness 打 stderr/stdout 是断言素材，不是生产日志格式。
                if path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("test"))
                {
                    continue;
                }
                collect_eprintln(&path, root, out);
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !name.ends_with(".rs") || name.contains("test") {
                continue;
            }
            if path == std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sync/engine/deliver.rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if text.contains("eprintln!") {
                out.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }

    #[test]
    fn only_quota_and_protocol_are_blocking() {
        // 「要人看」与「等着就好」必须区分开：把网络抖动也标 blocked，
        // 面板会天天报警，然后就没人看了。
        assert!(DeliveryFailure::Quota.is_blocking());
        assert!(DeliveryFailure::Protocol.is_blocking());
        assert!(!DeliveryFailure::Transport.is_blocking());
        assert!(!DeliveryFailure::Integrity.is_blocking());
        assert!(!DeliveryFailure::Replay.is_blocking());
    }
}
