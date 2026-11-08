//! Explicit local maintenance; caller must back up first and stop the app.
fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("database path required"))?;
    let store = elsewhen::storage::Store::open(std::path::Path::new(&path))?;
    let (goals, plans, chats) = store.clear_goal_workspace_data()?;
    println!("cleared goals={goals}, plans={plans}, goal_conversations={chats}");
    Ok(())
}
