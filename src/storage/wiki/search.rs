//! 知识页的词法索引（FTS5）：为检索出候选页，替代在全部正文上逐词 `instr`。
//!
//! 取舍与不变量：
//! - **语义与扫描路径相同**：词项是不含分隔符的小写串，命中 = 它是文本的子串。
//!   文档按字符二元组入索引，词项查成二元组的相邻短语，二者等价（见
//!   `tokens` / `phrase`）。排序分也与扫描的 SQL 相同：主题文本（标题、标签、
//!   适用条件）命中计 4，正文命中计 1，按分数、最近出现时间、id 取前 80。
//!   唯一的差别在带大小写的非 ASCII 字母（如 É、Ж）：这里用 Rust 做大小写
//!   折叠，扫描路径里 SQLite 的 `lower()` 只折叠 ASCII，所以这类字母的大写
//!   形式索引能命中、扫描命不中。
//! - **索引是本机派生物**：不进快照、不同步，随时可以整体重建。触发器只往
//!   `wiki_search_dirty` 记页 id（纯 SQL，不调用任何应用函数，外部工具改库
//!   不会报错）；分词与写索引在这里惰性完成。
//! - **不新鲜就不用**：`wiki_search_dirty` 为空 ⇔ 索引与页面一致（两者在同一个
//!   事务里更新）。检索前顺手追平一小批，追不平就让调用方回落到扫描路径。
//!   索引永远不是正确性的依赖。
//! - **只在自己的短事务里写**：身处调用方的事务时不动索引——否则会把别人的
//!   读事务变成写事务，或让写锁跟着调用方的事务一起被长时间持有。检索路径
//!   也不排队等写锁：别的连接正在写，就直接走扫描。
//! - 文档号用 `wiki_search_docs` 分配的整数主键，不用 `wiki_pages` 的隐式
//!   rowid：后者会在 VACUUM（设置页的数据库整理）后改变。
//! - 记号写成 UTF-8 字节的十六进制，配内置的 `ascii` 分词器：不依赖 SQLite
//!   自带的 Unicode 字符分类表，也不受它的大小写折叠与去变音影响。

use super::{Store, WIKI_PAGE_COLS, WikiPage, map_wiki_page};
use anyhow::Result;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use std::time::Duration;

/// 分词方案的版本。改了 `tokens` 的产出就改这里：下次使用时整库重建。
const SCHEME: &str = "bigram-hex-v1";
const SCHEME_KEY: &str = "wiki_fts_scheme";
/// 段（连续的字母数字串）之间的隔断记号：让短语不能跨段相邻。真实记号是
/// 十六进制，至少 4 个字符，不会与它相同。
const BREAK: &str = "z";

/// 检索路径上顺手追平索引的上限：超过就让本次检索走扫描，剩下的交给后台。
const QUERY_REFRESH_PAGES: usize = 64;
const QUERY_REFRESH_CHARS: usize = 300_000;
/// 检索路径等写锁的时间。
const QUERY_LOCK_WAIT: Duration = Duration::from_millis(20);
/// 后台一批的上限：限制单次写事务的时长。
const BATCH_REFRESH_PAGES: usize = 500;
const BATCH_REFRESH_CHARS: usize = 2_000_000;
/// 与扫描路径相同的候选上限。
const CANDIDATES: usize = 80;

/// 文本 → 空格分隔的记号串：小写后，每段连续的字母数字字符按相邻两字切成
/// 二元组（十六进制），段尾加隔断。单字符的段不产生记号（词项至少两个字符）。
pub(crate) fn tokens(text: &str) -> String {
    let lowered = text.to_lowercase();
    let mut out = String::with_capacity(lowered.len() * 4);
    let mut previous: Option<char> = None;
    let mut emitted = false;
    for ch in lowered.chars().chain(std::iter::once(' ')) {
        if !ch.is_alphanumeric() {
            if emitted {
                out.push_str(BREAK);
                out.push(' ');
            }
            previous = None;
            emitted = false;
            continue;
        }
        if let Some(first) = previous {
            push_hex(&mut out, first);
            push_hex(&mut out, ch);
            out.push(' ');
            emitted = true;
        }
        previous = Some(ch);
    }
    out
}

fn push_hex(out: &mut String, ch: char) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut buffer = [0u8; 4];
    for byte in ch.encode_utf8(&mut buffer).bytes() {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
}

/// 词项 → FTS5 短语（带引号）。词项应是 `query_terms` 给出的那种：已小写、
/// 至少两个字符、只含字母数字。不满足后两条时返回 None，调用方回落扫描。
pub(crate) fn phrase(term: &str) -> Option<String> {
    let chars: Vec<char> = term.chars().collect();
    if chars.len() < 2 || !chars.iter().all(|ch| ch.is_alphanumeric()) {
        return None;
    }
    let mut out = String::from("\"");
    for pair in chars.windows(2) {
        if out.len() > 1 {
            out.push(' ');
        }
        push_hex(&mut out, pair[0]);
        push_hex(&mut out, pair[1]);
    }
    out.push('"');
    Some(out)
}

impl Store {
    /// 索引与页面一致：分词方案是当前的，且没有待重建的页。
    fn wiki_search_fresh(&self) -> Result<bool> {
        Ok(self.get_meta(SCHEME_KEY)?.as_deref() == Some(SCHEME)
            && !self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM wiki_search_dirty)",
                [],
                |row| row.get::<_, bool>(0),
            )?)
    }

    /// 后台 tick 调用：重建一批脏页的索引。返回索引是否已是最新。
    pub(crate) fn refresh_wiki_search_batch(&self) -> Result<bool> {
        self.refresh_wiki_search(BATCH_REFRESH_PAGES, BATCH_REFRESH_CHARS, None)
    }

    /// 让索引追上页面，至多处理 `max_pages` 页、约 `max_chars` 个字符（至少
    /// 一页）。返回 true 表示索引已是最新。索引本来就新鲜时不开事务；身处
    /// 调用方的事务、或在 `lock_wait` 内拿不到写锁时不做任何事，返回 false。
    /// `lock_wait` 为 None 时按连接默认的等待时间排队。
    fn refresh_wiki_search(
        &self,
        max_pages: usize,
        max_chars: usize,
        lock_wait: Option<Duration>,
    ) -> Result<bool> {
        if self.wiki_search_fresh()? {
            return Ok(true);
        }
        if !self.connection.is_autocommit() {
            return Ok(false);
        }
        let Some(transaction) = self.begin_immediate_within(lock_wait)? else {
            return Ok(false);
        };
        // 拿到写锁之后再看一次：别的连接可能刚刚重建过。
        if self.get_meta(SCHEME_KEY)?.as_deref() != Some(SCHEME) {
            // 分词方案变了（或第一次使用）：旧索引整体作废，全部页面重新排队。
            self.connection
                .execute("INSERT INTO wiki_fts(wiki_fts) VALUES('delete-all')", [])?;
            self.connection
                .execute("DELETE FROM wiki_search_docs", [])?;
            self.connection.execute(
                "INSERT OR IGNORE INTO wiki_search_dirty(page_id) SELECT id FROM wiki_pages",
                [],
            )?;
            self.set_meta(SCHEME_KEY, SCHEME)?;
        }
        let dirty: Vec<String> = self
            .connection
            .prepare("SELECT page_id FROM wiki_search_dirty LIMIT ?1")?
            .query_map([max_pages.max(1) as i64], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut chars = 0usize;
        for page_id in &dirty {
            chars += self.reindex_wiki_page(page_id)?;
            self.connection
                .execute("DELETE FROM wiki_search_dirty WHERE page_id=?1", [page_id])?;
            if chars >= max_chars {
                break;
            }
        }
        let fresh = self.wiki_search_fresh()?;
        transaction.commit()?;
        Ok(fresh)
    }

    /// 开一个写事务；给了 `wait` 就只等这么久，写锁被占时返回 None。
    fn begin_immediate_within(&self, wait: Option<Duration>) -> Result<Option<Transaction<'_>>> {
        let begin = || Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate);
        let Some(wait) = wait else {
            return Ok(Some(begin()?));
        };
        let usual: i64 = self
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        self.connection.busy_timeout(wait)?;
        let begun = begin();
        self.connection
            .busy_timeout(Duration::from_millis(usual.max(0) as u64))?;
        match begun {
            Ok(transaction) => Ok(Some(transaction)),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// 重建（或删除）一页的索引项，返回处理的字符数。
    fn reindex_wiki_page(&self, page_id: &str) -> Result<usize> {
        let doc: Option<i64> = self
            .connection
            .query_row(
                "SELECT doc FROM wiki_search_docs WHERE page_id=?1",
                [page_id],
                |row| row.get(0),
            )
            .optional()?;
        // 与扫描的 SQL 用同一份原始文本：tags 是存储的 JSON 串，不是解析后的列表。
        let page: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT p.title||' '||p.tags||' '||COALESCE(
                            (SELECT applicable_when FROM knowledge_metadata WHERE page_id=p.id),''),
                        p.content_md
                   FROM wiki_pages p WHERE p.id=?1",
                [page_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((prominent, body)) = page else {
            // 页面已删除。
            if let Some(doc) = doc {
                self.connection
                    .execute("DELETE FROM wiki_fts WHERE rowid=?1", [doc])?;
                self.connection
                    .execute("DELETE FROM wiki_search_docs WHERE doc=?1", [doc])?;
            }
            return Ok(0);
        };
        let doc = match doc {
            Some(doc) => doc,
            None => {
                self.connection.execute(
                    "INSERT INTO wiki_search_docs(page_id) VALUES(?1)",
                    [page_id],
                )?;
                self.connection.last_insert_rowid()
            }
        };
        self.connection.execute(
            "INSERT OR REPLACE INTO wiki_fts(rowid,prominent,body) VALUES(?1,?2,?3)",
            params![doc, tokens(&prominent), tokens(&body)],
        )?;
        Ok(prominent.chars().count() + body.chars().count())
    }

    /// 用索引取候选页，结果与扫描路径相同：主题文本命中计 4、正文命中计 1，按
    /// 分数、最近出现时间、id 取前 80 个可参与的页。索引不是最新、或有词项
    /// 无法表达时返回 None，调用方走扫描。
    pub(crate) fn wiki_search_candidates(&self, terms: &[String]) -> Result<Option<Vec<WikiPage>>> {
        let mut arms = Vec::with_capacity(terms.len() * 2);
        for term in terms {
            let Some(phrase) = phrase(term) else {
                return Ok(None);
            };
            arms.push(serde_json::json!([format!("prominent : {phrase}"), 4]));
            arms.push(serde_json::json!([format!("body : {phrase}"), 1]));
        }
        if arms.is_empty()
            || !self.refresh_wiki_search(
                QUERY_REFRESH_PAGES,
                QUERY_REFRESH_CHARS,
                Some(QUERY_LOCK_WAIT),
            )?
        {
            return Ok(None);
        }
        let pages = self
            .connection
            .prepare_cached(&candidates_sql())?
            .query_map([serde_json::Value::Array(arms).to_string()], map_wiki_page)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(pages))
    }
}

/// 候选查询。参数是 `[[短语查询, 权重], …]` 的 JSON：每个（词项，列）一条
/// 短语查询，按文档累加权重。先只带排序要用的列选出前 80 个，再回表取整行：
/// 正文很大，不能让全部命中页的正文都进排序。
///
/// 资格过滤指定走 `wiki_pages_search_filter`（迁移 84）：它覆盖过滤和排序用到
/// 的全部列。不指定时规划器选主键索引再回表，而状态、时间这些列在行里排在
/// 正文之后，读它们要把每个命中页的正文溢出页走一遍。
fn candidates_sql() -> String {
    format!(
        "SELECT {WIKI_PAGE_COLS} FROM (
            SELECT p.id AS pid, h.score AS score, p.last_seen_at AS seen
              FROM (SELECT f.rowid AS doc, SUM(json_extract(t.value,'$[1]')) AS score
                      FROM json_each(?1) AS t CROSS JOIN wiki_fts AS f
                     WHERE f.wiki_fts MATCH json_extract(t.value,'$[0]')
                     GROUP BY f.rowid) h
              JOIN wiki_search_docs d ON d.doc=h.doc
              JOIN wiki_pages p INDEXED BY wiki_pages_search_filter ON p.id=d.page_id
             WHERE {eligible}
             ORDER BY h.score DESC, p.last_seen_at DESC, p.id LIMIT {CANDIDATES}
         ) top JOIN wiki_pages ON wiki_pages.id=top.pid
         ORDER BY top.score DESC, top.seen DESC, top.pid",
        eligible = crate::knowledge::select::sql_participating("p"),
    )
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
