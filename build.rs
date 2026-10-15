fn main() {
    // Flutter Rust Bridge v2 doesn't need build.rs for generation
    // Code generation is done via CLI: flutter_rust_bridge_codegen generate
    println!("cargo:rerun-if-changed=src/api.rs");

    // 版本可溯源：设置页要展示的不是恒定不变的 pubspec 版本，而是「这次
    // 构建对应哪个 git 提交」。编译期抓短 hash；不再依赖（或补充）版本号。
    let hash = git_head_hash();
    println!("cargo:rustc-env=ELSEWHEN_BUILD_HASH={hash}");
    // 升级序号：提交计数单调递增，客户端据此判断「远端是否比本地新」
    //（hash 只能回答「相同/不同」，无法分辨方向）。
    let seq = git_commit_count();
    println!("cargo:rustc-env=ELSEWHEN_BUILD_SEQ={seq}");
    // HEAD 或 index 变化（commit/checkout/stage）都触发重编译取新 hash；
    // 源码改动本身经常规依赖链同样触发。
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}

fn git_head_hash() -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() { "unknown".into() } else { s }
        }
        // tarball/无 git 环境构建：如实降级，不伪造。
        _ => "unknown".into(),
    }
}

fn git_commit_count() -> u64 {
    let output = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}
