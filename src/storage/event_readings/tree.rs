//! Verify complete coverage from immutable ranges and exact dependency hashes.
use super::*;
use std::collections::{HashMap, HashSet};

impl Store {
    pub(crate) fn event_reading_children(&self, node: &ReadingNode) -> Result<Vec<ReadingNode>> {
        let rows=self.connection.prepare("SELECT n.id,n.task_id,n.kind,n.level,n.start_char,n.end_char,n.input_hash,n.output_json,d.position,d.output_hash FROM event_reading_dependencies d JOIN event_reading_nodes n ON n.id=d.child_id AND n.task_id=d.task_id WHERE d.task_id=?1 AND d.parent_id=?2 ORDER BY d.position")?.query_map(params![node.task_id,node.id],|r|Ok((ReadingNode{id:r.get(0)?,task_id:r.get(1)?,kind:r.get(2)?,level:r.get::<_,i64>(3)? as usize,start:r.get::<_,i64>(4)? as usize,end:r.get::<_,i64>(5)? as usize,input_hash:r.get(6)?,output:r.get(7)?},r.get::<_,i64>(8)? as usize,r.get::<_,String>(9)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut children = vec![];
        for (child, position, hash) in rows {
            ensure!(position == children.len(), "event_reading_dependency_order");
            let output = child
                .output
                .as_ref()
                .context("event_reading_dependency_unfinished")?;
            ensure!(
                content_hash(output) == hash,
                "event_reading_dependency_changed"
            );
            children.push(child);
        }
        ensure!(
            (2..=8).contains(&children.len()),
            "event_reading_fan_in_limit"
        );
        let identity: Vec<_> = children
            .iter()
            .map(|child| (&child.id, content_hash(child.output.as_ref().unwrap())))
            .collect();
        ensure!(
            content_hash(&serde_json::to_string(&identity)?) == node.input_hash,
            "event_reading_dependency_changed"
        );
        Ok(children)
    }

    fn reading_dependencies(&self, task: &str) -> Result<Vec<(String, usize, String, String)>> {
        Ok(self.connection.prepare("SELECT parent_id,position,child_id,output_hash FROM event_reading_dependencies WHERE task_id=?1 ORDER BY parent_id,position")?.query_map([task],|r|Ok((r.get(0)?,r.get::<_,i64>(1)? as usize,r.get(2)?,r.get(3)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Every node must be completed and structurally accounted for. A single
    /// range/short summary is never used as a substitute for leaf coverage.
    pub(crate) fn event_reading_frontier(&self, task: &ReadingTask) -> Result<Vec<ReadingNode>> {
        let original = self.event_reading_text(task)?;
        let chars: Vec<char> = original.chars().collect();
        let nodes = self.event_reading_nodes(&task.id)?;
        ensure!(
            !nodes.is_empty() && nodes.len() as i64 <= MAX_NODES,
            "event_reading_coverage_incomplete"
        );
        let deps = self.reading_dependencies(&task.id)?;
        let map: HashMap<_, _> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut consumed = HashSet::new();
        let mut bytes = 0;
        let mut cursor = 0;
        for node in &nodes {
            ensure!(
                node.start < node.end && node.end <= chars.len(),
                "event_reading_range_invalid"
            );
            let raw = node
                .output
                .as_ref()
                .context("event_reading_coverage_incomplete")?;
            bytes += raw.len();
            let output: ReadingOutput = serde_json::from_str(raw)?;
            self.validate_event_reading_output(node, &output, &chars)?;
            if node.kind == "leaf" {
                ensure!(
                    node.level == 0 && node.start == cursor,
                    "event_reading_leaf_gap_or_overlap"
                );
                ensure!(
                    content_hash(&chars[node.start..node.end].iter().collect::<String>())
                        == node.input_hash,
                    "event_reading_content_changed"
                );
                ensure!(
                    !deps.iter().any(|(parent, _, _, _)| parent == &node.id),
                    "event_reading_leaf_dependencies"
                );
                cursor = node.end;
            } else {
                let dependencies: Vec<_> = deps
                    .iter()
                    .filter(|(parent, _, _, _)| parent == &node.id)
                    .collect();
                ensure!(
                    (2..=8).contains(&dependencies.len()),
                    "event_reading_fan_in_limit"
                );
                let mut position = node.start;
                let mut level = 0;
                let mut identity = vec![];
                for (index, (_, ordinal, child, hash)) in dependencies.into_iter().enumerate() {
                    ensure!(
                        index == *ordinal && consumed.insert(child.as_str()),
                        "event_reading_dependency_order"
                    );
                    let child = map
                        .get(child.as_str())
                        .context("event_reading_dependency_missing")?;
                    ensure!(
                        child.level < node.level && child.start == position,
                        "event_reading_dependency_gap_or_overlap"
                    );
                    let raw = child
                        .output
                        .as_ref()
                        .context("event_reading_dependency_unfinished")?;
                    ensure!(
                        content_hash(raw) == *hash,
                        "event_reading_dependency_changed"
                    );
                    position = child.end;
                    level = level.max(child.level + 1);
                    identity.push((&child.id, hash));
                }
                ensure!(
                    position == node.end && level == node.level && level <= 16,
                    "event_reading_dependency_coverage_or_depth"
                );
                ensure!(
                    content_hash(&serde_json::to_string(&identity)?) == node.input_hash,
                    "event_reading_dependency_changed"
                );
            }
        }
        ensure!(
            cursor == task.total_chars && bytes as i64 <= MAX_CACHE_BYTES,
            "event_reading_coverage_incomplete"
        );
        ensure!(
            deps.iter()
                .all(|(parent, _, child, _)| map.contains_key(parent.as_str())
                    && map.contains_key(child.as_str())),
            "event_reading_dependency_missing"
        );
        let mut frontier: Vec<_> = nodes
            .into_iter()
            .filter(|n| !consumed.contains(n.id.as_str()))
            .collect();
        frontier.sort_by_key(|n| n.start);
        let mut cursor = 0;
        for n in &frontier {
            ensure!(n.start == cursor, "event_reading_coverage_incomplete");
            cursor = n.end;
        }
        ensure!(
            cursor == task.total_chars,
            "event_reading_coverage_incomplete"
        );
        Ok(frontier)
    }

    pub(crate) fn event_reading_root(&self, task: &ReadingTask) -> Result<ReadingNode> {
        let mut frontier = self.event_reading_frontier(task)?;
        ensure!(frontier.len() == 1, "event_reading_root_incomplete");
        Ok(frontier.remove(0))
    }

    pub(crate) fn mark_event_reading_ready(&self, task_id: &str) -> Result<()> {
        let tx = self.reading_tx()?;
        let task = self.event_reading_task(task_id)?;
        self.event_reading_root(&task)?;
        let busy: bool = tx.query_row(
            "SELECT lease_token IS NOT NULL FROM event_reading_tasks WHERE id=?1",
            [task_id],
            |r| r.get(0),
        )?;
        ensure!(!busy, "event_reading_lease_active");
        tx.execute(
            "UPDATE event_reading_tasks SET status='ready',updated_at=?2 WHERE id=?1",
            params![task_id, chrono::Utc::now().timestamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(super) fn validate_event_reading_output(
        &self,
        node: &ReadingNode,
        result: &ReadingOutput,
        chars: &[char],
    ) -> Result<()> {
        ensure!(
            serde_json::to_string(result)?.len() <= MAX_OUTPUT_BYTES
                && !result.summary.trim().is_empty()
                && result.summary.chars().count() <= 1600
                && result.quotes.len() <= 8,
            "event_reading_output_limit"
        );
        let allowed = if node.kind == "merge" {
            let mut quotes = vec![];
            for child in self.event_reading_children(node)? {
                let output: ReadingOutput = serde_json::from_str(child.output.as_ref().unwrap())?;
                quotes.extend(output.quotes);
            }
            Some(quotes)
        } else {
            None
        };
        for q in &result.quotes {
            ensure!(
                q.start >= node.start
                    && q.end <= node.end
                    && q.end <= chars.len()
                    && q.start < q.end
                    && q.end - q.start <= 400,
                "event_reading_quote_range"
            );
            ensure!(
                chars[q.start..q.end].iter().collect::<String>() == q.text,
                "event_reading_quote_forged"
            );
            if let Some(allowed) = &allowed {
                ensure!(allowed.contains(q), "event_reading_quote_not_in_children");
            }
        }
        Ok(())
    }
}
