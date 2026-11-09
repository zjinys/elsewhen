//! Explicit user-requested goal reset; preserves unrelated records and emits deletions.
use super::Store;
use anyhow::Result;
impl Store {
    pub fn clear_goal_workspace_data(&self) -> Result<(usize, usize, usize)> {
        let tx = self.begin_write_transaction()?;
        let conversations = {
            let mut q=self.connection.prepare("SELECT id FROM conversations WHERE goal_id IS NOT NULL OR id='goal-studio' OR assistant_mode='goal_mentor'")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &conversations {
            // These legacy caches/feedback tables predate cascade deletion.
            for table in ["suggestion_feedback", "knowledge_generations"] {
                self.connection.execute(
                    &format!("DELETE FROM {table} WHERE conversation_id=?1"),
                    [id],
                )?;
            }
            self.set_conversation_archived(id, true)?;
            anyhow::ensure!(self.delete_archived_conversation(id)?, "目标会话删除失败");
        }
        let goals = {
            let mut q = self.connection.prepare("SELECT id FROM goals")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &goals {
            self.sync_delete_goal(id)?;
        }
        // Semantic compass records are local derived state. Remove their
        // references before deleting the goal workspace, in this transaction.
        for table in [
            "goal_compass_feedback",
            "goal_compass_nudges",
            "goal_compass_assessments",
        ] {
            self.connection
                .execute(&format!("DELETE FROM {table}"), [])?;
        }
        self.connection.execute("DELETE FROM goals", [])?;
        let plans = self.list_plans()?;
        for plan in &plans {
            self.delete_plan(&plan.id)?;
        }
        // Old reference metadata belongs to discarded plans; user profile remains.
        let references = {
            let mut q = self.connection.prepare(
                "SELECT slug FROM wiki_pages WHERE slug LIKE 'capability/goal-references-%'",
            )?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for slug in references {
            self.delete_wiki_page(&slug)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok((goals.len(), plans.len(), conversations.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_clears_goal_threads_and_keeps_unrelated_chat() {
        let s = Store::open(
            &std::env::temp_dir().join(format!("reset-goals-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();
        s.initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
            dataset_id: "test-reset".into(),
            device_id: "a".into(),
            stream_id: "stream-a".into(),
            processor_device_id: "a".into(),
        })
        .unwrap();
        let g = s
            .create_goal("test", crate::storage::GoalPhase::Near)
            .unwrap();
        s.create_goal_chat_conversation(&g.id, "goal").unwrap();
        s.ensure_goal_studio_conversation().unwrap();
        let other = s.create_conversation(Some("keep"), None).unwrap();
        let counts = s.clear_goal_workspace_data().unwrap();
        assert_eq!(counts.0, 1);
        assert_eq!(counts.2, 2);
        assert!(s.get_conversation(&other).unwrap().is_some());
        assert!(s.list_active_goals().unwrap().is_empty());
    }
    #[test]
    fn provider_named_reference_resolves_and_tracks_rotation() {
        let s = Store::open(
            &std::env::temp_dir().join(format!("provider-secret-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();
        s.save_named_secret("shared_provider", "Shared", "first-key")
            .unwrap();
        s.save_ai_provider_config(
            None,
            "test",
            "openai-compatible",
            "https://example.invalid",
            "model",
            "{{secret:shared_provider}}",
            0.7,
            None,
        )
        .unwrap();
        assert_eq!(
            s.list_ai_provider_configs_for_runtime().unwrap()[0].api_key,
            "first-key"
        );
        assert_eq!(
            s.active_ai_provider_config().unwrap().unwrap().api_key,
            "first-key"
        );
        s.save_named_secret("shared_provider", "Shared", "rotated")
            .unwrap();
        assert_eq!(
            s.list_ai_provider_configs_for_runtime().unwrap()[0].api_key,
            "rotated"
        );
        assert!(s.delete_named_secret("shared_provider", false).is_err());
    }
}
