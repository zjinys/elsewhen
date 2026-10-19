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
