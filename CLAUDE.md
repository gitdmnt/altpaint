# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## First Steps

Before any task, read in order:
1. [AGENTS.md](AGENTS.md) — navigation rules and task-specific reading sets
2. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) — current implementation state

For multi-crate boundary work, also read [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**When documents and code conflict, the running code is authoritative.**

## Commands

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test --workspace
cargo test -p <crate-name>          # single crate
cargo test <test_name>              # single test

# Lint (must pass before committing)
cargo clippy --workspace --all-targets

# Build panel Wasm (PowerShell, Windows)
.\scripts\build-ui-wasm.ps1
.\scripts\build-ui-wasm.ps1 -Release
```

## Development Workflow

1. Create an Issue defining purpose, scope, and completion criteria
2. Branch off `main` — never commit directly to `main`
3. TDD first: write a failing test, then implement the minimum to pass
4. After Changes: `cargo test -p <crate-name>` must pass
5. Then, `cargo test` + `cargo clippy --workspace --all-targets`
6. Update docs immediately after code changes
7. End tasks with `ask_user` to wait for confirmation

## Architecture Overview

altpaint is a desktop digital painting application. The workspace is a Rust 2024-edition Cargo workspace with 28 members: 15 library crates, 10 built-in plugins, and 1 desktop app.

### Runtime Flow

**Startup**: `apps/desktop` initializes winit + wgpu → `DesktopApp::new` restores session/project/workspace → `PanelRuntime` loads `plugins/**/*.altp-panel` → `storage` loads tools and pens → initial render

**Input → Draw**: OS input → `runtime/pointer.rs` normalizes → `app/input.rs` routes to canvas or panel → `canvas::view_mapping` converts coordinates → `canvas::gesture` produces `PaintInput` → `canvas::context_builder` resolves paint context from `Document` → built-in bitmap plugin writes bitmap diff → diff applied to `Document` → `render::FramePlan` assembled → dirty rect compose → `wgpu_canvas.rs` presents to GPU

**Panel**: `panel-dsl` parses `.altp-panel` → `plugin-host` (wasmtime) runs Wasm → `PanelRuntime` syncs host snapshot → `PanelEvent`/`HostAction` → `DesktopApp` applies as `Command` or side effect → `render` rasterizes panel surfaces and hit regions

### Key Crates

| Crate                                 | Responsibility                                                                      |
| ------------------------------------- | ----------------------------------------------------------------------------------- |
| `apps/desktop`                        | winit + wgpu host, `DesktopApp` orchestration, input routing, present               |
| `crates/app-core`                     | `Document`, domain model (`Work→Page→Panel→LayerNode`), `Command`, paint primitives |
| `crates/canvas`                       | `CanvasRuntime`, gesture state machine, bitmap ops (stroke/fill/erase/lasso)        |
| `crates/render`                       | `FramePlan`/`CanvasPlan`/`OverlayPlan`/`PanelPlan`, dirty rect, CPU frame compose   |
| `crates/panel-runtime`                | Panel registry, DSL/Wasm bridge, host snapshot sync, persistent config              |
| `crates/ui-shell`                     | Panel workspace layout, focus, hit-test, surface render                             |
| `crates/panel-api`                    | Panel/host contract (`PanelPlugin`, `PanelEvent`, `HostAction`, `ServiceRequest`)   |
| `crates/plugin-host`                  | wasmtime-based Wasm panel runtime                                                   |
| `crates/panel-dsl`                    | `.altp-panel` parser/validator/IR                                                   |
| `crates/panel-schema`                 | Host↔Wasm shared DTOs                                                               |
| `crates/plugin-sdk` + `plugin-macros` | Plugin author SDK and proc-macros                                                   |
| `crates/storage`                      | SQLite project persistence, pen/tool catalog                                        |
| `crates/desktop-support`              | Session, dialogs, paths, profiler, canvas templates                                 |
| `crates/workspace-persistence`        | `WorkspaceUiState`, `PluginConfigs` shared DTOs                                     |
| `plugins/*`                           | 10 built-in panels (each has `.altp-panel` + Rust/Wasm source + compiled `.wasm`)   |

### Current Responsibility Concentrations

- **`DesktopApp`** (`apps/desktop/src/app/`) — orchestration center; bootstrap, command routing, panel dispatch, I/O services, dirty rect collection, and present logic are all here. Ongoing refactoring is distributing this.
- **`Document`** (`crates/app-core/src/document.rs`) — domain state plus tool/pen runtime state
- **`CanvasRuntime`** (`crates/canvas/src/runtime.rs`) — paint plugin registry, context building, bitmap ops

### File Placement Rules

- `runtime/` — external runtimes and stateful bridges
- `presentation/` — layout, hit-test, focus, text input, surface generation
- `services/` — I/O orchestration (project, workspace, export, catalog)
- `ops/` — high-frequency canvas/render operations
- `tests/` — crate/module boundary tests
- `lib.rs` — module declarations, re-exports, thin public API only (no large implementations)

## Panel Plugin Development

See [docs/builtin-plugins/PLUGIN_DEVELOPMENT.md](docs/builtin-plugins/PLUGIN_DEVELOPMENT.md) for the Rust SDK, `.altp-panel` DSL, and Wasm build process.

Built-in plugins live in `plugins/<name>/` with `panel.altp-panel`, `src/lib.rs`, and the compiled `.wasm`. The `.wasm` files are git-ignored and must be rebuilt with `build-ui-wasm.ps1`.

## Key Documentation

| Document                                                   | When to read                                                    |
| ---------------------------------------------------------- | --------------------------------------------------------------- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)               | Design principles, crate responsibilities, dependency direction |
| [docs/ROADMAP.md](docs/ROADMAP.md)                         | Implementation phases and next priorities                       |
| [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)       | Dirty rect algorithm, frame composition, canvas rendering       |
| [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) | Compile-time and runtime dependency graph                       |
| [docs/SKETCH.md](docs/SKETCH.md)                           | Product vision, MVP scope, requirements background              |
