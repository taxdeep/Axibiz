# Source Verification Record

This is a curated verification summary for the source snapshot. Internal company-data reports, customer screenshots, backup files, local paths and raw diagnostic outputs are not distributed.

## Current baseline: 0.25.0 / company schema 21

- Formatting and Clippy with warnings denied passed. The regular suite has 202 passing tests, with two optional external integrations skipped by default.
- Coverage includes financial balance/period/source rules, idempotent and atomic settlements, rollback/numbering/audit, historical snapshots, migration preservation, company-window isolation and unsaved input.
- Optional company sign-in covers separate company credentials/sessions, administrator/own-password restrictions, persisted retry throttling, no password/hash leakage into audit JSON, credential transaction failure, backup restore reauthentication and CLI gating.
- Actual large/small desktop-widget checks cover signed-out content hiding and reachable form actions. Physical mixed-DPI and screen-reader acceptance are separate; no certification is claimed.
- Release noninteractive access to a disposable protected company was rejected without modifying the fixture. Isolated legacy upgrades preserved original saved columns, integrity and foreign keys without altering source files. Existing historical review findings require the operator's attention rather than being silently rewritten.
- Company databases/backups remain unencrypted. OS access protection is required. Administrator/User roles do not implement financial approval or segregation of duties.

## Source-publication preparation

Only source, build scripts, templates and curated technical documents are staged. Company records, backups, credentials, test outputs, binaries, local configuration and reference artwork are excluded. The retained UI test fixture was checked after replacing reference-company labels with generic examples. The owner approved public publication with rights reserved and no open-source license. Third-party licenses remain unchanged.
