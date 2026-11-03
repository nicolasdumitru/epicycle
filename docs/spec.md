# Epicycle: specification

A local-first app for tracking subscriptions, built to understand recurring
expenses and how they change over time, not just to store a list of
subscriptions.

This is the original specification. Decisions made since are in
[status.md](status.md) and [DECISIONS.md](../DECISIONS.md).

## Architecture

The app is built with Tauri 2. The financial domain logic is a separate Rust
crate that does not depend on Tauri, exposed to the UI through Tauri commands.

## Target platforms

- Linux (specifically NixOS)
- iPhone
- iPad
- Android
- Windows
- macOS

Validation order: NixOS and iOS first, then iPadOS, Windows, macOS and Android.
A working Linux desktop build does not mean mobile is ready: each platform must
be tested on its own.

## Core principles

1. **Local-first.** The app works fully offline with local storage. No account,
   no server, no backend required, and it stays fully functional without any
   cloud backend, forever.
2. **Data portability.** Users can always export and import all of their data.
3. **Pure, testable domain logic.** Financial calculations, recurrence handling
   and validation live in a module that is independent of the UI framework and
   the storage layer, so it can be unit-tested in isolation and reused by other
   personal finance tools.

## Financial model: keep these three concepts distinct

The app must never conflate these.

### 1. Current recurring commitment

What subscriptions cost at their current prices, normalised to monthly and
annual equivalents. For a subscription with amount $a$ billed every $n$ months:

$$
\text{monthly equivalent} = \frac{a}{n}, \qquad \text{annualised cost} = \frac{12a}{n}
$$

Weekly, daily or custom intervals need equivalent, well-defined conversions, with
the convention documented.

### 2. Scheduled cash flow

When actual payments occur. Example: an annual subscription costing EUR 120 has a
monthly equivalent of EUR 10, but the cash flow is EUR 120 in its renewal month
and EUR 0 in the other eleven.

### 3. Historical spending

What was paid, or estimated to have been paid, at a given time, using the price
in effect then. Without bank integration, historical amounts are labelled as
estimates, never as verified transactions.

## Data model

- Stable, unique subscription IDs (e.g. UUIDs) that survive export and import.
- Per subscription: name, amount, currency, billing interval, start date, next
  payment date (or enough recurrence information to derive it), and status
  (active, paused, cancelled).
- Explicit recurrence rules that handle month-end edge cases (e.g. a
  subscription started on 31 January), leap years and custom intervals
  deterministically.
- Price history with effective dates, so historical spending uses historical
  prices.
- Money in integer minor units (or a proper decimal type), never binary floating
  point. Currency is always stored explicitly; amounts in different currencies
  are never summed without explicit conversion.
- Schema migrations from day one.

## Currency and exchange rates

- Each subscription keeps its original currency.
- Dashboard totals are shown in one reporting currency.
- Rate source: the European Central Bank Data API,
  https://data.ecb.europa.eu/help/api/data
- Rates are cached locally and refreshed opportunistically when the app runs and
  the network is available.
- Each rate stores its effective date, and the app distinguishes fresh cached,
  stale and manually entered rates. Staleness is shown in the UI.
- The app remains usable offline with cached or manual rates.

## MVP scope

### Subscription management

Create, edit, pause, cancel and delete subscriptions; record price changes into
the price history.

### Financial calculations (fully unit-tested)

- Monthly equivalents
- Annualised costs
- Payment schedules and next payment dates
- Totals over arbitrary periods
- Average spending over selected periods
- Recurrence edge cases
- Currency-aware aggregation

### Dashboard

- Current recurring monthly commitment
- Annualised recurring cost
- Subscription list
- Upcoming scheduled payments
- A basic monthly spending chart that clearly separates scheduled cash flow from
  monthly-equivalent commitment

### Local persistence

A reliable local database with migrations.

### Backup and restore

- Versioned JSON export with a schema version, so newer versions of the app can
  migrate older backups.
- Import:
  - validates the whole file before touching the database (atomic: all or
    nothing);
  - handles unknown or invalid fields safely;
  - offers an explicit choice between merge and replace;
  - never silently duplicates subscriptions, and preserves stable IDs;
  - reports errors clearly and specifically.
- On iOS and Android, the system document picker lets users save backups to
  iCloud Drive, Google Drive or any other file provider, with no server.

### Platform validation

Responsive layouts that work from phone to desktop. Check explicitly: touch
targets, gestures, keyboard behaviour, safe areas and notches, back navigation,
scrolling, accessibility, chart rendering, window resizing, and platform
interaction conventions.

## Later features (after the MVP, roughly in order)

- Historical spending trends and price-history views
- Richer analytics: selectable periods, per-category breakdowns, statistics such
  as monthly and quarterly averages
- Optional reminders and notifications
- Integration with other personal finance tools
- Optional sync across devices
- Recording that something unexpectedly cost more than usual

## Out of scope for now

Notifications, user accounts, sync, bank integration, receipt OCR, automatic
subscription discovery, family accounts, a scripting system, and any backend
service.

## Constraints on future features

Design so that these aren't blocked, but don't build them yet.

### Notifications

They must be tested on real devices with release builds, covering: scheduling,
delivery while the app is closed, tapping to reopen the app, editing, cancelling
and rescheduling, time zone and DST changes, device reboot, Android battery
optimisation and exact-alarm restrictions, and permission denial and later
grant. Desktop and mobile scheduled notifications are not equivalent; be
prepared to fall back to native APIs (UNUserNotificationCenter on Apple
platforms, native scheduling on Android).

### Sync

Keep the data model and domain logic completely independent of any sync system.
