# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Elsewhen is a local-first personal event system with a **Rust core** and **Flutter UI**. It captures raw events (timestamped text entries) into an immutable SQLite store, then asynchronously analyzes them using an OpenAI-compatible AI provider to extract structured information.

**Core architectural principle**: Raw events are immutable and remain the source of truth even if AI analysis fails. Analysis happens asynchronously in separate jobs with exponential backoff retry.

### Project Structure

- **Rust core** (project root): Event storage, AI analysis, knowledge digest, SQLite database — built only as the Flutter bridge library (no CLI binary)
- **Flutter UI** (`ui/` directory): Modern cross-platform GUI for desktop (Linux/macOS/Windows) and mobile (Android/iOS)

## Key Architecture

### Data Flow

1. **Event capture** → `Store::insert_event()` (and the other event write paths in `src/storage/events.rs`) atomically commit:
   - An immutable raw event record
   - A pending analysis job
   - A pending knowledge digest job (`knowledge_digest_jobs`, available after a 10-minute settle window)

2. **Analysis** → The Flutter app's background worker (`RustBridgeRepository._wakeAnalysisWorker`, 5s timer) calls `trigger_analysis()`, which claims jobs, calls the AI provider and stores structured results

3. **Knowledge digest** (FR-PES-004 phase 1) → After the analysis queue drains, the same worker calls `trigger_knowledge_digest()`: skips non-recordable events, claims a bounded batch, asks the model for wiki page proposals, validates the whole batch, and commits page changes + `wiki_log` + job confirmation in one transaction. Failures back off; after 5 attempts a job cools down (6h) and is re-queued automatically. There is no manual trigger; the queue and run log (`knowledge_digest_runs`) are viewable read-only in Settings → 数据.

4. **Database is the single source of truth** for AI provider config (configured in the app's settings)

5. **Imported source compilation** → The same worker's knowledge tick processes one eligible source version. It automatically saves bounded method/case/principle output as reference knowledge, or skips insufficient material. Optional page extraction follows the same reference publication rules. Manual pending/rejected suggestions, human edits and confirmed rules remain protected; substantive changes to protected pages need confirmation.

6. **Cross-source wiki maintenance** → Each knowledge tick also integrates one source with related material into shared reference topics and evidence-backed conflict/outdated/duplicate hints. `knowledge_background_runs` provides per-version dedupe, leases and failure backoff; unchanged versions are revisited at most once every 30 days. Topic discovery inputs are bounded (8 sources, 3 existing topics; long originals use cached full-coverage readings), updates retain all prior source identities, and the integration batch commits topics, hints and successful task status atomically. Schema v6 adds `knowledge_semantic_issues`. Schema v7 adds resumable full-document readings, generation-fenced per-page source refresh jobs, strategy versions, review decisions, reviewed topic organization and provider context windows. UI views refresh on background changes. There is no manual background trigger; the application must be running.

### Module Responsibilities

- `storage/`: SQLite schema and migrations, transactions, analysis and knowledge digest queues (`storage/digest.rs`). Enforces raw event immutability via trigger.
- `ai/`: OpenAI-compatible provider client, conversation memory and tools, JSON parsing with fallback for markdown-wrapped responses
- `wiki.rs`: LLM wiki knowledge base — digest worker (`process_digest_queue`), validation, index, export
- `api/`: flutter_rust_bridge facade (business-facing APIs only)
- `event.rs`: Domain types for new events
- `config.rs`: Platform-specific data directory resolution, respects `ELSEWHEN_DATA_DIR` override

## Development Commands

### Rust Core

#### Build and test
```bash
cargo build --release          # Bridge library → target/release/libelsewhen.so
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

Desktop UI app packages（Flutter UI + Rust bridge lib；先跑 scripts/regen.sh 保证桥 hash 一致）:
```bash
./scripts/package-appimage.sh      # Linux → dist/Elsewhen-*-x86_64.AppImage
./scripts/package-dmg.sh           # macOS → dist/Elsewhen-*.dmg（ad-hoc 签名；分发需 Developer ID + 公证）
powershell -File scripts/package-msi.ps1   # Windows → dist/Elsewhen-*-x64.msi（需 WiX Toolset v3.11）
```

## Database Schema

Core tables in `elsewhen.db`:

1. **events**: Immutable raw entries with trigger preventing mutation of `raw_text`, `recorded_at`, `source`
2. **analysis_jobs**: Queue with status (`pending`/`running`/`retry`/`succeeded`/`failed`), exponential backoff via `available_at`
3. **event_analyses**: JSON results keyed by `prompt_version`
4. **knowledge_digest_jobs**: Per-event digest queue keyed by `(event_id, digest_version)`, status adds `skipped`; progress is per event, never a time cursor
5. **knowledge_digest_runs**: One row per claimed batch (status, duration, created/updated/protected page slugs, error — no raw event text)

Additional tables: **ai_provider_configs** (base URL, model, API key), **wiki_pages** / **wiki_revisions** / **wiki_log** (knowledge base).

## Testing Notes

- Tests create temporary databases in system temp dir
- Storage tests verify immutability trigger, concurrent inserts, job lifecycle
- Digest tests (`src/digest_tests.rs`) cover backlog/same-timestamp/late events, whole-batch rollback, retry/cool-down, recovery and human-edit protection
- AI tests verify JSON parsing with markdown fence fallback
- Run tests with `cargo test` (no special setup needed)

## Platform-Specific Behavior

### Data Directory
Default follows platform conventions via `directories` crate:
- Linux: `~/.local/share/elsewhen/`
- macOS: `~/Library/Application Support/elsewhen/`
- Windows: `%LOCALAPPDATA%\elsewhen\`

Override with `ELSEWHEN_DATA_DIR=/custom/path` for testing or portable installs.

## Important Constraints

1. **Never bypass `Store::insert_event()`** — all event sources must use this single commit boundary
2. **Raw events are append-only** — the immutability trigger will abort any UPDATE attempt
3. **Analysis and knowledge digest failures never block event storage** — jobs enter retry state, raw event persists
4. **Database is authoritative for AI config** — configured in the app's settings
5. **Knowledge digest has no manual trigger** — it runs only from the background worker; do not add CLI or button entry points

## Flutter GUI Development

The Flutter app is the only GUI (see `docs/requirements/product/FR-PES-003-Flutter统一GUI.md`); the legacy Iced GUI and CLI have been removed.

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
- wiki import, tags, relations, derivatives, todos, rules, provider settings, analysis queue status, and knowledge digest queue/run log (read-only)

After changing a public Rust API, run `scripts/regen.sh` from the repository root. It regenerates Dart/Rust bindings and rebuilds `target/release/libelsewhen.so` so content hashes stay synchronized.

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
- `cargo build --release`

### LLM Wiki maintenance (schema v7)

- Long originals (>1400 characters) are read in 6000-character chunks and recursively summarized, with exact quoted excerpts verified separately. Cached reading nodes are keyed by snapshot and reading strategy version. Never label generated summaries as verbatim source text.
- `knowledge_refresh_jobs` tracks every dependent non-original knowledge page after a new source snapshot. Generation and run leases fence late results; human edits and rule metadata require review. Do not add manual background trigger/retry UI.
- `knowledge/authoring.rs` binds chat drafts only to selected verified evidence. `knowledge/review.rs` implements paragraph selection, issue-to-revision decisions and historical text restoration proposals. Partial acceptance cannot silently migrate source versions.
- `knowledge/organization.rs` prepares reviewed topic merge/split batches. Confirmation preserves all input evidence, archives old topics and records replacement links; original sources remain unchanged.
- `ai/budget.rs` checks serialized model requests, tool definitions and output reserve. Known BPE families are bundled locally, unknown families use conservative byte estimates. Context windows belong to provider config; explicit context-window rejection may retry once with a smaller per-request budget.
- Current delivery/limits: `docs/llm-wiki-implementation-gaps.md`; existing-data verification guide: `docs/testing/llm-wiki-real-world-testing.md`. Never equate mock/bridge tests with real-model quality acceptance.


### LLM Wiki repair and dependency review (schema v8)

- `knowledge/dependencies.rs` records explicit knowledge page bases for derivatives and cited chat drafts. Upstream changes invalidate direct and transitive descendants until review; do not infer missing historical bases from current content.
- Source repair uses selected current usable snapshots and a reviewed full proposal. Never remove a source relation while silently retaining unsupported prose. Skipped, still-outdated source refresh jobs recover automatically when their sources become usable again.
- `knowledge/maintenance.rs` validates cross-page issue resolution against all cited source versions and the target's current revision, and exposes paginated review history. `knowledge/queue.rs` inventories unstarted/waiting/skipped work independently of recent run logs; its UI is read-only.
- Current validation and boundaries remain in `docs/llm-wiki-implementation-gaps.md`; personal databases must not be used by automated tests.


### LLM Wiki workflows and scale (schema v9)

- Topic plan membership and post-acceptance states support guarded batch undo. Old plans without undo bases are readable but not silently reversible.
- Reading/use state is independent of source endorsement. Artifact versions select an exact revision per parent/type; never attach a newly read revision ID to previously displayed content.
- Dependency epochs accelerate stale checks; full bases remain the final confirmation contract. Background dependency refresh always prepares reviewed proposals, never silently accepts them.
- Suggestions have append-only feedback, scoped to the originating conversation. Acceptance is not execution and does not create a permanent rule.
- Library and artifact lists load bounded metadata; queue aggregation is in SQLite. Remaining full scans and measured limits: `docs/llm-wiki-scale-and-refactoring.md`. Do not call the entire app ten-thousand/hundred-thousand-page ready based on one query benchmark.
