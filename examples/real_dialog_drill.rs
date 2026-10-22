//! 真实对话演练：走 DB-authoritative 全链路（provider 选择 → 装配 →
//! 工具/skill → 落库），在演练库副本上造真实流量，评估 P1 观测。
//!
//! 用法：cargo run --release --example real_dialog_drill -- [db路径]
//! 库要求：含真实 ai_provider_configs + provider_key_secrets（生产备份副本）。

use elsewhen::ai::conversation::{ConversationConfig, generate_conversation_reply};
use elsewhen::storage::Store;

fn main() -> anyhow::Result<()> {
    let db = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/elsewhen-dialog-drill.db".into());
    let store = Store::open(std::path::Path::new(&db))?;
    let providers = store.list_ai_provider_configs_for_runtime()?;
    println!("可用 provider：{} 个", providers.len());
    for p in &providers {
        println!("  - {} ({}, 窗口 {:?})", p.name, p.model, p.context_window);
    }

    let conversation = store.create_conversation(
        Some("【真实演练】对话链路压力（skill/工具混排）"),
        Some("general"),
    )?;
    println!("演练会话：{conversation}");

    let long_material = "这段素材讨论了长期记忆系统里『压缩表述但不压缩事实』的原则：摘要可以改写措辞、省略细节，\
         但关键事实（人名、日期、决定、未解决问题的状态）不得走样；被压缩掉的原始内容必须仍然可以回溯，\
         回溯不了的压缩等于销毁证据；因此每一次有损压缩都应该留下审计线索，让未来的自己能判断这则摘要还能不能信。";

    let rounds: Vec<(&str, String)> = vec![
        (
            "工具诱导",
            "帮我在知识库里找一下与「卖古籍」相关的条目，把要点摘出来给我，注明出自哪篇。".into(),
        ),
        (
            "追问深挖",
            "刚才找到的内容里，挑最核心的一条，说说它和我之前整理的其他条目有什么矛盾或互补。".into(),
        ),
        (
            "纯闲聊",
            "不查资料，就用你自己的话说说：为什么整理笔记这件事总是半途而废？两三句就好。".into(),
        ),
        (
            "长材料组织",
            "下面是一段素材，帮我压成三条要点并存进知识库草稿：\n\n".to_string() + &long_material.repeat(6),
        ),
        (
            "工具加收束",
            "把上面三条要点跟之前检索到的条目做个交叉对照，如果有重复或冲突请直接调整条目，然后告诉我最终动了哪几篇。".into(),
        ),
        (
            "总结",
            "用三句话总结这次演练对话我们做了什么、得出了什么。".into(),
        ),
    ];

    for (idx, (label, user_text)) in rounds.iter().enumerate() {
        let config = ConversationConfig::default();
        let started = std::time::Instant::now();
        store.send_message(&conversation, "user", user_text, None)?;
        let reply = generate_conversation_reply(&conversation, &store, Some(config))
            .map_err(|e| anyhow::anyhow!("第{}轮（{label}）生成失败：{e:#}", idx + 1))?;
        println!(
            "[{}/{}] {label}: {} 字回复，耗时 {:.1}s",
            idx + 1,
            rounds.len(),
            reply.chars().count(),
            started.elapsed().as_secs_f32()
        );
    }
    println!("演练完成，会话 id：{conversation}");
    Ok(())
}
