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

First impressions from testing on the desktop (2026-11-03) are being worked
through; see "Next steps".

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
- **Currency and date inputs replaced.** The currency field only accepts
  currencies in use for payments today (no funds such as CHE, no replaced ones
  such as HRK), searchable by code or name. Dates are typed (year first, or year
  last in the locale's order) or picked from a calendar that jumps straight to
  any month and year: WebKitGTK's `<input type="date">` doesn't accept typing.
  Opening the currency field without typing shows the whole list in code order,
  scrolled to the current currency.
- **Backup exports** are named with the local date and time
  (`epicycle-backup-2026-11-03_19-45-07.json`) and start in Downloads. Mobile
  document pickers may ignore the starting folder; check on iOS.
- **Replace import gets a second, destructive mode.** The current Replace (adds
  revisions, erases nothing) stays the default and its wording gets clearer. A
  separate "replace history" option makes this device's history exactly the
  backup's, permanently deleting what isn't in it. It is a deliberate exception to
  "keep data forever", behind a red confirmation button that is disabled for 5
  seconds (with a circular countdown) next to a friendly Cancel.
- **History in the UI**, with restoring an earlier version of a subscription.
- **Plain-text notes per subscription**, part of the subscription itself: editing
  them creates a revision, and they are in backups and merged like any other
  field. Backup format version 2; version 1 files still import, with empty notes.
  Markdown may come later.
- **Converted total**: ECB rates fetched and cached when online, plus manual
  rates for currencies the ECB doesn't publish. The total is shown in the
  reporting currency by default and can be switched to any other currency.

## Next steps

Agreed order for the rest of milestone 1 (2026-11-03):

1. ~~Currency and date inputs~~ (done).
2. ~~Editing subscriptions with notes~~ (done): each subscription has its own
   screen for name, notes, price or billing changes from a date, pausing,
   resuming, cancelling and deleting.
3. ~~History view~~ (done): on each subscription's screen, with restore; a
   screen for deleted subscriptions; the two Replace modes.
4. ~~Exchange rates and the converted total~~ (done): ECB rates fetched when
   online, manual rates on an "Exchange rates" screen, and the dashboard total in
   any currency, with the rates it used and their age.

5. ~~Favourite currencies~~ (done, 2026-11-03): one-tap chips on the dashboard
   total and in the add form, starred from the dashboard; backed up with manual
   exchange rates in backup format version 3.
6. ~~Hourly exchange rate refresh~~ (done): in the background while the app runs.

Agreed, for when there is a settings screen:

- **Default reporting currency**: for now it changes from the dashboard
  ("Always show totals in …").
- **Default folder for backups**: exports start in Downloads on every platform
  (since 2026-11-03); a setting will let the user choose another folder.

Later, not yet agreed:

- **iOS build**: find a Mac (for example a remote build service), then go through
  the specification's platform validation checklist on a real iPhone, including
  the document picker for backups.
- **CI** on GitHub Actions: `just check` and `just test`.
- **Upcoming payments** list and the **monthly chart** (scheduled cash flow vs
  monthly-equivalent commitment, kept visibly separate).
- **Nix package** for installing the app on NixOS (`nix run`, or in the system
  configuration).

## Open questions

- Sync through cloud drives: if wanted, when, and which providers first?
