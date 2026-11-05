//! Keeping the phone's scheduled reminders in step with the data.
//!
//! The core works out which reminders are due (`epicycle_core::reminders`);
//! here a background task replaces the scheduled notifications with them
//! whenever something changes, and every few hours as time passes. Only on
//! Android for now: on desktop the notification plugin can't schedule, and iOS
//! hasn't been tried (see docs/DECISIONS.md, "Reminders").

use std::sync::Arc;
use std::time::Duration;

use epicycle_core::money::Money;
use epicycle_core::reminders::{Reminder, ReminderKind, ReminderSettings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Notify;
use ts_rs::TS;

use crate::commands::CommandError;
use crate::db::DbHandle;

/// How far ahead reminders are scheduled, and at most how many: Android
/// limits how many alarms an app may set.
pub const DAYS_AHEAD: i32 = 60;
pub const AT_MOST: usize = 60;

/// Even without changes, reschedule this often, so the window moves on.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Asks the background task to reschedule. Cheap: several requests while it
/// is busy run it once more.
#[derive(Clone, Default)]
pub struct Rescheduler(Arc<Notify>);

impl Rescheduler {
    pub fn request(&self) {
        self.0.notify_one();
    }
}

/// Nudges the rescheduler after a change, if it has started.
pub fn changed(app: &AppHandle) {
    if let Some(r) = app.try_state::<Rescheduler>() {
        r.request();
    }
}

/// The reminder settings, as the settings screen shows and edits them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReminderSettingsView {
    pub enabled: bool,
    /// Days before each event; at most 5, each 0–365.
    pub lead_days: Vec<u16>,
    /// Local time of day, "HH:MM".
    pub time_of_day: String,
}

/// Whether the app may show notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NotificationAccess {
    Granted,
    Denied,
    /// Not asked yet: requesting shows the system's prompt.
    Prompt,
    /// Scheduled reminders aren't available on this platform yet (desktop).
    Unsupported,
}

const MAX_LEAD_TIMES: usize = 5;

fn view(settings: &ReminderSettings) -> ReminderSettingsView {
    ReminderSettingsView {
        enabled: settings.enabled,
        lead_days: settings.lead_days.clone(),
        time_of_day: settings.time_of_day.strftime("%H:%M").to_string(),
    }
}

/// Checks settings as entered.
pub fn settings_of(view: &ReminderSettingsView) -> Result<ReminderSettings, CommandError> {
    let time_of_day = jiff::civil::Time::strptime("%H:%M", view.time_of_day.trim())
        .map_err(|_| CommandError::from("The time must look like 09:00"))?;
    let settings = ReminderSettings {
        enabled: view.enabled,
        lead_days: view.lead_days.clone(),
        time_of_day,
    }
    .normalized();
    if settings.lead_days.iter().any(|&d| d > 365) {
        return Err("A reminder can be at most 365 days ahead".into());
    }
    if settings.lead_days.len() > MAX_LEAD_TIMES {
        return Err(format!("At most {MAX_LEAD_TIMES} reminders per event").into());
    }
    Ok(settings)
}

#[tauri::command]
pub async fn reminder_settings(
    db: State<'_, DbHandle>,
) -> Result<ReminderSettingsView, CommandError> {
    Ok(view(&db.ready().await.reminder_settings().await?))
}

#[tauri::command]
pub async fn set_reminder_settings(
    app: AppHandle,
    db: State<'_, DbHandle>,
    settings: ReminderSettingsView,
) -> Result<ReminderSettingsView, CommandError> {
    let settings = settings_of(&settings)?;
    let db = db.ready().await;
    db.set_reminder_settings(&settings).await?;
    changed(&app);
    Ok(view(&db.reminder_settings().await?))
}

#[cfg(mobile)]
fn access(state: tauri_plugin_notification::PermissionState) -> NotificationAccess {
    use tauri_plugin_notification::PermissionState;
    match state {
        PermissionState::Granted => NotificationAccess::Granted,
        PermissionState::Denied => NotificationAccess::Denied,
        _ => NotificationAccess::Prompt,
    }
}

#[tauri::command]
pub fn notification_access(app: AppHandle) -> Result<NotificationAccess, CommandError> {
    #[cfg(mobile)]
    {
        use tauri_plugin_notification::NotificationExt;
        Ok(access(app.notification().permission_state()?))
    }
    #[cfg(desktop)]
    {
        let _ = app;
        Ok(NotificationAccess::Unsupported)
    }
}

/// Shows the system's prompt (if it still can) and schedules reminders once
/// allowed.
#[tauri::command]
pub async fn request_notification_access(
    app: AppHandle,
) -> Result<NotificationAccess, CommandError> {
    #[cfg(mobile)]
    {
        use tauri_plugin_notification::NotificationExt;
        let state = access(app.notification().request_permission()?);
        changed(&app);
        Ok(state)
    }
    #[cfg(desktop)]
    {
        let _ = app;
        Ok(NotificationAccess::Unsupported)
    }
}

/// "30.00 EUR": plain, and the same on every platform.
fn money(m: Money) -> String {
    format!("{} {}", m.to_major(), m.currency.code())
}

/// "today", "tomorrow", "in 3 days".
fn when(lead_days: u16) -> String {
    match lead_days {
        0 => "today".into(),
        1 => "tomorrow".into(),
        n => format!("in {n} days"),
    }
}

/// The notification's title and text.
pub fn text(r: &Reminder) -> (String, String) {
    let day = r.date.strftime("%b %-d");
    let when = when(r.lead_days);
    let body = match r.kind {
        ReminderKind::PaymentDue => format!("{} due {when} ({day})", money(r.amount)),
        ReminderKind::Billed => format!("{} billed {when} ({day})", money(r.amount)),
        ReminderKind::TrialEnds => {
            format!("Free period ends {when} ({day}); then {}", money(r.amount))
        }
        ReminderKind::PriceChange => {
            format!("New price from {when} ({day}): {}", money(r.amount))
        }
    };
    (r.name.clone(), body)
}

/// Starts the background task and returns the handle to nudge it.
pub fn spawn(app: AppHandle) -> Rescheduler {
    let rescheduler = Rescheduler::default();
    let signal = rescheduler.0.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // Failures (e.g. permission revoked) are retried next time.
            if let Err(error) = reschedule(&app).await {
                eprintln!("Couldn't schedule reminders: {error}");
            }
            tokio::select! {
                _ = signal.notified() => {}
                _ = tokio::time::sleep(EVERY) => {}
            }
        }
    });
    rescheduler
}

/// A reminder as the Android plugin takes it, with `sourceJson` filled in.
///
/// The plugin saves each notification's `sourceJson` to restore it after a
/// reboot, and to know what is pending, but never fills it in itself (2.8.0):
/// it has to come with the notification, as the notification's own JSON.
#[cfg(any(target_os = "android", test))]
fn batch_entry(
    id: i32,
    title: String,
    body: String,
    schedule: tauri_plugin_notification::Schedule,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut entry = serde_json::json!({
        "id": id,
        "title": title,
        "body": body,
        "schedule": serde_json::to_value(schedule)?,
    });
    entry["sourceJson"] = entry.to_string().into();
    Ok(entry)
}

/// The Android side of the notification plugin, for its `batch` command: only
/// that one saves notifications, and the plugin's Rust API only offers `show`,
/// which sets the alarm without saving it. Unsaved reminders are lost at a
/// reboot and can't be cancelled.
#[cfg(target_os = "android")]
struct AndroidNotifications(tauri::plugin::PluginHandle<tauri::Wry>);

/// A second, private instance of the plugin's Android class, for that handle.
#[cfg(target_os = "android")]
pub fn android_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("epicycle-notifications")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin("app.tauri.notification", "NotificationPlugin")?;
            app.manage(AndroidNotifications(handle));
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
async fn reschedule(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_notification::{NotificationExt, PermissionState, Schedule};

    let notifications = app.notification();
    if notifications.permission_state()? != PermissionState::Granted {
        return Ok(());
    }
    let handle = app.state::<DbHandle>();
    let db = handle.ready().await;
    let settings = db.reminder_settings().await?;
    let subscriptions = db.subscriptions().await?;
    let now = jiff::Zoned::now();
    let due = epicycle_core::reminders::reminders(
        &subscriptions,
        &settings,
        now.datetime(),
        DAYS_AHEAD,
        AT_MOST,
    );

    let entries = due
        .iter()
        .map(|r| {
            let at = r.at.to_zoned(now.time_zone().clone())?.timestamp();
            let (title, body) = text(r);
            let schedule = Schedule::At {
                date: time::OffsetDateTime::from_unix_timestamp(at.as_second())?,
                repeating: false,
                // Fire even while the phone dozes.
                allow_while_idle: true,
            };
            Ok(batch_entry(r.id(), title, body, schedule)?)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    // Only this app's own reminders are ever saved, so replace them all. An
    // empty object cancels every saved one; the plugin's `cancel_all` sends
    // null, which its Android side can't parse (2.8.0).
    let plugin = &app.state::<AndroidNotifications>().0;
    plugin.run_mobile_plugin::<()>("cancel", serde_json::json!({}))?;
    if !entries.is_empty() {
        plugin.run_mobile_plugin::<serde_json::Value>(
            "batch",
            serde_json::json!({ "notifications": entries }),
        )?;
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
async fn reschedule(_app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use epicycle_core::money::Currency;
    use epicycle_core::subscription::SubscriptionId;
    use jiff::civil::date;

    fn reminder(kind: ReminderKind, lead_days: u16) -> Reminder {
        Reminder {
            subscription: SubscriptionId::generate(),
            name: "Vodafone".into(),
            kind,
            date: date(2026, 11, 20),
            lead_days,
            at: date(2026, 11, 19).at(9, 0, 0, 0),
            amount: Money::new(3000, Currency::EUR),
        }
    }

    #[test]
    fn checks_settings_as_entered() {
        let entered = |lead_days: Vec<u16>, time: &str| ReminderSettingsView {
            enabled: true,
            lead_days,
            time_of_day: time.into(),
        };
        let ok = settings_of(&entered(vec![3, 1, 3], "08:30")).unwrap();
        assert_eq!(ok.lead_days, [1, 3]);
        assert_eq!(ok.time_of_day, jiff::civil::time(8, 30, 0, 0));
        assert!(settings_of(&entered(vec![1], "8.30")).is_err());
        assert!(settings_of(&entered(vec![400], "09:00")).is_err());
        assert!(settings_of(&entered(vec![1, 2, 3, 4, 5, 6], "09:00")).is_err());
    }

    #[test]
    fn batch_entries_carry_their_own_json() {
        let date = time::OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let schedule = tauri_plugin_notification::Schedule::At {
            date,
            repeating: false,
            allow_while_idle: true,
        };
        let mut entry = batch_entry(7, "Vodafone".into(), "Due".into(), schedule).unwrap();
        let source = entry["sourceJson"].as_str().unwrap().to_owned();
        entry.as_object_mut().unwrap().remove("sourceJson");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&source).unwrap(),
            entry
        );
        assert_eq!(entry["schedule"]["at"]["allowWhileIdle"], true);
    }

    #[test]
    fn says_what_and_when() {
        assert_eq!(
            text(&reminder(ReminderKind::PaymentDue, 1)),
            ("Vodafone".into(), "30.00 EUR due tomorrow (Nov 20)".into())
        );
        assert_eq!(
            text(&reminder(ReminderKind::Billed, 3)).1,
            "30.00 EUR billed in 3 days (Nov 20)"
        );
        assert_eq!(
            text(&reminder(ReminderKind::TrialEnds, 0)).1,
            "Free period ends today (Nov 20); then 30.00 EUR"
        );
        assert_eq!(
            text(&reminder(ReminderKind::PriceChange, 1)).1,
            "New price from tomorrow (Nov 20): 30.00 EUR"
        );
    }
}
