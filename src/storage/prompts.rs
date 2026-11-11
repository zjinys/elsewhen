//! Local, append-only prompt revisions and immutable run manifests.
pub(crate) mod render;
use super::{Store, knowledge::content_hash};
use crate::language::runtime::{BundleLanguage, ResponsePolicy, response_policy};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

pub(crate) const LOCALE: &str = "zh-Hans";
const CATALOG: &str = "prompts-v14";
const MAX_BODY: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguagePreferences {
    pub instruction_locale: Option<String>,
    pub response_mode: String,
    pub response_locale: Option<String>,
    pub generation: i64,
}

impl Store {
    pub(crate) fn prompt_language_preferences(&self) -> Result<LanguagePreferences> {
        Ok(self.connection.query_row(
            "SELECT instruction_locale,response_mode,response_locale,generation FROM prompt_language_preferences WHERE singleton=1",
            [], |r| Ok(LanguagePreferences { instruction_locale: r.get(0)?, response_mode: r.get(1)?,
                response_locale: r.get(2)?, generation: r.get(3)? }),
        ).optional()?.unwrap_or(LanguagePreferences { instruction_locale: None,
            response_mode: "auto".into(), response_locale: None, generation: 0 }))
    }

    pub(crate) fn save_prompt_language_preferences(
        &self,
        instruction: Option<&str>,
        mode: &str,
        response: Option<&str>,
        expected: i64,
    ) -> Result<LanguagePreferences> {
        ensure!(expected >= 0, "language_preferences_generation_invalid");
        ensure!(
            (mode == "auto" && response.is_none()) || (mode == "fixed" && response.is_some()),
            "language_preferences_response_invalid"
        );
        let instruction = instruction.map(normalize).transpose()?;
        let response = response.map(normalize).transpose()?;
        let tx = self.begin_write_transaction()?;
        let changed = if expected == 0 {
            self.connection.execute(
                "INSERT OR IGNORE INTO prompt_language_preferences VALUES(1,?1,?2,?3,1,datetime('now'))",
                params![instruction, mode, response])?
        } else {
            self.connection.execute(
                "UPDATE prompt_language_preferences SET instruction_locale=?1,response_mode=?2,response_locale=?3,generation=generation+1,updated_at=datetime('now') WHERE singleton=1 AND generation=?4",
                params![instruction, mode, response, expected])?
        };
        ensure!(changed == 1, "language_preferences_conflict");
        let saved = self.prompt_language_preferences()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(saved)
    }
}
#[derive(Clone)]
pub(crate) struct Definition {
    pub key: &'static str,
    pub title: &'static str,
    pub body: &'static str,
    pub english_body: &'static str,
    pub editable: bool,
}
pub(crate) fn catalog() -> Vec<Definition> {
    vec![
        Definition {
            key: "chat.personal.policy",
            title: "promptPersonalPolicy",
            body: include_str!("../../resources/prompts/chat-policy.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/chat-policy.en.txt"),
            editable: true,
        },
        Definition {
            key: "chat.fixed_contract",
            title: "promptChatContract",
            body: include_str!("../../resources/prompts/chat-contract.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/chat-contract.en.txt"),
            editable: false,
        },
        Definition {
            key: "summary.fixed_contract",
            title: "promptSummaryContract",
            body: include_str!("../../resources/prompts/summary-contract.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/summary-contract.en.txt"),
            editable: false,
        },
        Definition {
            key: "reading.fixed_contract",
            title: "promptReadingContract",
            body: include_str!("../../resources/prompts/reading-contract.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/reading-contract.en.txt"),
            editable: false,
        },
        Definition {
            key: "digest.fixed_contract",
            title: "promptDigestContract",
            body: include_str!("../../resources/prompts/digest-contract.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/digest-contract.en.txt"),
            editable: false,
        },
        Definition {
            key: "summary.policy",
            title: "promptSummaryPolicy",
            body: include_str!("../../resources/prompts/summary-policy.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/summary-policy.en.txt"),
            editable: true,
        },
        Definition {
            key: "reading.policy",
            title: "promptReadingPolicy",
            body: include_str!("../../resources/prompts/reading-policy.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/reading-policy.en.txt"),
            editable: true,
        },
        Definition {
            key: "digest.policy",
            title: "promptDigestPolicy",
            body: include_str!("../../resources/prompts/digest-policy.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/digest-policy.en.txt"),
            editable: true,
        },
        Definition {
            key: "analysis.policy",
            title: "promptAnalysisPolicy",
            body: include_str!("../../resources/prompts/analysis-policy.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/analysis-policy.en.txt"),
            editable: true,
        },
        Definition {
            key: "compass.fixed_contract",
            title: "promptCompassContract",
            body: include_str!("../../resources/prompts/compass-contract.zh-Hans.txt"),
            english_body: include_str!("../../resources/prompts/compass-contract.en.txt"),
            editable: false,
        },
        Definition {
            key: "chat.context.fixed_contract",
            title: "promptContextContract",
            body: include_str!("../../resources/prompts/chat-context.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/chat-context.en.json"),
            editable: false,
        },
        Definition {
            key: "tool.descriptions.fixed_contract",
            title: "promptToolDescriptionsContract",
            body: include_str!("../../resources/prompts/tool-descriptions.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/tool-descriptions.en.json"),
            editable: false,
        },
        Definition {
            key: "tool.metadata.fixed_contract",
            title: "promptToolMetadataContract",
            body: include_str!("../../resources/prompts/tool-metadata.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/tool-metadata.en.json"),
            editable: false,
        },
        Definition {
            key: "chat.execution.fixed_contract",
            title: "promptExecutionContract",
            body: include_str!("../../resources/prompts/chat-execution.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/chat-execution.en.json"),
            editable: false,
        },
        Definition {
            key: "task.processing.fixed_contract",
            title: "promptProcessingContract",
            body: include_str!("../../resources/prompts/task-processing.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/task-processing.en.json"),
            editable: false,
        },
        Definition {
            key: "chat.reply.fixed_contract",
            title: "promptReplyContract",
            body: include_str!("../../resources/prompts/chat-reply.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/chat-reply.en.json"),
            editable: false,
        },
        Definition {
            key: "knowledge.processing.fixed_contract",
            title: "promptKnowledgeProcessingContract",
            body: include_str!("../../resources/prompts/knowledge-processing.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/knowledge-processing.en.json"),
            editable: false,
        },
        Definition {
            key: "chat.lookup.fixed_contract",
            title: "promptLookupContract",
            body: include_str!("../../resources/prompts/chat-lookup.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/chat-lookup.en.json"),
            editable: false,
        },
        Definition {
            key: "chat.supplement.fixed_contract",
            title: "promptSupplementContract",
            body: include_str!("../../resources/prompts/chat-supplement.zh-Hans.json"),
            english_body: include_str!("../../resources/prompts/chat-supplement.en.json"),
            editable: false,
        },
    ]
}
impl Definition {
    fn body_for(&self, locale: &str) -> Option<&'static str> {
        match locale {
            LOCALE => Some(self.body),
            "en" => Some(self.english_body),
            _ => None,
        }
    }

    fn builtin_id(&self, locale: &str) -> Result<String> {
        let body = self.body_for(locale).context("prompt_builtin_missing")?;
        Ok(format!(
            "builtin:{}:{locale}:{}",
            self.key,
            content_hash(body)
        ))
    }
}

#[derive(Clone)]
pub(crate) struct Revision {
    pub id: String,
    pub key: String,
    pub locale: String,
    pub body: String,
    pub origin: String,
    pub note: String,
    pub created_at: String,
}
#[derive(Clone)]
pub(crate) struct Selection {
    pub revision: Revision,
    pub generation: i64,
    pub fallback: String,
}
pub(crate) fn normalize(raw: &str) -> Result<String> {
    let raw = raw.trim().replace('_', "-");
    ensure!(!raw.is_empty() && raw.len() <= 64, "语言标签无效");
    let mut parts = Vec::new();
    for (i, p) in raw.split('-').enumerate() {
        ensure!(
            !p.is_empty() && p.len() <= 8 && p.bytes().all(|b| b.is_ascii_alphanumeric()),
            "语言标签无效"
        );
        parts.push(if i == 0 {
            ensure!(
                p.len() >= 2 && p.bytes().all(|b| b.is_ascii_alphabetic()),
                "语言标签无效"
            );
            p.to_ascii_lowercase()
        } else if p.len() == 4 {
            format!(
                "{}{}",
                p[..1].to_ascii_uppercase(),
                p[1..].to_ascii_lowercase()
            )
        } else {
            p.to_ascii_uppercase()
        });
    }
    let value = parts.join("-");
    Ok(match value.as_str() {
        "zh" | "zh-CN" | "zh-SG" => "zh-Hans".into(),
        "zh-TW" | "zh-HK" => "zh-Hant".into(),
        _ => value,
    })
}
fn definition(key: &str) -> Result<Definition> {
    catalog()
        .into_iter()
        .find(|d| d.key == key)
        .context("未注册的提示词")
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Revision> {
    Ok(Revision {
        id: r.get(0)?,
        key: r.get(1)?,
        locale: r.get(2)?,
        body: r.get(3)?,
        origin: r.get(4)?,
        note: r.get(5)?,
        created_at: r.get(6)?,
    })
}
impl Store {
    pub(crate) fn register_prompts(&self) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        for d in catalog() {
            self.connection.execute("INSERT INTO prompt_definitions(prompt_key,category,title_key,default_locale,contract_version,variables_version,variables_schema_json,editable,max_body_bytes,catalog_version) VALUES(?1,?2,?3,'zh-Hans',1,1,'{}',?4,8192,?5) ON CONFLICT(prompt_key) DO UPDATE SET category=excluded.category,title_key=excluded.title_key,editable=excluded.editable,catalog_version=excluded.catalog_version",params![d.key,if d.editable{"policy"}else{"fixed_contract"},d.title,d.editable,CATALOG])?;
            for locale in [LOCALE, "en"] {
                let body = d.body_for(locale).context("prompt_builtin_missing")?;
                let hash = content_hash(body);
                let id = d.builtin_id(locale)?;
                self.connection.execute(
                    "INSERT OR IGNORE INTO prompt_revisions(id,prompt_key,locale,origin,body,body_hash,contract_version,variables_version,builtin_pack_version,change_note,created_at) VALUES(?1,?2,?3,'builtin',?4,?5,1,1,?6,'builtin',datetime('now'))",
                    params![id, d.key, locale, body, hash, format!("{CATALOG}-{locale}-{hash}")],
                )?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    pub(crate) fn prompt_revision(&self, id: &str) -> Result<Revision> {
        self.connection.query_row("SELECT id,prompt_key,locale,body,origin,change_note,created_at FROM prompt_revisions WHERE id=?1",[id],row).optional()?.context("修订不存在")
    }
    pub(crate) fn prompt_history(
        &self,
        key: &str,
        locale: &str,
        offset: i64,
    ) -> Result<Vec<Revision>> {
        definition(key)?;
        let locale = normalize(locale)?;
        let mut q=self.connection.prepare("SELECT id,prompt_key,locale,body,origin,change_note,created_at FROM prompt_revisions WHERE prompt_key=?1 AND locale=?2 ORDER BY created_at DESC,rowid DESC LIMIT 30 OFFSET ?3")?;
        Ok(q.query_map(params![key, locale, offset.max(0)], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub(crate) fn resolve_prompt(&self, key: &str, locale: &str) -> Result<Selection> {
        self.resolve_prompt_inner(key, locale, true)
    }

    pub(crate) fn resolve_builtin_prompt(&self, key: &str, locale: &str) -> Result<Selection> {
        self.resolve_prompt_inner(key, locale, false)
    }

    fn resolve_prompt_inner(&self, key: &str, locale: &str, allow_user: bool) -> Result<Selection> {
        let d = definition(key)?;
        let locale = normalize(locale)?;
        let mut candidate = locale.clone();
        let mut builtin_only = !allow_user || !d.editable;
        let mut generation = 0;
        let mut reason = String::new();
        loop {
            let activation: Option<(String, Option<String>, i64)> = self.connection.query_row(
                "SELECT mode,revision_id,generation FROM prompt_activations WHERE prompt_key=?1 AND locale=?2",
                params![key, candidate], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).optional()?;
            if let Some((mode, id, g)) = activation {
                if candidate == locale {
                    generation = g;
                }
                if !builtin_only
                    && mode == "pinned"
                    && let Some(id) = id
                {
                    // A bad override must not cause an unrelated parent override to run.
                    // Read compatibility and body from one revision row.
                    let revision: Option<(Revision, String, i64, i64)> = self.connection.query_row(
                        "SELECT id,prompt_key,locale,body,origin,change_note,created_at,body_hash,contract_version,variables_version FROM prompt_revisions WHERE id=?1",
                        [&id], |r| Ok((row(r)?, r.get(7)?, r.get(8)?, r.get(9)?)),
                    ).optional()?;
                    if let Some((rev, hash, contract, variables)) = revision
                        && rev.key == key
                        && rev.locale == candidate
                        && hash == content_hash(&rev.body)
                        && contract == 1
                        && variables == 1
                    {
                        return Ok(Selection {
                            revision: rev,
                            generation,
                            fallback: if candidate == locale {
                                String::new()
                            } else {
                                format!("parent_locale:{locale}:{candidate}")
                            },
                        });
                    }
                    reason = "incompatible_revision".into();
                }
                // An explicit default blocks user overrides in every parent locale.
                builtin_only = true;
            }
            if d.body_for(&candidate).is_some() {
                if candidate != locale && reason.is_empty() {
                    reason = format!("parent_locale:{locale}:{candidate}");
                }
                return Ok(Selection {
                    revision: self.prompt_revision(&d.builtin_id(&candidate)?)?,
                    generation,
                    fallback: reason,
                });
            }
            match candidate.rsplit_once('-') {
                Some((parent, _)) => candidate = parent.to_string(),
                None => break,
            }
        }
        // The final default is always built in. In particular zh-Hant must never
        // inherit a user's zh-Hans instructions just because both are Chinese.
        Ok(Selection {
            revision: self.prompt_revision(&d.builtin_id(LOCALE)?)?,
            generation,
            fallback: if reason.is_empty() {
                format!("missing_translation:{locale}:{LOCALE}")
            } else {
                reason
            },
        })
    }
    pub(crate) fn save_prompt_revision(
        &self,
        key: &str,
        locale: &str,
        body: &str,
        parent: Option<&str>,
        note: &str,
    ) -> Result<String> {
        let d = definition(key)?;
        ensure!(d.editable, "协议提示词只读");
        let locale = normalize(locale)?;
        let body = body.replace("\r\n", "\n");
        ensure!(
            !body.trim().is_empty() && body.len() <= MAX_BODY,
            "正文不能为空，且最多8192字节"
        );
        ensure!(note.len() <= 1024, "备注过长");
        if let Some(parent) = parent {
            let p = self.prompt_revision(parent)?;
            ensure!(
                p.key == key && p.locale == locale,
                "父修订不属于当前提示词和语言"
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        let base = self.resolve_builtin_prompt(key, &locale)?.revision.id;
        self.connection.execute("INSERT INTO prompt_revisions(id,prompt_key,locale,origin,body,body_hash,contract_version,variables_version,parent_revision_id,base_default_revision_id,change_note,created_at) VALUES(?1,?2,?3,'user',?4,?5,1,1,?6,?7,?8,datetime('now'))",params![id,key,locale,body,content_hash(&body),parent,base,note])?;
        Ok(id)
    }
    pub(crate) fn activate_prompt(
        &self,
        key: &str,
        locale: &str,
        id: Option<&str>,
        expected: i64,
    ) -> Result<()> {
        let d = definition(key)?;
        ensure!(d.editable, "协议提示词只读");
        let locale = normalize(locale)?;
        let tx = self.begin_write_transaction()?;
        if let Some(id) = id {
            let r = self.prompt_revision(id)?;
            ensure!(r.key == key && r.locale == locale, "修订语言或 key 不匹配");
            let hash: String = self.connection.query_row(
                "SELECT body_hash FROM prompt_revisions WHERE id=?1 AND contract_version=1 AND variables_version=1",
                [id],
                |r| r.get(0),
            )?;
            ensure!(hash == content_hash(&r.body), "修订校验失败");
        }
        let n=self.connection.execute("INSERT INTO prompt_activations(prompt_key,locale,mode,revision_id,generation,updated_at) SELECT ?1,?2,?3,?4,1,datetime('now') WHERE ?5=0 ON CONFLICT(prompt_key,locale) DO UPDATE SET mode=excluded.mode,revision_id=excluded.revision_id,generation=prompt_activations.generation+1,updated_at=excluded.updated_at WHERE prompt_activations.generation=?5",params![key,locale,if id.is_some(){"pinned"}else{"default"},id,expected])?;
        // INSERT SELECT WHERE expected=0 cannot update existing rows for nonzero generations.
        let n = if n == 0 && expected > 0 {
            self.connection.execute("UPDATE prompt_activations SET mode=?3,revision_id=?4,generation=generation+1,updated_at=datetime('now') WHERE prompt_key=?1 AND locale=?2 AND generation=?5",params![key,locale,if id.is_some(){"pinned"}else{"default"},id,expected])?
        } else {
            n
        };
        ensure!(n == 1, "提示词已被其他窗口修改，请刷新后重试");
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    pub(crate) fn capture_prompt_bundle(&self, kind: &str, owner: &str) -> Result<String> {
        self.capture_prompt_bundle_for_ui(kind, owner, &crate::language::runtime_ui_locale())
    }

    fn capture_prompt_bundle_for_ui(&self, kind: &str, owner: &str, ui: &str) -> Result<String> {
        self.capture_prompt_bundle_with_messages(kind, owner, ui, None)
    }

    fn capture_prompt_bundle_with_messages(
        &self,
        kind: &str,
        owner: &str,
        ui: &str,
        messages: Option<(&str, &[String])>,
    ) -> Result<String> {
        self.register_prompts()?;
        let tx = self.begin_write_transaction()?;
        if let Some(id) = self
            .connection
            .query_row(
                "SELECT id FROM prompt_bundles WHERE owner_kind=?1 AND owner_id=?2",
                params![kind, owner],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let mut language = self.prepare_bundle_language(kind, owner, ui)?;
        if let Some((current, earlier)) = messages {
            language.response = response_policy(
                &language.response.mode,
                language.response.requested_locale.as_deref(),
                current,
                earlier,
                ui,
            );
        }
        let selections = catalog()
            .iter()
            .map(|d| {
                let requested = if d.key == "chat.reply.fixed_contract"
                    && language.response.mode != "preserve_source"
                {
                    &language.response.locale
                } else if d.key.starts_with("chat.") || d.key.starts_with("tool.") {
                    &language.instruction_locale
                } else {
                    &language.background_instruction_locale
                };
                Ok((requested.clone(), self.resolve_prompt(d.key, requested)?))
            })
            .collect::<Result<Vec<_>>>()?;
        let fingerprint = content_hash(&serde_json::json!({
            "language": language, "renderer": "render-v2", "locale_policy": "locale-v2",
            "entries": selections.iter().map(|(requested, s)| {
                serde_json::json!([s.revision.key, requested, s.revision.locale, s.revision.id, s.fallback])
            }).collect::<Vec<_>>()
        }).to_string());
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO prompt_bundles VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'locale-v2','render-v2',?10,datetime('now'))",
            params![id, kind, owner, language.ui_locale, language.instruction_locale,
                serde_json::to_string(&language.response)?, language.knowledge_locale,
                language.knowledge_policy_version, CATALOG, fingerprint],
        )?;
        self.connection.execute(
            "INSERT INTO prompt_bundle_language_generations VALUES(?1,?2)",
            params![id, language.knowledge_policy_generation],
        )?;
        for (i, (requested, selection)) in selections.iter().enumerate() {
            self.connection.execute(
                "INSERT INTO prompt_bundle_entries VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    i as i64,
                    selection.revision.key,
                    requested,
                    selection.revision.locale,
                    selection.revision.id,
                    selection.fallback
                ],
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(id)
    }

    fn prepare_bundle_language(&self, kind: &str, owner: &str, ui: &str) -> Result<BundleLanguage> {
        let preference = self.prompt_language_preferences()?;
        let (knowledge_locale, knowledge_policy_version, knowledge_policy_generation): (String, i64, i64) = self.connection.query_row(
            "SELECT output_locale,policy_version,generation FROM dataset_content_language WHERE singleton=1",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let instruction_locale = preference.instruction_locale.clone().unwrap_or_else(|| {
            if kind == "background_task" {
                knowledge_locale.clone()
            } else {
                ui.to_string()
            }
        });
        let background_instruction_locale = preference
            .instruction_locale
            .unwrap_or_else(|| knowledge_locale.clone());
        let response = if kind == "chat_run" {
            let (conversation, input): (String, String) = self.connection.query_row(
                "SELECT conversation_id,input_message_id FROM chat_runs WHERE id=?1",
                [owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let history = self.context_for_input(&conversation, &input)?;
            let current = history
                .iter()
                .find(|m| m.id == input)
                .context("language_input_missing")?;
            let earlier = history
                .iter()
                .take_while(|m| m.id != input)
                .filter(|m| m.role == "user")
                .map(|m| m.content.clone())
                .collect::<Vec<_>>();
            response_policy(
                &preference.response_mode,
                preference.response_locale.as_deref(),
                &current.content,
                &earlier,
                ui,
            )
        } else if kind == "preview" && owner.starts_with("import_format:") {
            ResponsePolicy {
                mode: "preserve_source".into(),
                locale: "und".into(),
                source: "source_preservation".into(),
                fallback_locale: ui.into(),
                requested_locale: None,
                locale_resolution: None,
            }
        } else if kind == "background_task" {
            ResponsePolicy {
                mode: "knowledge".into(),
                locale: knowledge_locale.clone(),
                source: "dataset_policy".into(),
                fallback_locale: knowledge_locale.clone(),
                requested_locale: Some(knowledge_locale.clone()),
                locale_resolution: None,
            }
        } else {
            response_policy(
                &preference.response_mode,
                preference.response_locale.as_deref(),
                "",
                &[],
                ui,
            )
        };
        Ok(BundleLanguage {
            ui_locale: ui.into(),
            instruction_locale,
            background_instruction_locale,
            response,
            knowledge_locale,
            knowledge_policy_version,
            knowledge_policy_generation,
        })
    }

    pub(crate) fn run_language(&self) -> Result<BundleLanguage> {
        let bundle = if let Some(id) = self.prompt_bundle.borrow().clone() {
            Some(id)
        } else if let Some(run) = &self.chat_run_identity {
            Some(
                self.connection
                    .query_row(
                        "SELECT id FROM prompt_bundles WHERE owner_kind='chat_run' AND owner_id=?1",
                        [&run.id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .context("language_bundle_missing")?,
            )
        } else {
            None
        };
        if let Some(id) = bundle {
            return self.connection.query_row(
                "SELECT ui_locale,requested_instruction_locale,response_policy_json,knowledge_output_locale,content_language_policy_version FROM prompt_bundles WHERE id=?1",
                [&id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
            ).map_err(anyhow::Error::from).and_then(|(ui, instruction, response, knowledge, version)| {
                let background = self.connection.query_row(
                    "SELECT requested_locale FROM prompt_bundle_entries WHERE bundle_id=?1 AND prompt_key='summary.policy'",
                    [&id], |r| r.get(0),
                )?;
                let generation = self.connection.query_row(
                    "SELECT policy_generation FROM prompt_bundle_language_generations WHERE bundle_id=?1",
                    [&id], |r| r.get(0),
                ).optional()?.unwrap_or(0);
                Ok(BundleLanguage { ui_locale: ui, instruction_locale: instruction,
                    background_instruction_locale: background, response: serde_json::from_str(&response)?,
                    knowledge_locale: knowledge, knowledge_policy_version: version,
                    knowledge_policy_generation: generation })
            });
        }
        self.prepare_bundle_language("preview", "", &crate::language::runtime_ui_locale())
    }

    pub(crate) fn output_language_directive(&self, interactive: bool) -> Result<String> {
        let language = self.run_language()?;
        if !interactive {
            ensure!(
                super::knowledge_language::supported(
                    &language.knowledge_locale,
                    language.knowledge_policy_version
                ),
                "knowledge_language_unsupported"
            );
        }
        let locale = if interactive {
            &language.response.locale
        } else {
            &language.knowledge_locale
        };
        // The output strategy is independent from the language of instructions.
        // Original records, quotations, names, slugs and machine protocol remain data.
        Ok(format!(
            "\n[elsewhen-language-policy] Output language: {locale}. {} Preserve original user records, names, quotations, code, slugs and protocol keys. Do not translate source text or claim it was translated. An ordinary update to an existing page retains that page's language; an explicit translation is a reviewed derivative. The response language does not authorize any write. [end-elsewhen-language-policy]",
            if interactive {
                "Write the assistant's reply in this language. Follow an unambiguously stated language request in the current user message if this conservative policy did not parse it."
            } else {
                "Write generated natural-language fields in this language, while preserving the required JSON/schema exactly."
            }
        ))
    }

    pub(crate) fn prompt_for_run(&self, key: &str) -> Result<String> {
        if let Some(bundle) = self.prompt_bundle.borrow().as_ref() {
            return self.prompt_from_bundle(bundle, key);
        }
        if let Some(run) = &self.chat_run_identity {
            let bundle: Option<String> = self
                .connection
                .query_row(
                    "SELECT id FROM prompt_bundles WHERE owner_kind='chat_run' AND owner_id=?1",
                    [&run.id],
                    |r| r.get(0),
                )
                .optional()?;
            return self.prompt_from_bundle(&bundle.context("运行提示词版本未固定")?, key);
        }
        self.register_prompts()?;
        Ok(self.resolve_prompt(key, LOCALE)?.revision.body)
    }

    pub(crate) fn render_context_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("chat.context.fixed_contract")?)?
            .render(name, values)
    }

    pub(crate) fn render_execution_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("chat.execution.fixed_contract")?)?
            .render(name, values)
    }

    pub(crate) fn render_processing_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("task.processing.fixed_contract")?)?
            .render(name, values)
    }

    pub(crate) fn render_reply_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        let body = if self.prompt_bundle.borrow().is_some() || self.chat_run_identity.is_some() {
            self.prompt_for_run("chat.reply.fixed_contract")?
        } else {
            self.register_prompts()?;
            self.resolve_prompt(
                "chat.reply.fixed_contract",
                &self.run_language()?.response.locale,
            )?
            .revision
            .body
        };
        render::TextResources::parse(&body)?.render(name, values)
    }

    pub(crate) fn render_knowledge_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("knowledge.processing.fixed_contract")?)?
            .render(name, values)
    }

    pub(crate) fn render_lookup_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("chat.lookup.fixed_contract")?)?
            .render(name, values)
    }

    pub(crate) fn bind_background_prompt_bundle(&self, run: &str) -> Result<()> {
        let bundle = self
            .prompt_bundle
            .borrow()
            .clone()
            .context("language_bundle_missing")?;
        self.connection.execute(
            "UPDATE knowledge_background_runs SET prompt_bundle_id=?1 WHERE id=?2",
            params![bundle, run],
        )?;
        Ok(())
    }

    pub(crate) fn render_supplement_instruction(
        &self,
        name: &str,
        values: &[(&str, String)],
    ) -> Result<String> {
        render::TextResources::parse(&self.prompt_for_run("chat.supplement.fixed_contract")?)?
            .render(name, values)
    }
}

pub(crate) struct PromptScope<'a> {
    store: &'a Store,
    previous: Option<String>,
}
impl Drop for PromptScope<'_> {
    fn drop(&mut self) {
        *self.store.prompt_bundle.borrow_mut() = self.previous.take();
    }
}
impl Store {
    pub(crate) fn import_format_prompt_scope(&self) -> Result<PromptScope<'_>> {
        let id = self.capture_prompt_bundle(
            "preview",
            &format!("import_format:{}", uuid::Uuid::new_v4()),
        )?;
        Ok(self.use_prompt_bundle(&id))
    }

    pub(crate) fn content_chat_prompt_scope(
        &self,
        current: &str,
        earlier: &[String],
    ) -> Result<PromptScope<'_>> {
        let id = self.capture_prompt_bundle_with_messages(
            "preview",
            &format!("content_chat:{}", uuid::Uuid::new_v4()),
            &crate::language::runtime_ui_locale(),
            Some((current, earlier)),
        )?;
        Ok(self.use_prompt_bundle(&id))
    }

    pub(crate) fn prompt_scope(&self, kind: &str, owner: &str) -> Result<PromptScope<'_>> {
        let previous = self.prompt_bundle.borrow().clone();
        let id = if let Some(id) = &previous {
            id.clone()
        } else {
            self.capture_prompt_bundle(kind, owner)?
        };
        *self.prompt_bundle.borrow_mut() = Some(id);
        Ok(PromptScope {
            store: self,
            previous,
        })
    }
    pub(crate) fn prompt_dependency(&self, keys: &[&str]) -> Result<String> {
        let language = self.run_language()?;
        let values = keys
            .iter()
            .map(|key| {
                let body =
                    if self.prompt_bundle.borrow().is_some() || self.chat_run_identity.is_some() {
                        self.prompt_for_run(key)?
                    } else {
                        self.register_prompts()?;
                        self.resolve_prompt(key, &language.background_instruction_locale)?
                            .revision
                            .body
                    };
                Ok(format!("{key}:{}", content_hash(&body)))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(content_hash(&format!(
            "render-v2|locale-v2|{}|{}|{}|{}|{}",
            language.background_instruction_locale,
            language.knowledge_locale,
            language.knowledge_policy_version,
            language.knowledge_policy_generation,
            values.join("|")
        )))
    }
    pub(crate) fn prompt_from_bundle(&self, bundle: &str, key: &str) -> Result<String> {
        let body = self.connection.query_row("SELECT r.body FROM prompt_bundle_entries e JOIN prompt_revisions r ON r.id=e.revision_id WHERE e.bundle_id=?1 AND e.prompt_key=?2",params![bundle,key],|r|r.get(0)).optional()?;
        if body.is_some() {
            return Ok(body.unwrap());
        }
        // Catalogs v1/v2 predate the context resource. Keep their historical
        // Chinese instructions rather than borrowing a newly selected locale.
        // This compatibility resource is frozen independently of future packs.
        let catalog: String = self.connection.query_row(
            "SELECT catalog_version FROM prompt_bundles WHERE id=?1",
            [bundle],
            |r| r.get(0),
        )?;
        if key == "chat.context.fixed_contract"
            && matches!(catalog.as_str(), "prompts-v1" | "prompts-v2")
        {
            return Ok(include_str!("../../resources/prompts/chat-context.legacy-v1.json").into());
        }
        if key == "tool.descriptions.fixed_contract"
            && matches!(catalog.as_str(), "prompts-v1" | "prompts-v2" | "prompts-v3")
        {
            return Ok(
                include_str!("../../resources/prompts/tool-descriptions.legacy-v1.json").into(),
            );
        }
        if key == "tool.metadata.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1" | "prompts-v2" | "prompts-v3" | "prompts-v4"
            )
        {
            return Ok(include_str!("../../resources/prompts/tool-metadata.legacy-v1.json").into());
        }
        if key == "chat.execution.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1" | "prompts-v2" | "prompts-v3" | "prompts-v4" | "prompts-v5"
            )
        {
            return Ok(
                include_str!("../../resources/prompts/chat-execution.legacy-v1.json").into(),
            );
        }
        if key == "task.processing.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1" | "prompts-v2" | "prompts-v3" | "prompts-v4" | "prompts-v5"
            )
        {
            return Ok(
                include_str!("../../resources/prompts/task-processing.legacy-v1.json").into(),
            );
        }
        if key == "chat.reply.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1" | "prompts-v2" | "prompts-v3" | "prompts-v4" | "prompts-v5"
            )
        {
            return Ok(include_str!("../../resources/prompts/chat-reply.legacy-v1.json").into());
        }
        if key == "knowledge.processing.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1"
                    | "prompts-v2"
                    | "prompts-v3"
                    | "prompts-v4"
                    | "prompts-v5"
                    | "prompts-v6"
            )
        {
            return Ok(
                include_str!("../../resources/prompts/knowledge-processing.legacy-v1.json").into(),
            );
        }
        if key == "chat.lookup.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1"
                    | "prompts-v2"
                    | "prompts-v3"
                    | "prompts-v4"
                    | "prompts-v5"
                    | "prompts-v6"
                    | "prompts-v7"
            )
        {
            return Ok(include_str!("../../resources/prompts/chat-lookup.legacy-v1.json").into());
        }
        if key == "chat.supplement.fixed_contract"
            && matches!(
                catalog.as_str(),
                "prompts-v1"
                    | "prompts-v2"
                    | "prompts-v3"
                    | "prompts-v4"
                    | "prompts-v5"
                    | "prompts-v6"
                    | "prompts-v7"
                    | "prompts-v8"
            )
        {
            return Ok(
                include_str!("../../resources/prompts/chat-supplement.legacy-v1.json").into(),
            );
        }
        anyhow::bail!("运行集合缺少提示词")
    }
}
impl Store {
    pub(crate) fn use_prompt_bundle(&self, id: &str) -> PromptScope<'_> {
        let previous = self.prompt_bundle.replace(Some(id.to_string()));
        PromptScope {
            store: self,
            previous,
        }
    }
    pub(crate) fn reading_prompt_task(
        &self,
        snapshot: &str,
        version: &str,
    ) -> Result<(String, String)> {
        let generation = self.run_language()?.knowledge_policy_generation;
        let pinned = self.prompt_bundle.borrow().is_some() || self.chat_run_identity.is_some();
        if let Some(task)=self.connection.query_row("SELECT bundle_id,strategy FROM prompt_reading_tasks WHERE snapshot_id=?1 AND contract_version=?2 AND (?4=0 OR policy_generation=?3) ORDER BY policy_generation DESC LIMIT 1",params![snapshot,version,generation,pinned],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?{return Ok(task);}
        let tx = self.begin_write_transaction()?;
        let bundle = match self.prompt_bundle.borrow().clone() {
            Some(bundle) => bundle,
            None => self.capture_prompt_bundle(
                "background_task",
                &format!("reading:{snapshot}:{version}:{generation}"),
            )?,
        };
        let strategy = {
            let _scope = self.use_prompt_bundle(&bundle);
            format!(
                "{version}:{}",
                self.prompt_dependency(&[
                    "reading.policy",
                    "reading.fixed_contract",
                    "task.processing.fixed_contract"
                ])?
            )
        };
        self.connection.execute(
            "INSERT OR IGNORE INTO prompt_reading_tasks VALUES(?1,?2,?3,?4,?5)",
            params![snapshot, version, bundle, strategy, generation],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok((bundle, strategy))
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;

impl Store {
    pub(crate) fn reading_prompt_strategy(&self, snapshot: &str, version: &str) -> Result<String> {
        let generation = self.run_language()?.knowledge_policy_generation;
        // Existing completed readings remain usable as generated evidence. A
        // policy change alone never enrolls every historical source again.
        Ok(self.connection.query_row("SELECT strategy FROM prompt_reading_tasks WHERE snapshot_id=?1 AND contract_version=?2 ORDER BY (policy_generation=?3) DESC,policy_generation DESC LIMIT 1",params![snapshot,version,generation],|r|r.get(0)).optional()?.unwrap_or_else(||version.to_string()))
    }
}
