-- Reminder settings (see docs/DECISIONS.md, "Reminders"): on by default, at
-- 09:00 local time, one day before each event.
ALTER TABLE app_state ADD COLUMN reminders_enabled INTEGER NOT NULL DEFAULT 1
    CHECK (reminders_enabled IN (0, 1));
-- Local time of day, HH:MM:SS.
ALTER TABLE app_state ADD COLUMN reminders_time TEXT NOT NULL DEFAULT '09:00:00';

-- Days before each event that a reminder goes out, one row each.
CREATE TABLE reminder_lead_days (
    days INTEGER PRIMARY KEY NOT NULL CHECK (days BETWEEN 0 AND 365)
) STRICT;
INSERT INTO reminder_lead_days (days) VALUES (1);
