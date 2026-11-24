use super::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) mod grants;
mod management;
mod review;
mod search;
mod strategies;
mod writes;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Basis {
    pub kind: String,
    pub id: String,
    pub version: String,
    pub quote: String,
    pub offset: usize,
    pub independent_group: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Claim {
    pub id: String,
    pub page_slug: String,
    pub body_hash: String,
    pub content: Option<String>,
    pub kind: String,
    pub subject: String,
    pub predicate: String,
    pub state: String,
    pub scope: String,
    pub conversation_id: String,
    pub version: i64,
    pub confirmed_hash: Option<String>,
    pub support: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub superseded_by: Option<String>,
    pub basis: Vec<Basis>,
    pub pinned: bool,
    pub excluded: bool,
    pub salience: String,
}

fn hash(text: &str) -> String {
    super::knowledge::content_hash(text)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn date(value: Option<&str>) -> Result<Option<String>> {
    value
        .map(|value| {
            Ok(chrono::DateTime::parse_from_rfc3339(value)?
                .with_timezone(&chrono::Utc)
                .to_rfc3339())
        })
        .transpose()
}

fn row_claim(row: &rusqlite::Row<'_>) -> rusqlite::Result<Claim> {
    let raw: String = row.get(17)?;
    let basis = serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Claim {
        id: row.get(0)?,
        page_slug: row.get(1)?,
        body_hash: row.get(2)?,
        content: row.get(3)?,
        kind: row.get(4)?,
        subject: row.get(5)?, predicate: row.get(6)?, state: row.get(7)?, scope: row.get(8)?, conversation_id: row.get(9)?, version: row.get(10)?, confirmed_hash: row.get(11)?, support: row.get(12)?, valid_from: row.get(13)?, valid_until: row.get(14)?, superseded_by: row.get(15)?,
        basis,
        pinned: row.get(18)?, excluded: row.get(19)?, salience: row.get(20)?,
    })
}

const COLUMNS: &str = "id,page_slug,body_hash,content,kind,subject,predicate,state,scope,conversation_id,version,confirmed_hash,support,valid_from,valid_until,superseded_by,updated_at,basis_json,pinned,excluded,salience";

impl Store {
    pub(crate) fn memory_event_visible(&self, event: &str) -> Result<bool> {
        let Some(snapshot) = &self.conversation_snapshot else {
            return Ok(true);
        };
        let linked = self.connection.prepare("SELECT m.id FROM messages m WHERE m.conversation_id=?1 AND (EXISTS(SELECT 1 FROM input_records i WHERE i.message_id=m.id AND i.event_id=?2) OR EXISTS(SELECT 1 FROM chat_input_effects e WHERE e.input_message_id=m.id AND e.event_id=?2))")?.query_map(params![snapshot.conversation_id,event], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(linked
            .iter()
            .all(|id| snapshot.messages.iter().any(|message| &message.id == id)))
    }

    pub(crate) fn memory_claim(&self, id: &str) -> Result<Option<Claim>> {
        Ok(self
            .connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE id=?1"),
                [id],
                row_claim,
            )
            .optional()?)
    }

    pub(crate) fn memory_policy(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM personal_memory_policy WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub(crate) fn memory_basis_current(&self, basis: &Basis) -> Result<bool> {
        let text: Option<String> = match basis.kind.as_str() {
            "event" => {
                if !self.recordable_event(&basis.id)? || !self.memory_event_visible(&basis.id)? {
                    return Ok(false);
                }
                self.connection
                    .query_row(
                        "SELECT raw_text FROM events WHERE id=?1",
                        [&basis.id],
                        |row| row.get(0),
                    )
                    .optional()?
            }
            "message" => {
                if let Some(snapshot) = &self.conversation_snapshot {
                    let belongs: bool = self.connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 AND conversation_id=?2)",
                        params![basis.id, snapshot.conversation_id],
                        |row| row.get(0),
                    )?;
                    if belongs
                        && !snapshot
                            .messages
                            .iter()
                            .any(|message| message.id == basis.id)
                    {
                        return Ok(false);
                    }
                }
                if !crate::ai::memory::history::source_eligible(self, &basis.id)? {
                    return Ok(false);
                }
                self.connection
                    .query_row(
                        "SELECT content FROM messages WHERE id=?1 AND role='user'",
                        [&basis.id],
                        |row| row.get(0),
                    )
                    .optional()?
            }
            "snapshot" => {
                let Some(snapshot) = self.source_snapshot(&basis.id)? else {
                    return Ok(false);
                };
                if snapshot.opinion.as_deref() == Some("reject") {
                    return Ok(false);
                }
                let current: String = self.connection.query_row("SELECT id FROM knowledge_snapshots WHERE source_id=?1 ORDER BY version DESC LIMIT 1",[&snapshot.source_id],|row|row.get(0))?;
                if current != snapshot.id {
                    return Ok(false);
                }
                Some(snapshot.content_md)
            }
            _ => return Ok(false),
        };
        Ok(text.is_some_and(|text| {
            hash(&text) == basis.version
                && text
                    .chars()
                    .skip(basis.offset)
                    .take(basis.quote.chars().count())
                    .collect::<String>()
                    == basis.quote
        }))
    }

    pub(crate) fn memory_validity(&self, claim: &Claim) -> Result<&'static str> {
        if let Some(content) = claim.content.as_deref() {
            if hash(content) != claim.body_hash {
                return Ok("stale");
            }
            if claim.valid_from.as_deref().is_some_and(|date| chrono::DateTime::parse_from_rfc3339(date).map_or(true, |date| date > chrono::Utc::now())) || claim.valid_until.as_deref().is_some_and(|date| chrono::DateTime::parse_from_rfc3339(date).map_or(true, |date| date <= chrono::Utc::now())) { return Ok("stale"); }
            if claim.basis.is_empty() { return Ok("unavailable"); }
            let mut explicit_input = false;
            let mut all_current = true;
            for basis in &claim.basis {
                let current = self.memory_basis_current(basis)?;
                explicit_input |= current && basis.kind == "message";
                all_current &= current;
            }
            if !all_current && !(explicit_input && claim.confirmed_hash.as_deref() == Some(&claim.body_hash)) { return Ok("unavailable"); }
            if !grants::conversation_current(self, &claim.conversation_id)? { return Ok("unavailable"); }
            return Ok("current");
        }
        let Some(page) = self.get_wiki_page(&claim.page_slug)? else {
            return Ok("unavailable");
        };
        if page.status == "archived" || page.opinion.as_deref() == Some("reject") {
            return Ok("unavailable");
        }
        if hash(&page.content_md) != claim.body_hash
            || crate::knowledge::dependencies::stale(self, &page.id)?
        {
            return Ok("stale");
        }
        let time = chrono::Utc::now();
        if claim.valid_from.as_deref().is_some_and(|date| {
            chrono::DateTime::parse_from_rfc3339(date).map_or(true, |date| date > time)
        }) || claim.valid_until.as_deref().is_some_and(|date| {
            chrono::DateTime::parse_from_rfc3339(date).map_or(true, |date| date <= time)
        }) {
            return Ok("stale");
        }
        if claim.basis.is_empty() {
            return Ok("unavailable");
        }
        let mut explicit_input = false;
        let mut all_current = true;
        for basis in &claim.basis {
            let current = self.memory_basis_current(basis)?;
            explicit_input |= current && basis.kind == "message";
            all_current &= current;
        }
        if !all_current
            && !(explicit_input && claim.confirmed_hash.as_deref() == Some(&claim.body_hash))
        {
            return Ok("unavailable");
        }
        if !grants::conversation_current(self, &claim.conversation_id)? {
            return Ok("unavailable");
        }
        Ok("current")
    }
}

#[cfg(test)]
mod tests;
