fn main() {
    // Configure code generation
    flutter_rust_bridge_codegen::generate(
        flutter_rust_bridge_codegen::Config::from_config_file(
            "flutter_rust_bridge.yaml".into(),
        )
        .unwrap(),
        flutter_rust_bridge_codegen::Opts {
            skip_deps_check: true,
            ..Default::default()
        },
    );
}
