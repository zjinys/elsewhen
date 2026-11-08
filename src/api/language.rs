//! Language preferences. UI selection is installation-local; AI preferences are
//! local to the dataset. Knowledge language is an owner-authenticated policy.
use crate::{config::AppConfig, storage::Store};
use anyhow::Result;

pub struct UiLanguagePreferenceDto {
    /// None means follow the OS, missing preferences default to zh-Hans.
    pub locale: Option<String>,
}

pub fn get_ui_language_preference() -> Result<UiLanguagePreferenceDto> {
    let config = AppConfig::load()?;
    Ok(UiLanguagePreferenceDto {
        locale: crate::language::read_ui_preference(&config.data_dir)?,
    })
}

pub fn update_ui_language_preference(locale: Option<String>) -> Result<UiLanguagePreferenceDto> {
    let config = AppConfig::load()?;
    Ok(UiLanguagePreferenceDto {
        locale: crate::language::write_ui_preference(&config.data_dir, locale.as_deref())?,
    })
}

pub fn set_runtime_ui_locale(locale: String) -> Result<()> {
    crate::language::set_runtime_ui_locale(&locale)
}

pub struct PromptLanguagePreferencesDto {
    pub instruction_locale: Option<String>,
    pub response_mode: String,
    pub response_locale: Option<String>,
    pub generation: i64,
}

impl From<crate::storage::prompts::LanguagePreferences> for PromptLanguagePreferencesDto {
    fn from(p: crate::storage::prompts::LanguagePreferences) -> Self {
        Self {
            instruction_locale: p.instruction_locale,
            response_mode: p.response_mode,
            response_locale: p.response_locale,
            generation: p.generation,
        }
    }
}

pub fn get_prompt_language_preferences() -> Result<PromptLanguagePreferencesDto> {
    let store = Store::open(&AppConfig::load()?.database_path)?;
    Ok(store.prompt_language_preferences()?.into())
}

pub fn update_prompt_language_preferences(
    instruction_locale: Option<String>,
    response_mode: String,
    response_locale: Option<String>,
    expected_generation: i64,
) -> Result<PromptLanguagePreferencesDto> {
    let store = Store::open(&AppConfig::load()?.database_path)?;
    Ok(store
        .save_prompt_language_preferences(
            instruction_locale.as_deref(),
            &response_mode,
            response_locale.as_deref(),
            expected_generation,
        )
        .map_err(crate::user_error::present)?
        .into())
}

pub struct KnowledgeLanguageDto {
    pub output_locale: String,
    pub policy_version: i64,
    pub generation: i64,
    pub supported: bool,
    pub can_edit: bool,
    pub edit_block_reason: Option<String>,
}

pub fn get_knowledge_language() -> Result<KnowledgeLanguageDto> {
    let store = Store::open(&AppConfig::load()?.database_path)?;
    knowledge_language_dto(&store)
}

fn knowledge_language_dto(store: &Store) -> Result<KnowledgeLanguageDto> {
    let policy = store.knowledge_language_policy()?;
    let blocker = store.knowledge_language_edit_blocker()?;
    Ok(KnowledgeLanguageDto {
        supported: crate::storage::knowledge_language::supported(
            &policy.output_locale,
            policy.policy_version,
        ),
        output_locale: policy.output_locale,
        policy_version: policy.policy_version,
        generation: policy.generation,
        can_edit: blocker.is_none(),
        edit_block_reason: blocker.map(str::to_string),
    })
}

pub fn update_knowledge_language(
    output_locale: String,
    expected_generation: i64,
) -> Result<KnowledgeLanguageDto> {
    update_knowledge_language_inner(&output_locale, expected_generation)
        .map_err(crate::user_error::present)
}

fn update_knowledge_language_inner(
    output_locale: &str,
    expected_generation: i64,
) -> Result<KnowledgeLanguageDto> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if store.sync_identity()?.is_none() {
        store.set_knowledge_language(output_locale, expected_generation)?;
    } else {
        crate::sync::runtime::handoff::change_knowledge_language(
            &config,
            &output_locale,
            expected_generation,
        )?;
    }
    knowledge_language_dto(&store)
}
