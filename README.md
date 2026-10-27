# Epicycle

A local-first subscription tracker built with Tauri 2, Rust and SvelteKit.

## Development

All tools come from the Nix flake. With direnv the shell loads automatically;
otherwise run `nix develop` first.

```sh
just          # build the app (only what changed) and open it
just dev      # development mode, reloading on UI changes
just test     # Rust and frontend tests
just check    # formatting, lints, type checks and sqlx metadata
just --list   # everything else
```

Your data lives in `~/.local/share/io.github.nicolasdumitru.epicycle/`. To try
things without touching it, run with another folder, e.g.
`XDG_DATA_HOME=/tmp/epicycle-test just`.

SQL queries are checked at compile time against metadata committed in `.sqlx/`.
After changing a query or a migration in `crates/app/migrations/`, regenerate it:

```sh
just sqlx-prepare
```

## Layout

- `crates/app/` — Tauri app crate: commands and SQLite storage
- `crates/core/` — pure domain logic: money, recurrence, subscriptions, commitment, cash flow, exchange rates
- `ui/` — SvelteKit frontend (single-page app, no server rendering)

Design decisions are recorded in [DECISIONS.md](DECISIONS.md).
