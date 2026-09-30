//! Versioned knowledge-to-knowledge edges; originals use immutable snapshots.
use crate::storage::{knowledge::content_hash, Store, WikiPage};
use anyhow::{ensure, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Basis {
    pub page_id: String,
    pub hash: String,
}

fn basis(store: &Store, page: &WikiPage) -> Result<String> {
    let meta = store.knowledge_metadata(&page.slug)?;
    let edges = edges(store, &page.id)?;
    Ok(content_hash(&serde_json::to_string(&serde_json::json!({
        "content":page.content_md,"status":page.status,"opinion":page.opinion,
        "events":page.source_event_ids,"sources":store.page_source_snapshots(&page.slug)?.iter().map(|s|&s.id).collect::<Vec<_>>(),
        "applicable":meta.applicable_when,"strength":meta.strength,"edges":edges
    }))?))
}

fn edges(store: &Store, id: &str) -> Result<Vec<(String, Option<String>)>> {
    Ok(store.connection.prepare("SELECT upstream_id,basis FROM knowledge_dependencies WHERE page_id=?1 ORDER BY upstream_id")?
        .query_map([id], |r| Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn state(store: &Store, page_id: &str) -> Result<serde_json::Value> {
    let mut values = Vec::new();
    let rows=store.connection.prepare("WITH RECURSIVE parents(id) AS (SELECT ?1 UNION SELECT d.upstream_id FROM knowledge_dependencies d JOIN parents p ON d.page_id=p.id)
        SELECT d.upstream_id,d.basis FROM knowledge_dependencies d JOIN parents p ON d.page_id=p.id ORDER BY d.page_id,d.upstream_id")?
        .query_map([page_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, old) in rows {
        let current = basis(store, &page_by_id(store, &id)?)?;
        values.push((id, old, current));
    }
    Ok(serde_json::to_value(values)?)
}

fn page_by_id(store: &Store, id: &str) -> Result<WikiPage> {
    let slug: String =
        store
            .connection
            .query_row("SELECT slug FROM wiki_pages WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
    store.get_wiki_page(&slug)?.context("上游知识页不存在")
}

pub(crate) fn stale(store: &Store, page_id: &str) -> Result<bool> {
    Ok(store.connection.query_row("WITH RECURSIVE parents(id) AS (SELECT ?1 UNION SELECT d.upstream_id FROM knowledge_dependencies d JOIN parents p ON d.page_id=p.id)
        SELECT EXISTS(SELECT 1 FROM knowledge_dependencies d JOIN parents a ON a.id=d.page_id JOIN wiki_pages p ON p.id=d.upstream_id
        WHERE d.basis_epoch IS NULL OR d.basis_epoch<>p.knowledge_epoch OR p.status='archived' OR p.opinion='reject')",[page_id],|r|r.get(0))?)
}

/// Upgrade known v8 hashes once. Unknown historical bases remain unknown.
pub(crate) fn backfill_epochs(store: &Store) -> Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let rows=store.connection.prepare("SELECT page_id,upstream_id,basis FROM knowledge_dependencies WHERE basis_epoch IS NULL AND basis IS NOT NULL")?
        .query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (page, id, old) in rows {
        let current = basis(store, &page_by_id(store, &id)?)?;
        store.connection.execute("UPDATE knowledge_dependencies SET basis_epoch=CASE WHEN ?3=?4 THEN (SELECT knowledge_epoch FROM wiki_pages WHERE id=?2) ELSE -1 END WHERE page_id=?1 AND upstream_id=?2",params![page,id,old,current])?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn capture(store: &Store, slugs: &[String]) -> Result<Vec<Basis>> {
    let mut out = Vec::new();
    for slug in slugs {
        let p = store.get_wiki_page(slug)?.context("上游知识页不存在")?;
        if !store.source_history(slug)?.is_empty() {
            continue;
        }
        ensure!(!stale(store, &p.id)?, "上游知识仍待复核，请先处理上游页面");
        ensure!(
            crate::knowledge::citation_for_page(store, &p, "依赖核验".into(), 100)?.is_some(),
            "上游知识已失效，请先修复其依据"
        );
        if !out.iter().any(|b: &Basis| b.page_id == p.id) {
            out.push(Basis {
                page_id: p.id.clone(),
                hash: basis(store, &p)?,
            });
        }
    }
    ensure!(out.len() <= 8, "知识依赖超过 8 页，请拆分产物");
    Ok(out)
}

pub(crate) fn validate(store: &Store, bases: &[Basis]) -> Result<()> {
    for b in bases {
        let p = page_by_id(store, &b.page_id)?;
        ensure!(
            b.hash == basis(store, &p)? && !stale(store, &p.id)?,
            "上游知识已变化，请重新检查更新"
        );
        ensure!(
            crate::knowledge::citation_for_page(store, &p, "依赖核验".into(), 100)?.is_some(),
            "上游知识依据已失效，请先处理上游页面"
        );
    }
    Ok(())
}

pub(crate) fn bind(store: &Store, page_id: &str, bases: &[Basis]) -> Result<()> {
    validate(store, bases)?;
    for b in bases {
        let cycle:bool=store.connection.query_row("WITH RECURSIVE parents(id) AS (SELECT ?1 UNION SELECT d.upstream_id FROM knowledge_dependencies d JOIN parents p ON d.page_id=p.id) SELECT EXISTS(SELECT 1 FROM parents WHERE id=?2)",params![b.page_id,page_id],|r|r.get(0))?;
        ensure!(!cycle, "知识依赖不能形成循环");
        store.connection.execute("INSERT INTO knowledge_dependencies(page_id,upstream_id,basis,basis_epoch) VALUES(?1,?2,?3,(SELECT knowledge_epoch FROM wiki_pages WHERE id=?2)) ON CONFLICT(page_id,upstream_id) DO UPDATE SET basis=excluded.basis,basis_epoch=excluded.basis_epoch",params![page_id,b.page_id,b.hash])?;
    }
    Ok(())
}

/// Supplies current upstream prose and conditions, never merely its raw sources.
pub(crate) fn context(store: &Store, page: &WikiPage) -> Result<(Vec<Basis>, String)> {
    let mut slugs = Vec::new();
    let mut text = String::new();
    for (id, _) in edges(store, &page.id)? {
        let p = page_by_id(store, &id)?;
        let m = store.knowledge_metadata(&p.slug)?;
        text.push_str(&format!(
            "\n上游知识《{}》（含人工纠正）：\n{}\n适用条件：{}\n",
            p.title, p.content_md, m.applicable_when
        ));
        slugs.push(p.slug);
    }
    ensure!(
        text.chars().count() <= 12000,
        "上游知识超出审阅容量，请拆分产物"
    );
    Ok((capture(store, &slugs)?, text))
}
