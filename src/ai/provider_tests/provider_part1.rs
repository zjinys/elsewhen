use super::*;

#[test]
fn openai_compatible_config_has_defaults() {
    let config = OpenAiCompatibleConfig::default();
    assert_eq!(config.base_url, "https://api.openai.com/v1");
    assert_eq!(config.model, "gpt-3.5-turbo");
    assert_eq!(config.temperature, 0.7);
}

#[test]
fn ollama_config_has_defaults() {
    let config = OllamaConfig::default();
    assert_eq!(config.base_url, "http://localhost:11434");
    assert_eq!(config.model, "llama2");
    assert_eq!(config.temperature, 0.7);
}

#[test]
fn openai_provider_can_be_created() {
    let config = OpenAiCompatibleConfig::default();
    let provider = OpenAiCompatibleProvider::new(config);
    assert!(provider.is_ok());
}

#[test]
fn ollama_provider_can_be_created() {
    let config = OllamaConfig::default();
    let provider = OllamaProvider::new(config);
    assert!(provider.is_ok());
}

#[test]
fn prepared_request_preflight_reserves_output_for_both_transports() {
    let messages = vec![
        ContextMessage::new("system", "fixed contract"),
        ContextMessage::new("user", "中文事件".repeat(400)),
    ];
    // 未知模型输出预留固定为 default_output = 16384；用两个窗口验证
    // 预算判定：32K 窗口放得下，17408 窗口（预留后只剩几百 token）放不下。
    for window in [32768, 17408] {
        let expected = window == 32768;
        let openai = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            model: "unknown-budget-model".into(),
            context_window: Some(window),
            ..Default::default()
        })
        .unwrap();
        let ollama = OllamaProvider::new(OllamaConfig {
            model: "unknown-budget-model".into(),
            context_window: Some(window),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(openai.fits_request(&messages).unwrap(), expected);
        assert_eq!(ollama.fits_request(&messages).unwrap(), expected);
    }
}

#[test]
fn input_headroom_reports_what_the_next_request_can_still_hold() {
    let small = vec![
        ContextMessage::new("system", "fixed contract"),
        ContextMessage::new("user", "你好"),
    ];
    let mut large = small.clone();
    large.push(ContextMessage::new("user", "长".repeat(1000)));
    let mut overflow = small.clone();
    overflow.push(ContextMessage::new("user", "长".repeat(20_000)));
    let openai = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        model: "unknown-budget-model".into(),
        context_window: Some(32768),
        ..Default::default()
    })
    .unwrap()
    .with_output_reserve(8192);
    let ollama = OllamaProvider::new(OllamaConfig {
        model: "unknown-budget-model".into(),
        context_window: Some(32768),
        ..Default::default()
    })
    .unwrap()
    .with_output_reserve(8192);
    let limit = crate::ai::budget::input_limit(32768, 8192).unwrap();
    for provider in [&openai as &dyn AiProvider, &ollama] {
        let (headroom, model) = provider.input_headroom(&small, None).unwrap().unwrap();
        assert_eq!(model, "unknown-budget-model");
        assert!(headroom < limit && headroom > limit - 400, "{headroom}");
        // 1000 个汉字按启发式约 2240 token，余量相应减少。
        let (smaller, _) = provider.input_headroom(&large, None).unwrap().unwrap();
        assert!(
            (2240..2400).contains(&(headroom - smaller)),
            "{}",
            headroom - smaller
        );
        // 放不下时是 0，不下溢。
        assert_eq!(
            provider.input_headroom(&overflow, None).unwrap().unwrap().0,
            0
        );
    }
    // 与补充材料的预检同一口径：放得下 512 的余量才接受。
    assert!(openai.accepts_supplement(&small, None).unwrap());
    assert!(!openai.accepts_supplement(&overflow, None).unwrap());

    // 没有传输预算的 provider 不报余量，调用方按不限制处理。
    struct Plain;
    impl AiProvider for Plain {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            Ok(AiReply::text(""))
        }
    }
    assert!(Plain.input_headroom(&small, None).unwrap().is_none());
}
