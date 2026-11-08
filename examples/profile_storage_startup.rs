//! Measures storage initialization on an explicitly supplied disposable database.
//! Store::open may migrate/backfill; never point this tool at a live database.
fn main() -> anyhow::Result<()> {
    let path=std::env::args().nth(1).ok_or_else(||anyhow::anyhow!("disposable database copy path required"))?;
    let path=std::path::Path::new(&path);
    let start=std::time::Instant::now();
    drop(elsewhen::storage::Store::open(path)?);
    println!("foreground_open_ms={}",start.elapsed().as_millis());
    elsewhen::storage::Store::mark_initialized(path)?;
    let start=std::time::Instant::now();
    for _ in 0..20 { drop(elsewhen::storage::Store::open_background(path)?); }
    println!("background_open_20_ms={}",start.elapsed().as_millis());
    Ok(())
}
