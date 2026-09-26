# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Elsewhen is a local-first personal event system with a **Rust core** and **Flutter UI**. It captures raw events (timestamped text entries) into an immutable SQLite store, then asynchronously analyzes them using an OpenAI-compatible AI provider to extract structured information.

**Core architectural principle**: Raw events are immutable and remain the source of truth even if AI analysis fails. Analysis happens asynchronously in separate jobs with exponential backoff retry.

### Project Structure

- **Rust core** (project root): Event storage, AI analysis, SQLite database, legacy Iced GUI
- **Flutter UI** (`ui/` directory): Modern cross-platform GUI for desktop (Linux/macOS/Windows) and mobile (Android/iOS)

## Key Architecture

### Data Flow

1. **Event capture** → `Store::insert_event()` atomically commits both:
   - An immutable raw event record
   - A pending analysis job

2. **Analysis** → Background worker (`analyze-once` or `worker`) claims jobs, calls AI provider, stores structured results

3. **Database is the single source of truth** for AI provider config after first import from `.env`

### Module Responsibilities

- `storage.rs`: SQLite schema, transactions, job queue management. Enforces raw event immutability via trigger.
- `ai.rs`: OpenAI-compatible provider client, JSON parsing with fallback for markdown-wrapped responses
- `capture.rs`: Iced GUI for quick text entry (Enter to submit, Escape to cancel)
- `hotkey.rs`: Global keyboard listener (double-tap Left Ctrl) using `rdev` — X11 only, blocked on Wayland
- `event.rs`: Domain types for new events
- `config.rs`: Platform-specific data directory resolution, respects `ELSEWHEN_DATA_DIR` override
- `settings.rs`: Iced GUI showing capture command and database path

## Development Commands

### Rust Core

#### Build and test
```bash
cargo build --release          # Release binary → target/release/elsewhen
cargo test                      # Run all Rust unit/integration targets
cargo test --test <name>        # Run specific test
```

### Flutter UI

**Important**: This project uses [FVM (Flutter Version Management)](https://fvm.app/) to pin the Flutter SDK version. Always prefix Flutter commands with `fvm`.

#### Setup
```bash
cd ui/
fvm install                    # Install the pinned Flutter version (3.47.4)
fvm flutter pub get            # Get dependencies
```

#### Development
```bash
cd ui/
fvm flutter run -d linux       # Run on Linux desktop
fvm flutter run -d android     # Run on Android device/emulator
fvm flutter analyze            # Lint and analyze code
fvm flutter test               # Run unit tests
fvm flutter build linux        # Build Linux release
```

#### Flutter Doctor
```bash
fvm flutter doctor             # Check Flutter environment status
fvm flutter doctor -v          # Verbose output
```

### Rust Core Application

```bash
cargo run -- record "事件文本"   # CLI: save event directly
cargo run -- list               # List all raw events
cargo run -- analyses           # List completed structured analyses
cargo run -- capture            # Launch GUI capture window
cargo run -- daemon             # Start hotkey listener (X11 only)
cargo run -- settings           # Show settings window
cargo run -- providers          # Show active AI provider config
```
```

### AI analysis workflow
```bash
# First time: configure provider (will import from .env if database has none)
export ELSEWHEN_AI_BASE_URL="https://api.openai.com/v1"
export ELSEWHEN_AI_MODEL="gpt-4.1-mini"
export ELSEWHEN_AI_API_KEY="sk-..."

cargo run -- analyze-once       # Process one pending job
cargo run -- worker             # Continuous worker with retry loop
```

### Packaging
```bash
make release                    # Build release binary
make deb                        # Debian/Ubuntu package → dist/*.deb
make rpm                        # Fedora/RHEL package → dist/*.rpm
make pacman                     # Arch Linux package → dist/*.pkg.tar.*
make package                    # Build all formats (requires all tools)
```

Or use scripts directly:
```bash
./scripts/package-deb.sh
./scripts/package-rpm.sh
./scripts/package-pacman.sh
./scripts/install-linux-desktop.sh  # Install desktop metadata once
```

Desktop UI app packages（Flutter UI + Rust bridge lib；先跑 ./regen.sh 保证桥 hash 一致）:
```bash
./scripts/package-appimage.sh      # Linux → dist/Elsewhen-*-x86_64.AppImage
./scripts/package-dmg.sh           # macOS → dist/Elsewhen-*.dmg（ad-hoc 签名；分发需 Developer ID + 公证）
powershell -File scripts/package-msi.ps1   # Windows → dist/Elsewhen-*-x64.msi（需 WiX Toolset v3.11）
```

## Database Schema

Three core tables in `elsewhen.db`:

1. **events**: Immutable raw entries with trigger preventing mutation of `raw_text`, `recorded_at`, `source`
2. **analysis_jobs**: Queue with status (`pending`/`running`/`retry`/`succeeded`/`failed`), exponential backoff via `available_at`
3. **event_analyses**: JSON results keyed by `prompt_version`

Additional table: **ai_provider_configs** stores base URL, model, API key after first import from environment.

## Testing Notes

- Tests create temporary databases in system temp dir
- Storage tests verify immutability trigger, concurrent inserts, job lifecycle
- AI tests verify JSON parsing with markdown fence fallback
- Hotkey tests verify double-press timing window (80-300ms) and chord rejection
- Run tests with `cargo test` (no special setup needed)

## Platform-Specific Behavior

### Linux
- **X11**: Full global hotkey support via `rdev`
- **Wayland**: Hotkey blocked by compositor security. Users must bind `Meta+Space` → `elsewhen capture` in system settings
- GUI dependencies required: `libxkbcommon-dev libwayland-dev libx11-dev libxi-dev libxtst-dev`

### Data Directory
Default follows platform conventions via `directories` crate:
- Linux: `~/.local/share/elsewhen/`
- macOS: `~/Library/Application Support/elsewhen/`
- Windows: `%LOCALAPPDATA%\elsewhen\`

Override with `ELSEWHEN_DATA_DIR=/custom/path` for testing or portable installs.

## Important Constraints

1. **Never bypass `Store::insert_event()`** — all event sources must use this single commit boundary
2. **Raw events are append-only** — the immutability trigger will abort any UPDATE attempt
3. **Analysis failure never blocks event storage** — jobs enter retry state, raw event persists
4. **Database is authoritative for AI config** — after first import, `.env` is ignored
5. **Iced theme is `TokyoNight`** for both capture and settings windows

## Flutter GUI Development

The project is transitioning from Iced to Flutter for unified desktop and mobile GUI (see `docs/requirements/product/FR-PES-003-Flutter统一GUI.md`).

### FVM Setup
Flutter is managed via FVM (Flutter Version Manager):
```bash
fvm --version                   # Check FVM installation (currently 4.3.1)
fvm install <version>           # Install a Flutter version
fvm use <version>               # Set Flutter version for the project
fvm flutter --version           # Run Flutter commands via FVM
fvm flutter pub get             # Install dependencies
fvm flutter run -d linux        # Run on Linux desktop
```

### Flutter-Rust Bridge
The Flutter GUI communicates with Rust core via `flutter_rust_bridge`. The bridge exposes business-facing APIs (not raw SQLite access), including:
- `record_event(raw_text)`
- `list_events()`
- `create_conversation()` / `list_conversations()` / `list_messages()`
- `send_message(conversation_id, text)`
- `set_capture_mode(enabled)`
- wiki import, tags, relations, derivatives, todos, rules, provider settings, and analysis queue status

After changing a public Rust API, run `./regen.sh` from the repository root. It regenerates Dart/Rust bindings and rebuilds `target/release/libelsewhen.so` so content hashes stay synchronized.

Real Flutter bridge tests must initialize `RustBridgeRepository` through `ui/test/support/isolated_bridge.dart`. Never let a test use the default platform data directory or assume personal events/wiki/provider data already exist.

### GUI Architecture
- **Main app mode**: conversation, wiki/material processing, and todo views; the current roadmap is adding a unified daily record flow
- **Capture mode**: Compact mode in the same Flutter app window (not a separate native window)
- Modes are mutually exclusive; platform layer handles window size, focus, and topmost state
- Capture submission must not wait for network or AI response

## Current Product Direction

The active implementation plan is `docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md`. It supersedes the old phase ordering while retaining the local-first invariants. Every completed roadmap step must immediately update its checkbox and completion evidence in that file.

## CI

GitHub Actions workflow (`.github/workflows/ci.yml`) runs on Ubuntu, macOS, Windows:
- `cargo test --all-targets`
- `cargo build --release --bins`

Linux runner installs GUI dependencies before build.
