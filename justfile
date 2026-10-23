set default-list
set unstable

[windows]
set shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Format Rust, TOML, documents, and recipes.
fmt: _fmt-rust _fmt-tombi _fmt-oxfmt _fmt-just

# Check formatting and Rust lints without changing sources.
check: (_fmt-rust "--check") (_fmt-tombi "--check") (_fmt-oxfmt "--check") (_fmt-just "--check") lint

# Check every workspace target.
typecheck *args:
    cargo check --locked --workspace --all-targets {{ args }}

# Treat Clippy warnings as errors across all targets.
lint *args:
    cargo clippy --locked --workspace --all-targets {{ args }} -- -D warnings

# Run workspace target tests and doctests.
test: test-targets test-docs

# Run workspace target tests.
test-targets *args:
    cargo test --locked --workspace --all-targets {{ args }}

# Run workspace doctests.
test-docs *args:
    cargo test --locked --workspace --doc {{ args }}

# Build API documentation with warnings treated as errors.
[env("RUSTDOCFLAGS", "-D warnings")]
gen-docs *args:
    cargo doc --locked --no-deps --workspace {{ args }}

# Run the complete local validation gate.
ci: check typecheck test gen-docs

# Run the bot with credentials supplied through the environment.
dev *args:
    cargo run --locked --bin shuai {{ args }}

_fmt-rust *args:
    cargo fmt --all {{ args }}

_fmt-tombi *args:
    tombi format --quiet {{ args }} .

_fmt-oxfmt *args:
    bunx oxfmt {{ args }} README.md AGENTS.md docs .oxfmtrc.json

_fmt-just *args:
    just --fmt {{ args }}
