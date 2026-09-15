fn main() {
    // Flutter Rust Bridge v2 doesn't need build.rs for generation
    // Code generation is done via CLI: flutter_rust_bridge_codegen generate
    println!("cargo:rerun-if-changed=src/api.rs");
}
