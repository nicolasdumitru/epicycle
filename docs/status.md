# Project status

Where the project stands, what has been decided since the
[specification](spec.md), and what comes next. Keep it current: update it at the
end of each piece of work. Design decisions and their reasons are in
[DECISIONS.md](DECISIONS.md).

Last updated: 2026-11-06.

## Milestone 1

| Criterion | Status |
|---|---|
| 1. Stack chosen and approved | Done |
| 2. Prototype on NixOS and iOS: add a subscription, see its monthly equivalent and next payment, persists across restarts | Done on the desktop (tested on native NixOS and under WSL). iOS: CI builds for the simulator; installing on an iPhone waits for a friend's Mac (see "Open questions"). |
| 3. Domain crate with unit tests: monthly and annual equivalents, payment schedules (including month-end and leap-year cases), period totals, currency handling | Done (`crates/core`) |
| 4. Backup export/import round-trip test proving no data loss and no duplication | Done (`crates/app/src/db.rs` tests), plus Export and Import in the UI |

Everything except the iOS half of criterion 2 is done (2026-11-03), along with
much that went beyond the milestone: editing, notes, history and restore,
exchange rates and favourite currencies, and CI.

Not yet checked on iOS: how the app gets today's date in the device's time zone,
and the document picker (desktop uses GTK file dialogs).

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

**2026-11-04**

- **Reminders are in**, ahead of the specification, which lists them as a later
  feature and notifications as out of scope. Its testing bar still applies (see
  "Reminders" below).
- **Releases**: pushing a version tag publishes a GitHub Release with the
  binaries.

## Milestone 2: finish the MVP dashboard

Agreed 2026-11-03.

| Item | Status |
|---|---|
| Upcoming payments | Done: next 30 days, or 90, grouped by day, each in its own currency. |
| Monthly chart: scheduled cash flow vs monthly-equivalent commitment, kept separate | Done: columns and a line on one axis, six months back to six ahead, in the dashboard's currency, with hover details and a table view. |

Done, in v0.2.0, apart from the chart table below. Checked at phone width
(360 px) and in dark mode in a browser, and in the Android emulator.

**Feedback from 2026-11-04:**

- ~~Notes when adding a subscription~~ (done).
- ~~Tabs~~ (done): Overview, Subscriptions and More, a bar at the bottom on
  phones and a row at the top on wider screens; the cards are unchanged.
- ~~Estimated amounts, totals with a range, due dates, one-off changes to a
  payment~~ (done; see DECISIONS.md, "Estimated amounts, due dates and
  one-off changes"). Snapshot format version 3, backup format version 4.
- Later: the expected amount estimated from past payments (one-off changes
  record what was actually paid, which is what it needs).
- Later, once the user decides what they want: a clearer monthly chart table,
  especially for the current month (part paid, part still due).

## Milestone 3: platforms

Proposed and agreed 2026-11-03. Everything is built; what's left is mostly
trying it on real devices.

| Item | Status |
|---|---|
| Nix package | Done: `nix run github:nicolasdumitru/epicycle`, or in the system configuration (see the README). CI builds it. |
| Android | Builds in CI: a signed release APK on every push and every release. Works in a Pixel 10 emulator, where start-up, exchange rates, safe areas and the keyboard needed fixes. Testers have it; nothing recorded yet from a real phone. |
| Windows | Builds in CI: tests and installers (unsigned, so SmartScreen warns) on every push and every release. Not yet tried on a Windows PC; the user will. |
| iOS | Builds for the simulator in CI. Not yet installed on an iPhone: paused until a friend's Mac is available (see "Open questions"). |
| Releases | Pushing a version tag publishes the Android APK and the Windows installers. v0.1.0; v0.2.0 (2026-11-04: tabs, notes when adding, estimated amounts and ranges, due dates, single-payment changes, what's left to pay); v0.3.0 (2026-11-06: reminders on Android). |
| Emulator tooling | `just android-run` builds the current code and opens it on the emulator, starting one if needed. Also `just android-install`, `just android-reboot` and `just android-eval`. |
| Specification's platform checklist | Partly done. Phone layouts and safe areas are fixed on Android. Still to check on real devices: back navigation with the tabs, touch and keyboard behaviour, and above all backup export and import through the system file picker, which hasn't been tried on any phone. |

## Reminders

Agreed 2026-11-04, released in v0.3.0 for Android; see DECISIONS.md, "Reminders".
Notifications before payments due, separate bills, free trials ending and price
changes, at one or more lead times (default 1 day before at 09:00), set on a
Reminders screen under More, which also asks for permission. The settings are in
the database and in backups (format version 5). Desktop shows the settings but
doesn't schedule, and iOS comes later.

The specification asks for real devices and release builds; so far this has only
run as a debug build in the emulator (2026-11-05 and 06):

- Works: the permission prompt, scheduling, delivery (inexact, within a few
  minutes), delivery after a reboot without opening the app, tapping a
  notification to open the app, rescheduling after changing the settings,
  deleting a subscription or turning reminders off, and allowing notifications
  in the phone's settings and coming back to the app. Fixed on the way: reminders
  were lost at a reboot, stale ones weren't cancelled, and allowing notifications
  in the phone's settings went unnoticed.
- Not tried: time zone and daylight-saving changes, battery optimisation, and
  phones from other makers.
- Known limit: reminders cover 60 days ahead and are only renewed while the app
  runs. A background job will fix this (see "Next steps").

## Next steps

1. **Try v0.3.0 on real Android phones**: reminders, including on other makers'
   phones, and the rest of the platform checklist, especially backups through
   the file picker.
2. **Background job for reminders** (agreed 2026-11-06, after step 1): WorkManager,
   to renew reminders with the app closed. Needs the Rust logic and the database
   to run without the app's window.
3. **Try the Windows installers** on a PC (the user).

Agreed, without a date:

- **A clearer monthly chart table**, especially for the current month (part paid,
  part still due), once the user decides what they want.
- **The expected amount estimated from past payments.** One-off changes already
  record what was actually paid, which is what it needs.
- **A settings screen**: the default reporting currency, which for now changes
  from the dashboard ("Always show totals in …"), and a default folder for
  backups, which start in Downloads on every platform for now.
- **iOS**: install on an iPhone with a friend's Mac, then go through the
  platform checklist there, including the document picker for backups.
  Reminders on iOS follow from that.
- **Reminders on desktop**: only while the app runs, since the plugin can't
  schedule there.

Mentioned by the user, not yet planned:

- **Markdown in notes**, which are plain text for now.
- **A directory of test data** in the repository.
- **Dropping old backup formats** once the format matures.

From the specification's later features, not started:

- Historical spending trends and price-history views.
- Richer analytics: selectable periods, per-category breakdowns, monthly and
  quarterly averages. The averages exist in `crates/core`, with no screen yet.
- Integration with other personal finance tools.
- Optional sync across devices (see "Open questions").

## Open questions

- **Getting Epicycle onto an iPhone** (2026-11-03). CI builds for the simulator,
  with the Xcode project generated on GitHub's macOS runner. The user wants
  no third-party tools such as SideStore and won't pay for the Apple Developer
  Program, which TestFlight needs. iOS is paused until a friend's Mac is
  available; the friend's developer account may also help.
- Sync through cloud drives: if wanted, when, and which providers first?
