//! Durable, local semantic assessments. Network calls never run in this module.
use super::{Goal, GoalStatus, Store, knowledge::content_hash};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const POLICY: &str = "semantic-compass-v1";
const WINDOW_DAYS: i64 = 7;
const MAX_EVIDENCE: usize = 24;
const MAX_EXCERPT_CHARS: usize = 1000;
const LEASE_SECONDS: i64 = 600;
const REEVALUATE_SECONDS: i64 = 6 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompassEvidence {
    pub source_id: String,
    pub source_kind: String,
    pub occurred_at: String,
    pub day: String,
    pub excerpt: String,
    pub excerpt_start: usize,
    pub excerpt_end: usize,
    pub total_chars: usize,
    pub excerpt_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CompassGoalSnapshot {
    pub goal_id: String,
    pub content: String,
    pub phase: String,
    pub why: Option<String>,
    pub current_focus: Option<String>,
    pub blocker: Option<String>,
    pub done_criteria: Option<String>,
    pub plan_id: Option<String>,
    pub plan_context: Option<String>,
    pub goal_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CompassInput {
    pub policy_version: String,
    pub window_start: String,
    pub window_end: String,
    pub goal: CompassGoalSnapshot,
    pub evidence: Vec<CompassEvidence>,
    pub has_more_evidence: bool,
    pub feedback: Vec<CompassFeedbackInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CompassFeedbackInput {
    pub kind: String,
    pub note: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct CompassLease {
    pub id: String,
    pub input: CompassInput,
    pub fingerprint: String,
    pub bundle_id: String,
    pub generation: i64,
    pub token: String,
    pub authority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompassCitation {
    pub source_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompassVerdict {
    pub classification: String,
    pub confidence: f64,
    pub reason: String,
    pub question: String,
    pub citations: Vec<CompassCitation>,
    pub recommendations: Vec<String>,
}

impl CompassVerdict {
    pub(crate) fn validate(&self, input: &CompassInput) -> Result<()> {
        ensure!(
            matches!(
                self.classification.as_str(),
                "aligned" | "drifting" | "changed" | "insufficient"
            ),
            "compass_classification_invalid"
        );
        ensure!(
            self.confidence.is_finite() && (0.0..=1.0).contains(&self.confidence),
            "compass_confidence_invalid"
        );
        ensure!(
            !self.reason.trim().is_empty() && self.reason.chars().count() <= 400,
            "compass_reason_invalid"
        );
        ensure!(
            self.question.chars().count() <= 200
                && self.recommendations.len() <= 3
                && self
                    .recommendations
                    .iter()
                    .all(|s| !s.trim().is_empty() && s.chars().count() <= 200),
            "compass_advice_limit"
        );
        ensure!(self.citations.len() <= 8, "compass_citation_limit");
        let mut ids = HashSet::new();
        let mut days = HashSet::new();
        for citation in &self.citations {
            ensure!(
                ids.insert(&citation.source_id),
                "compass_duplicate_citation"
            );
            let evidence = input
                .evidence
                .iter()
                .find(|e| e.source_id == citation.source_id)
                .context("compass_unknown_citation")?;
            ensure!(
                (4..=400).contains(&citation.quote.chars().count())
                    && evidence.excerpt.contains(&citation.quote),
                "compass_quote_not_verbatim"
            );
            days.insert(&evidence.day);
        }
        if self.classification != "insufficient" {
            ensure!(!self.citations.is_empty(), "compass_evidence_required");
        }
        if self.classification == "drifting" {
            ensure!(
                days.len() >= 2 && self.confidence >= 0.8 && !self.question.trim().is_empty(),
                "compass_drift_evidence_insufficient"
            );
            ensure!(!input.has_more_evidence, "compass_incomplete_input");
            // A correction remains authoritative for this goal basis until the
            // user changes it. New records never silently override explicit feedback.
            ensure!(
                !input
                    .feedback
                    .iter()
                    .any(|f| matches!(f.kind.as_str(), "still_progressing" | "changed_direction")),
                "compass_user_correction_active"
            );
        }
        Ok(())
    }
}

impl CompassInput {
    /// Clock sampling does not create a paid task. Expiry of an actual record,
    /// a goal/plan change, feedback, or prompt dependency changes the identity.
    fn fingerprint(&self, dependency: &str) -> Result<String> {
        Ok(content_hash(
            &serde_json::json!([
                POLICY,
                dependency,
                self.goal,
                self.evidence,
                self.has_more_evidence,
                self.feedback
            ])
            .to_string(),
        ))
    }
}

impl Store {
    fn compass_tx(&self) -> Result<Transaction<'_>> {
        Ok(Transaction::new_unchecked(
            &self.connection,
            TransactionBehavior::Immediate,
        )?)
    }

    pub(crate) fn compass_authority_generation(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT generation FROM goal_compass_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn compass_goal_snapshot(&self, goal: &Goal) -> Result<CompassGoalSnapshot> {
        let plan_context = if let Some(id) = &goal.plan_id {
            self.get_plan(id)?.map(|p| serde_json::json!({"id":p.id,"intent":p.intent_text,"vision":p.vision,"assumptions":p.assumptions,"status":p.status.as_str(),"updated_at":p.updated_at}).to_string())
        } else {
            None
        };
        Ok(CompassGoalSnapshot {
            goal_id: goal.id.clone(),
            content: goal.content.clone(),
            phase: goal.phase.as_str().into(),
            why: goal.why.clone(),
            current_focus: goal.current_focus.clone(),
            blocker: goal.blocker.clone(),
            done_criteria: goal.done_criteria.clone(),
            plan_id: goal.plan_id.clone(),
            plan_context,
            goal_updated_at: goal.updated_at.clone(),
        })
    }

    pub(crate) fn build_compass_input(
        &self,
        goal: &Goal,
        now: DateTime<Utc>,
    ) -> Result<CompassInput> {
        let start = (now - Duration::days(WINDOW_DAYS))
            .max(DateTime::parse_from_rfc3339(&goal.updated_at)?.with_timezone(&Utc));
        // A captured user input has one authority: its event. A false event
        // classification must not re-enter through the linked message. Join
        // both capture relations, including record_event's tool-only path.
        let mut statement = self.connection.prepare(
            "SELECT id,at,text,kind FROM (
             SELECT e.id,e.occurred_at AS at,substr(e.raw_text,1,1001) AS text,'event' AS kind
             FROM events e WHERE julianday(e.occurred_at)>=julianday(?1) AND julianday(e.occurred_at)<=julianday(?2)
               AND e.source NOT IN ('assistant','memo','notebook')
               AND COALESCE((SELECT d.recordable FROM event_recordability_decisions d WHERE d.event_id=e.id ORDER BY d.created_at DESC,d.rowid DESC LIMIT 1),
                   (SELECT json_extract(a.result_json,'$.recordable') FROM event_analyses a WHERE a.event_id=e.id ORDER BY a.created_at DESC,a.id DESC LIMIT 1),1)=1
             UNION ALL
             SELECT m.id,m.created_at AS at,substr(m.content,1,1001) AS text,'user_message' AS kind FROM messages m
             WHERE m.role='user' AND julianday(m.created_at)>=julianday(?1) AND julianday(m.created_at)<=julianday(?2)
               AND NOT EXISTS(SELECT 1 FROM input_records i WHERE i.message_id=m.id AND (i.event_id IS NOT NULL OR i.source IN ('memo','notebook')))
               AND NOT EXISTS(SELECT 1 FROM chat_input_effects c WHERE c.input_message_id=m.id AND c.event_id IS NOT NULL)
             ) ORDER BY julianday(at) DESC,id LIMIT 65")?;
        let rows = statement
            .query_map(params![start.to_rfc3339(), now.to_rfc3339()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut seen = HashSet::new();
        let mut evidence = Vec::new();
        let mut has_more = rows.len() >= 65;
        for (id, at, preview, kind) in rows {
            let at = DateTime::parse_from_rfc3339(&at)?.with_timezone(&Utc);
            // Read at most one extra scalar to detect clipping. No unbounded
            // raw document is materialized just to decide whether it fits.
            let chars = preview.chars().count();
            if chars == 0 {
                continue;
            }
            let hash = content_hash(preview.trim());
            if !seen.insert(hash.clone()) {
                continue;
            }
            if evidence.len() >= MAX_EVIDENCE {
                has_more = true;
                break;
            }
            if chars > MAX_EXCERPT_CHARS {
                has_more = true;
            }
            let total_chars: i64 = if chars > MAX_EXCERPT_CHARS {
                self.connection.query_row(
                    if kind == "event" {
                        "SELECT length(raw_text) FROM events WHERE id=?1"
                    } else {
                        "SELECT length(content) FROM messages WHERE id=?1"
                    },
                    [&id],
                    |r| r.get(0),
                )?
            } else {
                chars as i64
            };
            let excerpt: String = preview.chars().take(MAX_EXCERPT_CHARS).collect();
            evidence.push(CompassEvidence {
                source_id: id,
                source_kind: kind,
                occurred_at: at.to_rfc3339(),
                day: at.with_timezone(&chrono::Local).date_naive().to_string(),
                excerpt_end: excerpt.chars().count(),
                excerpt,
                excerpt_start: 0,
                total_chars: total_chars as usize,
                excerpt_hash: hash,
            });
        }
        evidence.sort_by(|a, b| {
            a.occurred_at
                .cmp(&b.occurred_at)
                .then(a.source_id.cmp(&b.source_id))
        });
        let snapshot = self.compass_goal_snapshot(goal)?;
        // Corrections are scoped to the exact goal/plan basis that prompted
        // them, so a user-confirmed new action can start fresh.
        let mut statement = self.connection.prepare("SELECT f.kind,f.note,f.created_at FROM goal_compass_feedback f JOIN goal_compass_assessments a ON a.id=f.assessment_id WHERE a.goal_id=?1 AND json_extract(a.input_json,'$.goal')=json(?2) ORDER BY f.created_at DESC,f.rowid DESC LIMIT 8")?;
        let feedback = statement
            .query_map(params![goal.id, serde_json::to_string(&snapshot)?], |r| {
                Ok(CompassFeedbackInput {
                    kind: r.get(0)?,
                    note: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(CompassInput {
            policy_version: POLICY.into(),
            window_start: start.to_rfc3339(),
            window_end: now.to_rfc3339(),
            goal: snapshot,
            evidence,
            has_more_evidence: has_more,
            feedback,
        })
    }

    /// Claim at most one assessment globally. The immutable input and prompt
    /// bundle survive restart; a new timer cannot steal a healthy lease.
    pub(crate) fn claim_compass_assessment(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<CompassLease>> {
        if !self.background_processing_allowed()? {
            return Ok(None);
        }
        let dependency = self.prompt_dependency(&["compass.fixed_contract"])?;
        let tx = self.compass_tx()?;
        self.require_background_processor()?;
        let stamp = now.timestamp();
        let running: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM goal_compass_assessments WHERE status='running' AND lease_until>?1)",[stamp],|r|r.get(0))?;
        if running {
            return Ok(None);
        }
        tx.execute("UPDATE goal_compass_assessments SET status='pending',generation=generation+1,lease_token=NULL,lease_until=NULL WHERE status='running' AND lease_until<=?1",[stamp])?;
        if self.get_active_compass_nudge()?.is_some() {
            return Ok(None);
        }
        let mut goals = self.list_active_goals()?;
        let mut candidates = Vec::new();
        for goal in goals.drain(..) {
            if goal
                .current_focus
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
                || self.has_recent_compass_nudge_for_goal(&goal.id, 7)?
            {
                continue;
            }
            if let Some(id) = &goal.plan_id {
                if self.has_recent_compass_nudge_for_plan(id, 7)?
                    || self
                        .get_plan(id)?
                        .is_none_or(|p| p.status.as_str() != "active")
                {
                    continue;
                }
            }
            if now
                .signed_duration_since(
                    DateTime::parse_from_rfc3339(&goal.updated_at)?.with_timezone(&Utc),
                )
                .num_days()
                < 3
            {
                continue;
            }
            let latest: Option<i64> = tx.query_row(
                "SELECT MAX(created_at) FROM goal_compass_assessments WHERE goal_id=?1",
                [&goal.id],
                |r| r.get(0),
            )?;
            if latest.is_some_and(|t| stamp - t < REEVALUATE_SECONDS) {
                // A failed/expired task can retry according to its own backoff.
                let retry:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM goal_compass_assessments WHERE goal_id=?1 AND status IN ('pending','failed') AND available_at<=?2 AND attempts<3)",params![goal.id,stamp],|r|r.get(0))?;
                if !retry {
                    continue;
                }
            }
            candidates.push((latest.unwrap_or(0), goal));
        }
        candidates.sort_by_key(|(last, _)| *last);
        for (latest, goal) in candidates {
            let input = self.build_compass_input(&goal, now)?;
            if input.evidence.len() < 2 {
                continue;
            }
            let fingerprint = input.fingerprint(&dependency)?;
            let existing: Option<(String,String,String,i64,i64)> = tx.query_row("SELECT id,status,prompt_bundle_id,generation,attempts FROM goal_compass_assessments WHERE goal_id=?1 AND input_fingerprint=?2",params![goal.id,fingerprint],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            // An available retry for an older fingerprint must not bypass the
            // six-hour limit for a newly changed input.
            if existing.is_none() && latest != 0 && stamp - latest < REEVALUATE_SECONDS {
                continue;
            }
            if existing
                .as_ref()
                .is_some_and(|(_, status, _, _, attempts)| status == "succeeded" || *attempts >= 3)
            {
                continue;
            }
            let (id, bundle_id, generation) = if let Some((id, _, bundle, generation, _)) = existing
            {
                let available: i64 = tx.query_row(
                    "SELECT available_at FROM goal_compass_assessments WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                if available > stamp {
                    continue;
                }
                (id, bundle, generation + 1)
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let bundle =
                    self.capture_prompt_bundle("background_task", &format!("compass:{id}"))?;
                tx.execute("INSERT INTO goal_compass_assessments(id,goal_id,plan_id,input_fingerprint,input_json,prompt_bundle_id,authority_generation,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,0,?7,?7)",params![id,goal.id,goal.plan_id,fingerprint,serde_json::to_string(&input)?,bundle,stamp])?;
                (id, bundle, 1)
            };
            // Reusing a task reuses its captured window/input, never current
            // timer timestamps. Fingerprints exclude only sampled clock fields.
            let saved: String = tx.query_row(
                "SELECT input_json FROM goal_compass_assessments WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            let input = serde_json::from_str(&saved)?;
            let token = uuid::Uuid::new_v4().to_string();
            let authority = self.compass_authority_generation()?;
            tx.execute("UPDATE goal_compass_assessments SET status='running',generation=?2,attempts=attempts+1,lease_token=?3,lease_until=?4,authority_generation=?5,updated_at=?6 WHERE id=?1",params![id,generation,token,stamp+LEASE_SECONDS,authority,stamp])?;
            tx.commit()?;
            return Ok(Some(CompassLease {
                id,
                input,
                fingerprint,
                bundle_id,
                generation,
                token,
                authority,
            }));
        }
        tx.commit()?;
        Ok(None)
    }

    fn require_compass_lease(&self, lease: &CompassLease, now: i64) -> Result<()> {
        self.require_background_processor()?;
        let current:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM goal_compass_assessments WHERE id=?1 AND status='running' AND generation=?2 AND lease_token=?3 AND lease_until>?4 AND authority_generation=?5)",params![lease.id,lease.generation,lease.token,now,lease.authority],|r|r.get(0))?;
        ensure!(
            current && self.compass_authority_generation()? == lease.authority,
            "compass_lease_stale"
        );
        let goal = self
            .get_goal(&lease.input.goal.goal_id)?
            .context("compass_goal_missing")?;
        ensure!(
            goal.status == GoalStatus::Active
                && self.compass_goal_snapshot(&goal)? == lease.input.goal,
            "compass_goal_stale"
        );
        Ok(())
    }

    pub(crate) fn record_compass_response_usage(
        &self,
        lease: &CompassLease,
        model: Option<&str>,
        usage: Option<&str>,
    ) -> Result<()> {
        // A delayed attempt may record its cost in the general ledger, but may
        // never overwrite the result or usage belonging to a newer attempt.
        self.connection.execute(
            "UPDATE goal_compass_assessments SET model=?4,usage_json=?5 WHERE id=?1 AND status='running' AND generation=?2 AND lease_token=?3",
            params![lease.id,lease.generation,lease.token,model,usage],
        )?;
        Ok(())
    }

    /// Assessment result and card commit atomically. A stale response cannot
    /// replace an existing card, update a goal, or alter the captured evidence.
    pub(crate) fn complete_compass_assessment(
        &self,
        lease: &CompassLease,
        verdict: &CompassVerdict,
        model: Option<&str>,
        usage: Option<&str>,
        now: i64,
    ) -> Result<()> {
        verdict.validate(&lease.input)?;
        let tx = self.compass_tx()?;
        self.require_compass_lease(lease, now)?;
        tx.execute("UPDATE goal_compass_assessments SET status='succeeded',output_json=?2,usage_json=?3,model=?4,lease_token=NULL,lease_until=NULL,updated_at=?5,last_error=NULL WHERE id=?1",params![lease.id,serde_json::to_string(verdict)?,usage,model,now])?;
        if verdict.classification == "drifting" {
            let goal = self
                .get_goal(&lease.input.goal.goal_id)?
                .context("compass_goal_missing")?;
            ensure!(
                !self.has_recent_compass_nudge_for_goal(&goal.id, 7)?,
                "compass_cooldown_changed"
            );
            if let Some(id) = &goal.plan_id {
                ensure!(
                    !self.has_recent_compass_nudge_for_plan(id, 7)?,
                    "compass_cooldown_changed"
                );
            }
            tx.execute("INSERT INTO goal_compass_nudges(id,goal_id,goal_content,plan_id,plan_intent,current_focus,stalled_days,recent_topics,status,created_at,assessment_id) VALUES(?1,?2,?3,?4,?5,?6,0,?7,'pending',?8,?9)",params![uuid::Uuid::new_v4().to_string(),goal.id,goal.content,goal.plan_id,goal.plan_id.as_deref().map(|id| self.get_plan(id)).transpose()?.flatten().map(|p|p.intent_text),goal.current_focus.unwrap_or_default(),verdict.question,Utc::now().to_rfc3339(),lease.id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn fail_compass_assessment(
        &self,
        lease: &CompassLease,
        code: &str,
        now: i64,
    ) -> Result<()> {
        let tx = self.compass_tx()?;
        // Releasing one's own failed/stale lease is safe after an ownership
        // change. Generation/token still fence a new worker's lease.
        tx.execute("UPDATE goal_compass_assessments SET status='failed',available_at=?4,lease_token=NULL,lease_until=NULL,last_error=?5,updated_at=?6 WHERE id=?1 AND status='running' AND generation=?2 AND lease_token=?3",params![lease.id,lease.generation,lease.token,now+REEVALUATE_SECONDS,code,now])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn compass_feedback(&self, nudge_id: &str, kind: &str, note: &str) -> Result<()> {
        ensure!(
            matches!(
                kind,
                "still_progressing" | "changed_direction" | "snooze" | "discuss"
            ),
            "compass_feedback_invalid"
        );
        ensure!(note.chars().count() <= 1000, "compass_feedback_too_long");
        let tx = self.compass_tx()?;
        let assessment: Option<String> = tx
            .query_row(
                "SELECT assessment_id FROM goal_compass_nudges WHERE id=?1 AND status='pending'",
                [nudge_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let assessment = assessment.context("compass_nudge_stale")?;
        tx.execute("INSERT INTO goal_compass_feedback(id,assessment_id,nudge_id,kind,note,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![uuid::Uuid::new_v4().to_string(),assessment,nudge_id,kind,note,Utc::now().timestamp()])?;
        if kind != "discuss" {
            tx.execute(
                "UPDATE goal_compass_nudges SET status='dismissed',dismissed_until=?2 WHERE id=?1",
                params![nudge_id, (Utc::now() + Duration::days(7)).to_rfc3339()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn compass_details(
        &self,
        nudge_id: &str,
    ) -> Result<Option<(String, String, String)>> {
        Ok(self.connection.query_row("SELECT a.input_json,a.output_json,a.id FROM goal_compass_nudges n JOIN goal_compass_assessments a ON a.id=n.assessment_id WHERE n.id=?1 AND a.status='succeeded'",[nudge_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
    }
}

#[cfg(test)]
mod tests;
