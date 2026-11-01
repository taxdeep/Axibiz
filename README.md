# Axibiz Desktop Beta

Version **0.25.0**. A local Windows desktop workspace for business tasks, invoicing, bills, receipts and accounting, built with Rust, egui/eframe and SQLite.

## Features

- Tasks, customers, vendors, products and services.
- Invoice entry, preview, PDF/printing, receipts, bills and supplier payments.
- Journal entries, general ledger/trial balance, account setup and sales-tax records/reports.
- Date-aware saved exchange rates, background retrieval of missing historical rates and frozen invoice/company snapshots.
- Native multi-window workflows, company/business-editor singleton rules and unsaved-input protection.
- Separate Company Settings and machine-local Software Settings for backups and time display.
- Company-local users with optional password sign-in, Argon2id password verifiers, administrator/user management, persisted retry throttling and guarded desktop/CLI entry.

Existing/new companies default to direct access. Configure optional sign-in under **Setup > Users...**; the initial `admin` account has no password until configured. Credentials and policy follow the company database and backups. Application sign-in is **not database-file encryption**.

## Build and run

Install stable Rust with the Windows MSVC toolchain and the required Microsoft C++ build tools. Run from the repository root:

```powershell
cargo run --locked
```

To use a separately created company database:

```powershell
cargo run --locked -- --db C:\AxibizData\company.db
```

The parent directory must exist. Default company data is created in `database/tasks.db`. Company backup, preference and native-window metadata files are local data, not source assets.

For a validated Windows release:

```powershell
powershell -NoProfile -File build.ps1
```

The script checks formatting, denies Clippy warnings, runs regular tests and builds `dist/Axibiz Desktop Beta.exe`. It retains running/locked retired executables and cleans eligible inactive copies. The VBS launcher starts a locally built executable; binaries are not included here.

## Validation

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

The current regular suite contains 202 passing tests. Two optional integration tests require an installed browser or network access and are skipped by default. Verification history and practical limits are in [AUDIT.md](AUDIT.md).

## Documentation

- [Company users and password/access limits](docs/COMPANY_USERS.md)
- [Company Settings](docs/COMPANY_SETTINGS.md)
- [Software Settings](docs/SOFTWARE_SETTINGS.md)
- [UI interaction audit](docs/UI_INTERACTION_AUDIT.md)
- [Navigation](docs/NAVIGATION.md)
- [General Ledger](docs/GENERAL_LEDGER.md)
- [Sales Tax](docs/SALES_TAX.md)
- [Document template extension plan](docs/DOCUMENT_TEMPLATE_PLAN.md)
- [Change log](CHANGELOG.md)

Historical design/planning documents describe the version indicated in each document. They do not imply every planned capability is implemented. The program supports local company-file isolation, not cross-machine shared-database operation or a certified financial approval system. Tax reports support review/export; they do not submit statutory tax returns.

## Source distribution and licensing status

This source snapshot intentionally excludes company databases, backups, credentials, user/machine preferences, executables, exports, local test outputs and internal screenshots/reference artwork. Test fixtures use synthetic examples. No live company records are needed to build or run the tests.

**All rights reserved. No open-source license is granted for the original Axibiz code.** See [LICENSE](LICENSE). Publication/access alone is not a general software-use or commercial redistribution license; applicable platform terms and statutory exceptions still apply. See [LICENSING.md](LICENSING.md). Third-party dependencies retain their own licenses; a declaration inventory is in [THIRD_PARTY_DEPENDENCIES.md](THIRD_PARTY_DEPENDENCIES.md).
