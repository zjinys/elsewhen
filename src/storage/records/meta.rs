//! KV 元数据与分析列表：get/set/remove_meta、list_analyses。

use super::*;

impl Store {
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM app_meta WHERE key = ?1")?;
        statement
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(Into::into)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// 删除 app_meta 键（覆盖层「跟随全局」时清除覆盖值，回落到全局层）
    pub fn remove_meta(&self, key: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT e.raw_text,
                coalesce(json_extract(a.result_json,'$.event_type'),'unknown'),
                coalesce(json_extract(a.result_json,'$.confidence'),0),
                coalesce(json_extract(a.result_json,'$.clarifications'),'[]')
         FROM event_analyses a JOIN events e ON e.id=a.event_id
         ORDER BY a.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AnalysisSummary {
                raw_text: row.get(0)?,
                event_type: row.get(1)?,
                confidence: row.get(2)?,
                clarifications: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}
