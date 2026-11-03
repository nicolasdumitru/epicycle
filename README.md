# Epicycle

A local-first subscription tracker built with Tauri 2, Rust and SvelteKit.

## Installing on NixOS

The flake builds the app as a package:

```sh
nix run github:nicolasdumitru/epicycle      # try it without installing
```

To install it, add the flake to your system configuration's inputs and the
package to your packages:

```nix
# flake.nix
inputs.epicycle.url = "github:nicolasdumitru/epicycle";

# in a NixOS module, with `inputs` passed in
environment.systemPackages = [
  inputs.epicycle.packages.${pkgs.stdenv.hostPlatform.system}.default
];
```

`nix flake update epicycle` then picks up a newer version. The app appears in
your desktop's app launcher as Epicycle.

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

The package is defined in `nix/package.nix`. After changing
`ui/pnpm-lock.yaml`, its pnpm dependency hash needs updating: `nix build` fails
and prints the new one.

SQL queries are checked at compile time against metadata committed in `.sqlx/`.
After changing a query or a migration in `crates/app/migrations/`, regenerate it:

```sh
just sqlx-prepare
```

## Layout

- `crates/app/` — Tauri app crate: commands and SQLite storage
- `crates/core/` — pure domain logic: money, recurrence, subscriptions, commitment, cash flow, exchange rates
- `ui/` — SvelteKit frontend (single-page app, no server rendering)

Design decisions are recorded in [docs/DECISIONS.md](docs/DECISIONS.md).
