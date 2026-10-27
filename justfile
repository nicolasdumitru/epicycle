# Build the app (only what changed) and open it.
run: deps
    cargo tauri build --no-bundle
    ./target/release/epicycle

# Open the app in development mode, reloading on UI changes.
dev: deps
    cargo tauri dev

# Run the Rust and frontend tests.
test: deps
    cargo test --workspace
    pnpm --dir ui test

# Run formatting, lint and type checks, and verify the sqlx query metadata.
check: deps
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    python3 scripts/sqlx_prepare.py --check
    pnpm --dir ui check

# Regenerate the sqlx query metadata after changing a query or migration.
sqlx-prepare:
    python3 scripts/sqlx_prepare.py

# Install frontend dependencies; quick when they already match the lockfile.
deps:
    pnpm --dir ui install --frozen-lockfile
