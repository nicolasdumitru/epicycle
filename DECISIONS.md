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
- iOS builds need macOS with Xcode. We have no Mac yet; a remote macOS build machine
  or a friend's Mac will be used. The Android SDK stays out of the flake until Android
  work starts.

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

## Data history (to be finalised in the storage milestone)

- **Every subscription change is logged forever.** Each change (create, edit, price
  change, status change, delete, import) appends a full snapshot of the subscription
  with a timestamp and cause. Deleting a subscription keeps its history.
- **Merge conflicts on import**: when the same ID exists locally and in the backup,
  the version with the later `updated_at` wins and a tie keeps the local version.
  The import is logged with every conflict and both versions, so the user can switch
  to the other version later.
- **A Replace import first snapshots the whole database**, so it can be undone.
- History stays on the device for now.

### Future direction: history in backups and sync (not built yet)

Eventually the change history will be included in backups and synced across
devices, much like a Git repository with a single branch. Nothing is built for this
yet, but the history design must not rule it out:

- History entries get globally unique IDs (UUIDs), never auto-increment integers,
  so entries made on different devices can't collide.
- Each entry references the entry it replaces (its parent), as Git commits do.
  Then two devices that edited the same subscription can be detected as having
  diverged, rather than one silently overwriting the other.
- Entries are append-only and never rewritten. Undoing a change appends a new
  entry.
- Each entry records the device it was made on and when.
- Merging two diverged histories is the same problem as merge conflicts on import,
  so the two should share one mechanism.
