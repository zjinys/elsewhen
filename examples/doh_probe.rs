// 手动探针：ELSEWHEN_TEST_DOH=https://... cargo run --release --example doh_probe <url>
fn main() {
    let doh = std::env::var("ELSEWHEN_TEST_DOH").expect("set ELSEWHEN_TEST_DOH");
    let url = std::env::args().nth(1).unwrap_or_else(|| "https://www.cloudflare.com/cdn-cgi/trace".into());
    match elsewhen::net_doh::https_get_via_doh(&url, &doh, std::time::Duration::from_secs(15)) {
        Ok(t) => println!("=== {} ({} bytes) ===\n{}", url, t.len(), t),
        Err(e) => println!("FAIL {}: {:#}", url, e),
    }
}
