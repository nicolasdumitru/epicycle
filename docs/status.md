# Project status

Where the project stands, what has been decided since the
[specification](spec.md), and what comes next. Keep it current: update it at the
end of each piece of work. Design decisions and their reasons are in
[DECISIONS.md](DECISIONS.md).

Last updated: 2026-11-03.

## Milestone 1

| Criterion | Status |
|---|---|
| 1. Stack chosen and approved | Done |
| 2. Prototype on NixOS and iOS: add a subscription, see its monthly equivalent and next payment, persists across restarts | Done on the desktop (tested on NixOS under WSL). iOS not built yet: needs a Mac. |
| 3. Domain crate with unit tests: monthly and annual equivalents, payment schedules (including month-end and leap-year cases), period totals, currency handling | Done (`crates/core`) |
| 4. Backup export/import round-trip test proving no data loss and no duplication | Done (`crates/app/src/db.rs` tests), plus Export and Import in the UI |

Not yet checked: native NixOS (outside WSL), phone-width layouts on a real
device, how the app gets today's date in the device's time zone on iOS, and the
document picker on iOS (desktop uses GTK file dialogs).

## Changes to the specification

Decisions made after the specification, oldest first. Where they changed the
design, DECISIONS.md has the details.

**2026-10-27**

- **iOS first, not Android**, for the first milestone. iOS builds need a Mac,
  which isn't available yet, so the desktop prototype came first.
- **Frontend: Svelte 5 + SvelteKit + TypeScript.**
- **Storage: SQLite through sqlx, not rusqlite**, for compile-time checked
  queries.
- **Layout: `crates/` for Rust crates and `ui/` for the frontend**, both at the
  repository root, one Cargo workspace.
- **Normalising: 365.2425 days per year** (the Gregorian average), not 365.25.
- **Recurrence:** monthly and yearly subscriptions renew on the same date each
  month or year, and are distinct from "every N days". Stay flexible for custom
  cycles: every N months on a given day, every N weeks on a given weekday, every
  couple of years.
- **Keep data forever.** Log every change to every subscription.
- **Keep the history of how conflicts were resolved**, in case one was resolved
  differently from what the user expected.
- **History should eventually be in backups and synced across devices**, like a
  Git repository with a single branch. Backups include it since 2026-11-03.

**2026-11-03**

- **Syncing through storage the user already has** (Nextcloud, iCloud Drive,
  Google Drive) is under consideration as the way to sync devices. Notes on how
  it would work are in DECISIONS.md.
- **Backup format and import rules**: merging compares histories instead of "the
  later `updated_at` wins", and Replace keeps what it replaces in the history
  instead of first copying the database. See DECISIONS.md, "Backups".
- **Export and Import use Tauri's official dialog plugin.**
- **Commits follow Conventional Commits with Angular-style types and scopes.**
- **`just` runs everything**: `just`, `just dev`, `just test`, `just check`.
- **Project context lives in the repository** (this file and spec.md), because
  conversations with coding agents don't move between devices.
- **DECISIONS.md moved to `docs/`**, next to spec.md and this file. Comments in
  the first two migrations still say "DECISIONS.md" without the folder: editing
  an applied migration changes its checksum, and sqlx then refuses to open
  existing databases.

## Next steps

Not yet agreed; a starting point for planning the next milestone.

- **iOS build**: find a Mac (for example a remote build service), then go through
  the specification's platform validation checklist on a real iPhone, including
  the document picker for backups.
- **CI** on GitHub Actions: `just check` and `just test`.
- **Editing subscriptions**: rename, change price (adds a plan from a date),
  pause, resume, cancel. The domain crate supports all of these; the UI only adds
  and deletes. Also a "last day of the month" option, and showing the change
  history.
- **Upcoming payments** list and the **monthly chart** (scheduled cash flow vs
  monthly-equivalent commitment, kept visibly separate).
- **Exchange rates**: fetch from the ECB, cache, show fresh/stale/manual, and a
  reporting-currency total. The domain logic exists (`crates/core/src/fx.rs`);
  nothing fetches or stores rates yet.
- **Nix package** for installing the app on NixOS (`nix run`, or in the system
  configuration).

## Open questions

- Sync through cloud drives: if wanted, when, and which providers first?
