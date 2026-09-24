# Agent Note: Remove CLI From Desktop Builds

Status: implemented

## Problem

The desktop packaging scripts built the unused Rust CLI binary. That binary
produced compiler warnings and was not the executable used by the Flutter
desktop application.

## Decision

The Cargo package exposes only the Rust bridge library for desktop builds.
Linux package scripts build the Flutter Linux bundle, place
`libelsewhen.so` in the bundle's `lib` directory, and install a small wrapper
as `/usr/bin/elsewhen`. The desktop entry point launches the Flutter runner
without the former `capture` argument.

`src/lib.rs` allows dead code because bridge-generated exports are reached
dynamically by Flutter and cannot all be inferred by Cargo's static analysis.

## Alternatives considered

- Keep compiling the CLI and suppress its warnings: rejected because the CLI
  is not part of the desktop product and adds an unnecessary artifact.
- Remove the Rust package entirely: rejected because the Flutter application
  still depends on the Rust bridge library.

## Consequences

Desktop packages now contain the GUI bundle and bridge library only, and the
release Rust build is warning-free. The repository no longer produces an
`elsewhen` or `test_adapter` Cargo binary; CLI workflows that depended on
those binaries must be migrated separately.
