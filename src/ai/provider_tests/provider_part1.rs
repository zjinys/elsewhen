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
