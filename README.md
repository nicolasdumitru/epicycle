# Epicycle

A local-first subscription tracker built with Tauri 2, Rust and SvelteKit.

## Development

All tools come from the Nix flake. With direnv the shell loads automatically;
otherwise run `nix develop` first.

```sh
pnpm --dir ui install --frozen-lockfile   # once, and after lockfile changes
cargo tauri dev                           # run the desktop app with hot reload
cargo tauri build                         # release build and bundles
```

Checks:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir ui check
```

SQL queries are checked at compile time against metadata committed in `.sqlx/`.
After changing a query or a migration in `crates/app/migrations/`, regenerate it:

```sh
python3 scripts/sqlx_prepare.py
```

## Layout

- `crates/app/` — Tauri app crate (commands and, later, the SQLite storage module)
- `crates/core/` — pure domain logic: money, recurrence, subscriptions, commitment, cash flow, exchange rates
- `ui/` — SvelteKit frontend (single-page app, no server rendering)

Design decisions are recorded in [DECISIONS.md](DECISIONS.md).
