//! Translate application-owned descriptions only. Tool identities, schemas and
//! third-party text are retained exactly, including unknown future properties.
use super::{ToolRegistry, ToolSpec};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Description {
    description: String,
    parameters: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub(super) struct DescriptionCatalog(BTreeMap<String, Description>);

impl ToolRegistry {
    pub(crate) fn configure_language(&mut self, store: &crate::storage::Store) -> Result<()> {
        let body = store.prompt_for_run("tool.descriptions.fixed_contract")?;
        let mut catalog: DescriptionCatalog =
            serde_json::from_str(&body).context("tool_locale_resource_invalid")?;
        let metadata = crate::storage::prompts::render::TextResources::parse(
            &store.prompt_for_run("tool.metadata.fixed_contract")?,
        )?;
        metadata.validate()?;
        ensure!(
            catalog.0.len() == self.tools.len(),
            "tool_locale_catalog_mismatch"
        );
        for tool in &self.tools {
            let translated = catalog.0.get(tool.name()).context("tool_locale_missing")?;
            ensure!(
                !translated.description.trim().is_empty(),
                "tool_description_empty"
            );
            let schema = tool.parameters_schema();
            let mut pointers = BTreeMap::new();
            collect_descriptions(&schema, "", &mut pointers);
            ensure!(
                translated
                    .parameters
                    .keys()
                    .all(|key| pointers.contains_key(key))
                    && pointers
                        .keys()
                        .all(|key| translated.parameters.contains_key(key)
                            || matches!(
                                (tool.name(), key.as_str()),
                                (
                                    "search_knowledge_base",
                                    "/properties/alternate_queries/description"
                                ) | (
                                    "save_wiki_revision",
                                    "/properties/source_slugs/description"
                                        | "/properties/include_current_event/description"
                                ) | (
                                    "save_knowledge_draft",
                                    "/properties/distinct_reason/description"
                                )
                            )),
                "tool_locale_parameters_mismatch:{}",
                tool.name()
            );
            ensure!(
                translated.parameters.values().all(|v| !v.trim().is_empty()),
                "tool_parameter_description_empty"
            );
        }
        // Historical run bundles predate these optional schema additions. Keep
        // their existing text frozen, filling only the newly introduced fields
        // from the same language's built-in resource.
        for (name, translated) in &mut catalog.0 {
            let english = !translated
                .description
                .chars()
                .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c));
            let current: DescriptionCatalog = serde_json::from_str(if english {
                include_str!("../../../resources/prompts/tool-descriptions.en.json")
            } else {
                include_str!("../../../resources/prompts/tool-descriptions.zh-Hans.json")
            })?;
            let schema = self
                .tools
                .iter()
                .find(|t| t.name() == name)
                .context("tool_locale_missing")?
                .parameters_schema();
            let mut pointers = BTreeMap::new();
            collect_descriptions(&schema, "", &mut pointers);
            for key in pointers.keys() {
                if !translated.parameters.contains_key(key) {
                    translated
                        .parameters
                        .insert(key.clone(), current.0[name].parameters[key].clone());
                }
            }
        }
        self.description_catalog = Some(catalog);
        self.metadata = Some(metadata);
        Ok(())
    }

    pub(super) fn meta_literal(&self, key: &str, fallback: &str) -> String {
        self.metadata.as_ref().map_or_else(
            || fallback.to_string(),
            |m| {
                m.literal(key)
                    .expect("registered tool metadata")
                    .to_string()
            },
        )
    }

    pub(super) fn meta_render(
        &self,
        key: &str,
        fallback: String,
        values: &[(&str, String)],
    ) -> String {
        self.metadata.as_ref().map_or(fallback, |m| {
            m.render(key, values)
                .expect("registered tool metadata variables")
        })
    }

    pub(super) fn builtin_spec(&self, tool: &dyn super::Tool) -> ToolSpec {
        let mut spec = ToolSpec {
            name: tool.name().into(),
            description: tool.description().into(),
            parameters: tool.parameters_schema(),
        };
        if let Some(catalog) = &self.description_catalog {
            // configure_language validates coverage once before the first request.
            let translated = &catalog.0[tool.name()];
            spec.description.clone_from(&translated.description);
            for (pointer, text) in &translated.parameters {
                *spec
                    .parameters
                    .pointer_mut(pointer)
                    .expect("validated tool description pointer") =
                    serde_json::Value::String(text.clone());
            }
        }
        spec
    }
}

fn collect_descriptions(value: &serde_json::Value, path: &str, out: &mut BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                if key == "description" {
                    out.insert(next, value.as_str().unwrap_or_default().into());
                } else {
                    collect_descriptions(value, &next, out);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                collect_descriptions(value, &format!("{path}/{i}"), out);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_descriptions(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("description");
                for v in map.values_mut() {
                    strip_descriptions(v);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    strip_descriptions(v);
                }
            }
            _ => (),
        }
    }

    #[test]
    fn language_changes_descriptions_without_changing_tool_authority_or_schema() {
        let _lock = crate::config::test_env_guard();
        crate::config::unpin();
        struct ClearPin;
        impl Drop for ClearPin {
            fn drop(&mut self) {
                crate::config::unpin();
            }
        }
        let _pin = ClearPin;
        let root = std::env::temp_dir().join(format!("tool-language-{}", uuid::Uuid::new_v4()));
        crate::config::AppConfig::pin(Some(root.to_str().unwrap())).unwrap();
        let path = root.join("elsewhen.db");
        let store = crate::storage::Store::open(&path).unwrap();
        store
            .save_prompt_language_preferences(Some("en"), "auto", None, 0)
            .unwrap();
        let _scope = store.prompt_scope("preview", "tools").unwrap();
        let original = ToolRegistry::default();
        let mut translated = ToolRegistry::default();
        translated.configure_language(&store).unwrap();
        let before = original.provider_specs();
        let after = translated.provider_specs();
        assert_eq!(before.len(), after.len());
        for (mut a, mut b) in before.into_iter().zip(after) {
            assert_eq!(a.name, b.name);
            assert_ne!(a.description, b.description, "{}", a.name);
            assert!(
                b.description.is_ascii()
                    || !b
                        .description
                        .chars()
                        .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
                    || b.description.contains("新对话")
            );
            strip_descriptions(&mut a.parameters);
            strip_descriptions(&mut b.parameters);
            assert_eq!(a.parameters, b.parameters, "{} schema", a.name);
            assert_eq!(
                original.get(&a.name).unwrap().policy(),
                translated.get(&a.name).unwrap().policy()
            );
        }
        let profile = translated
            .provider_specs()
            .into_iter()
            .find(|s| s.name == "update_personal_profile")
            .unwrap();
        assert!(profile.description.contains("profile/me"));
        assert!(profile.description.contains("explicitly stated"));
        assert_eq!(
            profile.parameters["properties"]["skills"]["description"],
            "Skills, experience and background; preserve existing valid facts"
        );
        let upstream = crate::ai::mcp::McpToolInfo {
            name: "mcp__fixture__read".into(),
            server: "fixture".into(),
            server_id: "fixture-id".into(),
            keywords: "用户配置的关联意图".into(),
            raw_name: "read".into(),
            description: "第三方原始说明 – original upstream text".into(),
            parameters: serde_json::json!({"type":"object","properties":{"原始字段":{"type":"string","description":"上游参数说明"}}}),
            read_only_hint: true,
        };
        translated.set_mcp_tools(vec![upstream.clone()]);
        let external = translated
            .provider_specs()
            .into_iter()
            .find(|s| s.name == upstream.name)
            .unwrap();
        assert_eq!(external.description, upstream.description);
        assert_eq!(external.parameters, upstream.parameters);
        let mut deferred = ToolRegistry::deferred_runtime();
        deferred.configure_language(&store).unwrap();
        deferred.set_mcp_tools(vec![upstream.clone()]);
        let meta = deferred.meta_spec().unwrap();
        assert!(
            meta.description
                .starts_with("Load one or more skill packages")
        );
        assert!(meta.description.contains("Goals and direction"));
        assert!(meta.description.contains(&upstream.description));
        assert!(!meta.description.contains("联网检索"));
        assert_eq!(
            meta.parameters["properties"]["skills"]["description"],
            "At least one skill/package ID from the roster above"
        );
        let mut enabled = std::collections::HashSet::new();
        let activation = deferred.enable_result(
            &["goal-management".into(), upstream.name.clone()],
            &mut enabled,
        );
        assert!(activation.contains("At most three goals are active"));
        assert!(activation.contains("Every change") || activation.contains("reviewed proposals"));
        assert!(activation.contains("Required target goal ID"));
        assert!(activation.contains(&upstream.description));
        assert!(activation.contains("上游参数说明"));
        assert!(enabled.contains("create_plan"));
        assert!(enabled.contains(&upstream.name));
        let prompt = deferred
            .localized_prompt_for_specs(&deferred.provider_specs_partitioned(false, &enabled));
        assert!(prompt.starts_with("Available tools (name: purpose)"));
        assert!(prompt.contains("Parameter JSON Schema:"));
        assert!(prompt.contains("上游参数说明"));
        for skill in super::super::BUILTIN_SKILLS {
            let output = deferred.enable_result(&[skill.id.into()], &mut enabled);
            assert!(output.contains(skill.id));
            assert!(!output.contains("使用要点"));
        }
        drop(_scope);
        drop(store);
        let _ = std::fs::remove_dir_all(root);
    }
}
