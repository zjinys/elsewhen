//! Backend processing ownership. Local capture stays writable everywhere;
//! background claims require the designated active installation, not UI state.

use anyhow::{Result, ensure};

use crate::storage::Store;

impl Store {
    pub(crate) fn background_processing_allowed(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT allowed FROM sync_background_permission",
            [],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn require_background_processor(&self) -> Result<()> {
        ensure!(
            self.background_processing_allowed()?,
            "sync_background_processor_required"
        );
        Ok(())
    }

    pub(crate) fn require_background_quiescent_for_sync(&self) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let running: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM analysis_jobs WHERE status='running'
                UNION ALL SELECT 1 FROM knowledge_digest_jobs WHERE status='running'
                UNION ALL SELECT 1 FROM knowledge_digest_runs WHERE status='running'
                UNION ALL SELECT 1 FROM knowledge_background_runs WHERE status='running'
                UNION ALL SELECT 1 FROM knowledge_refresh_jobs WHERE status='running')",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            !running,
            "sync_background_processing_must_finish_before_enable"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
