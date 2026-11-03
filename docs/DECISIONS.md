# Design decisions

Short records of choices that are non-obvious or expensive to reverse.
Newest decisions are appended at the end of each section.

## Stack

- **Tauri 2** shell. The domain logic lives in Rust crates that don't depend on Tauri.
- **Frontend: SvelteKit + Svelte 5 + TypeScript**, built as a static single-page app
  (`adapter-static`, `ssr = false`). SvelteKit is used for routing, which mobile back
  navigation needs. TypeScript types for domain data will be generated from the Rust
  types with `ts-rs`, so Rust remains the single source of truth.
- **Charts are hand-written SVG** until a chart library clearly earns its place.
- **pnpm**, with every version pinned exactly and `minimumReleaseAge` set to 7 days
  (`ui/pnpm-workspace.yaml`). An urgent security fix can be exempted via
  `minimumReleaseAgeExclude`.
- **TypeScript 6.x.** SvelteKit 3 and svelte-check don't support TypeScript 7 yet.
- **Storage: SQLite through `sqlx`**, chosen for compile-time checked queries. The
  offline query metadata (`.sqlx/`) is committed so that CI and the Nix build need
  no live database (`SQLX_OFFLINE=true`).
- **Layout:** Rust crates live in `crates/` and the frontend in `ui/`. The root
  `Cargo.toml` is a workspace manifest only. There are two crates: `crates/core`
  (pure domain logic; the crate boundary guarantees it can't use Tauri or SQL) and
  `crates/app` (Tauri commands plus storage). Storage is a module of the app crate
  until a second program, such as a CLI, needs the database.
- **App identifier: `io.github.nicolasdumitru.epicycle`.** It determines the data
  directory on every platform and the iOS bundle ID, so it must not change.

## Platforms

- Validation order: **NixOS desktop, then iOS**, then iPadOS, Windows, macOS, Android.
- iOS builds need macOS with Xcode, which isn't available yet; a remote macOS build
  machine will be used. The Android SDK stays out of the flake until Android work
  starts.

## Domain

### Money
- Amounts are integer minor units (`i64`) paired with an ISO 4217 currency. Amounts
  in different currencies are never added without an explicit conversion.
- Exchange rates and derived values (such as the monthly equivalent of a weekly
  price) are exact decimals. They are rounded half-to-even only for display.

### Recurrence
Billing rules are an algebraic data type. Calendar rules and duration rules are kept
separate:

| Rule | Meaning |
|---|---|
| `Days { every }` | Every *n* days. A pure duration. |
| `Weeks { every }` | Every *n* weeks, on the anchor's weekday. |
| `Months { every, day }` | Every *n* months on a fixed day of the month (1–31). |
| `Years { every, month, day }` | Every *n* years on a fixed date. |

- **Day 31 means "the last day of the month".** Clamping makes it fall on the last
  day of every month, so a separate "last day" option would be a second way of
  writing the same rule.
- **Weekly rules don't store a weekday.** It always equals the anchor's weekday,
  and storing it twice would allow the two to disagree.

- Each rule comes with an **anchor**: one real payment date that fixes the phase,
  i.e. which month or week the "every *n*" counts from. A smart constructor rejects
  an anchor that doesn't satisfy the rule.
- The *k*-th payment is computed directly as `anchor + k × interval`, never from the
  previous payment, so dates can't drift. A day that doesn't exist in a month is
  clamped to that month's last day: day 31 falls on Feb 28/29, then on Mar 31 again.
  Feb 29 falls on Feb 28 in years that aren't leap years.
- The billing day is stored explicitly rather than read from the anchor date.
  Otherwise "the 28th" and "the last day" would be indistinguishable when the
  anchor is Feb 28.
- The next payment date is always derived, never stored.

### Normalising to monthly and annual equivalents
- Month and year rules convert exactly: amount *a* every *n* months is `12a/n`
  per year.
- Day and week rules use the **mean Gregorian year, 365.2425 days** (52.1775 weeks
  per year, 30.436875 days per month). A commitment is a long-run rate, so it needs
  an average year. 52 weeks per year would understate weekly costs by about 0.3%.
- Scheduled cash flow uses no averages: it counts the actual payment dates in a
  period, so a year with 53 weekly payments shows 53.

### Subscriptions: plan history and status history
- **A subscription keeps a history of plans, not only of prices.** A plan is a
  billing schedule plus an amount, in effect from a date until the next plan.
  Switching from monthly to yearly billing, or moving the billing day after a
  pause, is a new plan. A price change is a new plan with the same schedule.
  Without this, editing a schedule would silently rewrite every past estimated
  payment.
- **The first plan's date is the subscription's start.** No payments are counted
  before it.
- **Status changes take effect on their date.** A subscription is active until the
  first change. A payment that falls on the day a pause or cancellation takes
  effect is not counted, and one on the day it becomes active again is.
- **A subscription's currency is fixed.** Changing currency means creating a new
  subscription.
- Every period is half-open, `[from, to)`, so consecutive periods never count a
  payment twice.
- **Notes are part of the subscription**, plain text for now. Editing them records
  a revision like any other change, and they travel in backups and merge with the
  rest of the subscription. Line endings are normalised and trailing whitespace
  dropped, so the same text always compares equal.

### Exchange rates
- **Rates are stored the way the ECB publishes them:** units of a currency per
  1 EUR, with the date the rate applies to and its source (ECB or manual). Other
  pairs are converted through EUR.
- **Choosing a rate for a date:** the latest one on or before that date. If there
  is none that old, the earliest one after it is used. Every conversion reports
  the rates it used, so the UI can show their dates.
- **A manual rate wins over an ECB rate for the same currency and date.**
- **An ECB rate is stale once it is more than 5 days old.** That covers the longest
  regular gap between ECB publications: Thursday to Tuesday over Easter. Manual
  rates are always shown as manual, never as fresh or stale.

## Data history

- **Every subscription change is logged forever.** Each change (create, edit, price
  change, status change, delete, import) appends a revision: a full copy of the
  subscription, with when, on which device and why. Deleting a subscription keeps
  its history.
- **History works like a Git repository with a single branch.** Revisions have
  UUIDs, never auto-increment integers, so revisions made on different devices
  can't collide. Each names the revision it replaces (its parent); a revision that
  settles a conflict names both sides. Revisions are never rewritten: undoing a
  change appends a new one.
- **A subscription's latest revision is the one no other revision replaces**, not
  the newest by ID or by time, which device clocks can't be trusted for.
- **Importing a backup and, later, syncing devices are the same operation**:
  merging one history into another (see "Backups").

### Future direction: syncing devices (not built yet)

Backups already carry the whole history and imports merge it, which is what
syncing needs. What's missing is moving the history between devices
automatically.

- **Under consideration: syncing through storage the user already has** (Nextcloud,
  iCloud Drive, Google Drive). If we do this, the
  SQLite database must never sit in the synced folder: file-sync tools copy whole
  files without locking, and two devices writing to one database corrupt it or
  leave "conflicted copy" files. What would sync is the history: each device
  writes its new revisions as files to a shared folder and merges in the other
  devices' files, the same way an import does.

## Backups

- **A backup holds the whole change history and the settings**, as JSON. There is
  no separate list of current subscriptions: the current state is each
  subscription's latest revision, so the file can't contradict itself. Including
  the history from the first version avoids supporting a second, history-less
  format forever, and it is what lets imports tell outdated copies from conflicts.
- **Format**: `{"format": "epicycle-backup", "version": 1, "exported_at",
  "device_id", "settings", "revisions": [...]}`. Revisions are written parents
  first. Amounts are integer minor units, dates ISO 8601, timestamps RFC 3339.
  `crates/core/testdata/` has a sample of each version; tests check that every
  one still imports, and that the latest exports unchanged.
- **Version 2** (2026-11-03) adds `notes` to subscriptions, left out when empty.
  Version 1 files are read as version 2 files without notes. The version went up
  even though nothing else changed, so that older apps refuse these files rather
  than silently dropping the notes.
- **Versioning**: `version` covers the whole file and goes up on any change to
  the format. The app reads every older version and refuses newer ones with a
  message to update the app.
- **Validation comes first.** The whole file is checked before the database is
  touched: format and version, every subscription (the same rules as the app's own
  data), each revision's subscription ID matching its contents, every parent
  present in the file, no cycles, one latest revision per subscription, and no
  revision ID that the device has with different contents. The import itself is
  one transaction: all of it applies or none of it does. Before importing, the app
  shows what the import will do.
- **Merge import compares histories, subscription by subscription.** This
  replaces the earlier plan of "the later `updated_at` wins", which brought deleted
  subscriptions back when an old backup was imported, and let a device with a
  wrong clock overwrite newer edits.
  - Same latest revision on both sides: nothing to do.
  - The backup's version is an older point in this device's history: keep this
    device's. In particular, deleted subscriptions stay deleted.
  - This device's version is an older point in the backup's history: take the
    backup's.
  - Both changed it separately (a conflict): the later change wins and a tie keeps
    this device's. A new revision with both sides as parents records the result,
    so the other version stays in the history and can be restored. The import
    summary lists each conflict and which side was kept. Identical separate
    changes are joined the same way but not reported as conflicts.
  - Subscriptions only on this device are kept; only in the backup, added.
  - Settings stay as they are on this device.
- **Replace import makes this device match the backup, but erases nothing.** This
  replaces the earlier plan of saving a copy of the database first: the history
  already keeps everything, which matches "keep data forever".
  - Subscriptions only on this device get a deletion revision.
  - Subscriptions where this device differs get a new revision with the backup's
    version, on top of this device's history (with both parents if they diverged).
  - Settings come from the backup.
  - Anything replaced can be restored from its history.

## Storage

- **The current state is stored in normalised tables** (`subscriptions`, `plans`,
  `status_changes`) with `CHECK` constraints, so SQLite rejects invalid rows, and
  the compile-time checked queries have real columns to check.
- **History is the append-only `revisions` table.** Each revision holds a full JSON
  copy of the subscription, tagged with a format version so old copies can be
  upgraded when the format changes. A deletion is a revision with no copy.
- **Revision IDs are UUIDv7**, so they sort roughly by creation time. Parent
  links, not IDs or `created_at`, define the order of a history.
- **A revision with two parents** stores the second in `merge_parent_id`: two
  columns rather than a separate parents table, since there are never more than
  two.
- **Saving a subscription that didn't change records no revision.**
- **Snapshot version 2** adds notes, left out of the JSON when empty, so
  version 1 snapshots read unchanged and need no upgrade. Leaving empty notes out
  also keeps old and new snapshots of the same subscription byte-identical, which
  the "no change, no revision" check relies on.
- **Builds use the committed query metadata in `.sqlx/` and need no database.**
  After changing a query or a migration, run `just sqlx-prepare`
  and commit the result. CI runs it with `--check`.
