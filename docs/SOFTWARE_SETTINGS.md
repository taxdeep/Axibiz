# Software Settings

The navigation pane's **Settings** (gear) manages software behavior. **Setup > Company Settings** manages company identity, contact, legal/tax identifiers, default business currency and reporting references.

## Backups

- **Create a daily backup on startup** enables the existing once-per-day startup backup. Default: enabled. This is a startup backup, not a background timer; backup settings take effect on the next startup. Its day follows the existing America/Vancouver business-day rule.
- **Backup directory** chooses a common root; blank uses `backups` beside each company database. Saving a custom directory checks that it can be created/written. Browse selects a directory; Use Default Directory clears the override. Existing backups are not moved or removed when changing roots.
- Automatic daily and upgrade copies use a database-path-specific subfolder to separate companies. The File > Backup dialog starts at the configured root; restore safety copies also use that root. Manual backup filenames remain under the user's control.
- **Daily backups to keep** accepts 1–365; default 14. Rotation runs during startup backups. Required pre-upgrade copies remain enabled even when daily backup is off; the existing upgrade retention is five copies. A failed required backup/verification stops opening/migration before changing company data.
- Rotation accepts only generated daily/upgrade filename formats, skips directories/links and sorts by the actual timestamp. It does not rotate manual backups, restore safety copies or merely similarly prefixed filenames. Disabling daily backups does not clean up daily files.

## Time display

Choose a 24-hour or 12-hour AM/PM clock and America/Vancouver, system local time or UTC. Preview reflects the draft; Audit Log uses saved preferences immediately after Save. Displayed offsets distinguish repeated local times at the DST transition.

Only timestamp presentation changes. Stored UTC event times, audit JSON, filenames, document dates, posting/lock dates and exchange-rate observation dates retain their original meaning. Unrecognized legacy timestamp text is displayed unchanged.

## Persistence and safeguards

On Windows, settings are stored at `%LOCALAPPDATA%\Axibiz Desktop Beta\preferences.json` (fallback: APPDATA/user configuration directory). They apply across company files for the current computer user, independently of company database backups/restores. Sidebar and native-window layout preferences retain their existing company-side files.

Missing preferences use safe defaults. Invalid/unreadable files are preserved and reported; startup does not silently treat corruption as an instruction to disable backup. Save validates a draft, writes/syncs a unique temporary file, and atomically replaces the preferences file. A failed save leaves the saved preference state intact and keeps the draft available to correct. Revert requires a discard choice. Software and company drafts independently participate in exit/restore protection.

Software preferences add no company tables. The current company schema is **21**, including separate company users; software preferences still cause no settings/audit/business-table writes. Verification covers atomic replacement/failure preservation, customized startup backups/retention, disabling daily backups while retaining mandatory upgrade protection, DST/UTC formatting, route/draft/restore separation, and actual small/large widget layouts.
