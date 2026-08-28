# Repository Guidelines

## Project Structure & Module Organization

This is a compact macOS and Windows desktop application written in Rust. The egui application and DDC/CI monitor logic live in `src/main.rs`. `build.rs` embeds `icon.ico` and `photon_count_adjuster.exe.manifest` into Windows builds. Cargo metadata and pinned dependencies are in `Cargo.toml` and `Cargo.lock`. User-facing documentation and the current UI image are in `README.md` and `screenshot.webp`. CI and release automation live under `.github/workflows/`.

Keep new modules under `src/` and split them out of `main.rs` only when they have a clear responsibility. Put unit tests beside the code they exercise; use `tests/` only for integration-level behavior.

## Build, Test, and Development Commands

Run these commands from the repository root:

- `cargo check`: type-check the project quickly.
- `cargo build --release`: build the optimized native executable in `target/release/`.
- `cargo test`: run all unit and integration tests.
- `cargo fmt --all -- --check`: verify Rust formatting without changing files.
- `cargo clippy -- -D warnings`: run lint checks and treat warnings as errors.
- `cargo run`: build and launch the GUI for manual testing.

The project uses a pinned Rust nightly so Cargo can reject dependencies published less than 14 days ago. Runtime validation requires a DDC/CI display. Windows builds also require the MSVC toolchain and Visual Studio C++ Build Tools. CI runs on macOS and Windows.

## Coding Style & Naming Conventions

Use standard `rustfmt` formatting with four-space indentation. Follow Rust naming conventions: `snake_case` for functions and modules, `UpperCamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Keep Clippy clean. Prefer computed values over redundant mutable state. Fail conspicuously; do not silently discard errors. Document the reason for unsafe Windows API calls, workarounds, or intentionally ignored failures.

## Testing Guidelines

Add focused tests for non-GUI logic and name them after the behavior, for example `selects_first_controllable_monitor`. Hardware-dependent brightness changes require manual testing with both controllable and unsupported displays on macOS and Windows. Before submitting, run the same checks as CI: `cargo check`, `cargo test`, formatting, and Clippy.

## Commit & Pull Request Guidelines

History uses short, imperative, lowercase subjects such as `update deps` and `make messages appear in the gui`. Keep each commit scoped to one change. Pull requests should explain the user-visible effect, note the OS and monitor configurations tested, and include an updated screenshot for UI changes. Link relevant issues and call out dependency or manifest changes explicitly.
