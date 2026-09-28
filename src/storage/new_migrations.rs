//! Current-schema bootstrap for databases created after the migration rewrite.
//! Historical databases continue through `migrations.rs` until exported/imported.

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

static MIGRATION_LIST: &[M<'static>] = &[M::up(include_str!("../../schema.sql"))];

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    Migrations::from_slice(MIGRATION_LIST).to_latest(connection)?;
    Ok(())
}
