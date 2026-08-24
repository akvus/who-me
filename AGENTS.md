# Repository Guidelines

## Project Structure & Module Organization

`who-me` is a Rust 2024 terminal UI application. `src/main.rs` owns CLI parsing, terminal setup, and the event loop. Keep domain data and validation in `src/model.rs`, persistence and atomic backups in `src/storage.rs`, keyboard-driven state transitions in `src/app.rs`, palette loading in `src/theme.rs`, and Ratatui rendering/layout in `src/ui.rs`. Unit tests live beside the code in each module under `#[cfg(test)]`; there is currently no separate `tests/` or asset directory. `README.md` documents user-facing controls and storage behavior. Cargo output belongs in ignored `target/`.

## Build, Test, and Development Commands

- `cargo run` launches the dashboard without installing it.
- `cargo build` compiles a debug build; use `cargo build --release` to exercise release settings.
- `cargo install --path .` installs the local `who-me` binary.
- `cargo fmt --check` verifies standard Rust formatting.
- `cargo clippy --all-targets -- -D warnings` lints every target and treats warnings as failures.
- `cargo test` runs all colocated unit tests.

Run formatting, Clippy, and tests before submitting changes.

## Coding Style & Naming Conventions

Use default `rustfmt` output and four-space indentation. Follow Rust naming conventions: `snake_case` for functions, variables, and test names; `PascalCase` for structs and enums; and `SCREAMING_SNAKE_CASE` for constants. Keep module responsibilities narrow and prefer explicit `Result` propagation over panics in runtime paths. Preserve the keyboard-first interaction model and ensure terminal state is restored on every exit or error path.

## Testing Guidelines

Add focused unit tests to the affected module. Name tests after observable behavior, such as `malformed_data_is_not_overwritten`. Cover successful paths and failure cases, especially document validation, persistence/backup safety, key handling, Unicode text, and responsive layout calculations. Use `tempfile` for filesystem tests so they never touch a contributor's real data directory.

## Commit & Pull Request Guidelines

Existing history favors short, capitalized, imperative summaries such as `Improve UI`; keep each commit focused. Pull requests should explain the user-visible change, note storage-format or compatibility effects, and list verification commands. Link relevant issues. For rendering changes, include a terminal screenshot or concise before/after description and mention tested terminal sizes.

## Data & Configuration Safety

Runtime data is stored below `$XDG_DATA_HOME/who-me/` (or `~/.local/share/who-me/`) and themes are read from Omarchy state. Do not commit personal `data.toml` files. Preserve validation, temporary-file writes, and `.bak` creation when changing storage code.
