# UI and Interaction Audit — 0.20.0

0.21.0 follow-up: the [designer review](UI_DESIGN_REVIEW.md) is now partly implemented. Shared vector icons, bounded row targets/text cells, consistent workflow footers, Bill outcome selection, advanced columns, compact receipt summary, explicit reset/allocation confirmation and contextual cancel labels are delivered. The baseline matrix below documents the 0.20.0 review; current release evidence is at the top of [AUDIT.md](../AUDIT.md). The future [template system plan](DOCUMENT_TEMPLATE_PLAN.md) is separate from shipped functionality.

Date: 2026-10-31. Baseline: 0.19.0 / schema 19. Delivery: 0.20.0 / schema 19.

This is the implementation review accompanying [the design language](UI_DESIGN_LANGUAGE.md). It covers every registered page, business-window category and supporting dialog family. Source inspection, automated interaction tests and widget raster evidence are different levels of evidence; this document does not claim that every possible user sequence has been manually exercised.

## Review rules

For each module, review entry points, selection/search, draft input, validation, save/commit, clear/cancel, native X/Escape, singleton reuse, failures and related financial/export actions. Apply compact shared typography and control sizing; keep totals and final actions outside long scrolling bodies. Use English for modified action text. User names, notes, addresses and historical document snapshots retain their original text.

Rendering and review must not allocate numbers, create records or post accounting entries. Saving master data or a journal draft is distinct from issuing, recording, posting and reversing a financial event. Final financial validation remains in the corresponding domain transaction.

## Reusable structure

| Responsibility | Shared implementation | Consumers / boundary |
| --- | --- | --- |
| Page title, description, create entry, navigation | `src/gui/commands.rs` | All 16 pages; module query and row selection restored during navigation. Numbering drafts survive leaving/returning. |
| Typography, colors, control/row height, trash icon | `theme.rs`, `invoice_sheet.rs` | Root and native views; compact controls/rows use 28 logical pixels. Editable table cells retain inner padding. |
| Search and clear-search control | `controls::search` | Main registers, party centers and transaction search. Ctrl+F focuses the first visible search; clear-search uses ×, which is distinct from deleting a record. |
| Read-only register | `controls::register`, `invoice_sheet::register_columns` | Products, accounting lists, audit/tax/GL tables using the generic table wrapper. Specialized party and financial registers retain typed domain columns. |
| Primary action and bounded error | `controls::primary`, `controls::error` | Shared actions/review dialogs; green indicates the current explicit commit action, never its validity by itself. |
| Review and explicit confirmation | `controls::review`, `review_content` | Receipts, supplier payments, voids, Bill, journal post/reverse, task cancellation/expense, tax cash/void, restore, legacy currency and app exit. Safe initial focus is Keep Editing; Escape keeps the draft. |
| Discard protection | `controls::CloseGuard`, `windows.rs` | Product creation/edit, template close/reset, Bill/JE clear and tax-record switching use shared guards; business windows retain their shared return/save/discard flow. |
| Party workspace and profile input | `party_ui`, `party_transactions`, `party_data`, `people_ui` | Customer and Vendor share layout/filter/selection/export, responsive fields, profile categories and cache invalidation. Their domain records and balances stay separate. |
| Document/settlement tables | `invoice_sheet`, existing document/settlement models | Invoice, Bill, Receive Payment, Pay Bills and JE share sizing and scroll infrastructure. Column rules remain specific to each business document. |
| Settlement safeguards | `posting.rs` | Receipts, supplier payments and tax cash bank checks. No generic presentation helper can bypass source/date/currency/account validation. |

Removed obsolete Customer/Vendor branches from the generic create-record dialog. Those entry points now use their dedicated profile editors. Removed the invoice's separate inline discard behavior, the JE's unguarded Escape close, duplicate simple table rendering, repeated page-heading metadata and direct receipt/supplier-void native confirmation dialogs. The status bar says Ready instead of claiming all current input is saved. Manual-rate guidance now describes saved manual-rate priority correctly.

## Complete page review matrix

All rows received source review and empty/populated read-only render regression at 900×650, 1440×900 and 1920×1080 logical pixels. Additional evidence is identified below.

| Page | Reviewed interactions and resulting behavior | Specific verification / remaining boundary |
| --- | --- | --- |
| Tasks | Search/select/edit, new task, service lookup/create, line changes, Save, Complete, Cancel, invoice conversion. Cancel no longer saves pending changes first. A new task-linked expense requires review before saving; the source save remains atomic. | Existing task/lookup/failed-save tests; task financial input signature is rechecked under the write transaction. Cancelling a task retains already recorded expense history. |
| Customers | Active/all/inactive, search, selection, Transactions/Contacts/Notes/Tasks, new/edit profile, new Invoice/Task, Receive Payment, filters and CSV. | Shared party/cache/typed-balance tests and actual wide/narrow center/editor rasters. Unknown/read-error balances are not treated as verified zero. |
| Vendors | Same master/detail language; new/edit, notes/catalog, Enter Bill, Pay Bills, transaction filters and CSV. | Shared party tests; supplier profile/default-account/AP tests; wide/narrow center/editor rasters. Recorded expenses are not labelled as an authoritative payable balance without source reconciliation. |
| Products & Services | Search, row Edit, create from list/task lookup, price/currency validation, Save, Save & Close, Escape/discard. | Actual pointer test verifies correct row, failed input retention, saved values and explicit discard. Obsolete party creation paths removed. |
| Exchange Rates | Saved/manual rate maintenance, automatic missing dated lookup, queue/error/offline reuse. Invoice has no manual online-refresh action. | Historical/manual/current priority, queue and snapshot regressions; real Bank of Canada lookup followed by offline reuse. Manual rates remain deliberate input. |
| Company Settings | Setup > Company Settings: grouped company/legal/contact/registration/reporting references, default business currency and legacy currency labels. | Saved baseline participates in exit/restore checks. Bulk legacy labelling requires explicit review with frozen currency and blocks while other editors/drafts are open. It changes labels, not amounts. |
| Software Settings | Fixed navigation gear: backup directory, daily startup backup enablement/retention, 12/24-hour and time-zone display. | Machine-user-local atomic JSON preferences; custom-directory write checks; saved/draft separation and exit/restore protection. Configured backup/off/mandatory-upgrade and DST tests; large/small actual widget captures. No company-table writes. |
| Company Users | Setup > Users: account/profile/password management, optional company sign-in, active/administrator status; File > Lock / Switch User. | Company-bound sessions; administrator/own-password checks; current-password confirmation; no deletion/last-admin lockout; transactional secret-free audit; retry throttling; guarded GUI/CLI and reauthentication after restore. Actual signed-out captures hide business navigation. |
| Bills | Search/open, supplier/default lines, date/reference/terms/memo, tax/recovery, linked Tasks, Clear, Review Bill / Review & Close / Review & New. | Read-only review plus changed-input/role/period rollback tests. Final save rechecks the review signature under an Immediate transaction, preserving numbering on failure. Existing historic tax/FX and paid-bill restrictions remain. |
| Chart of Accounts | Search, create/edit code/name/type/active, Save/Save & Close, close guard. | Existing native-close and account-control tests; control/used-account restrictions remain in domain/DB rules. |
| Invoices | Search/paging/open, direct and completed-task creation, date/currency/tax preview, explicit issue, source link, receipt history, Receive Payment, PDF/Print. | Existing review/source/idempotency/failed-draft tests, real PDF and overwrite refusal. Direct form Cancel/X share the same guard. Escape from preview returns to editing. |
| Document Numbering | Prefix/digits/next-value edit and Save. Navigation retains pending changes; exit/restore checks include them. | Navigation regression verifies draft survives and restore reset clears stale state; existing numbering collision/atomic allocation tests. |
| Audit Log | Search/read-only register and historical record inspection. | Read-only render test; existing append-only audit and rollback tests. Audit cannot be edited through this view. |
| Sales Tax | Cutoff reports, detail/exceptions exports, cash payment/refund/adjustment review, dated void. | New read-only/stale-bank/amount/locked/future-date tests plus existing cutoff/reversal regressions. Tax cash validates a CAD-compatible bank; records accounting events and does not send money or file returns. |
| Sales Tax Setup | Agency/registration/accounts, effective tax rates, tax-code composition, default assignment, Save and record switching. | Saving one category cannot mark another category saved. Agency/code switching protects unsaved values; Escape retains them. Existing effective-rate/assignment/registration tests remain. |
| Journal Entries | Search/open/new, row account/party/debit/credit/memo/tax, attachments, saved rate, Save Draft, review Post, review Reverse, Clear. | Read-only posting preview and locked-period final-save regression; draft vs posted close tests. Close never posts or reverses; posted records retain their source restrictions. |
| General Ledger | Ledger/trial balance, account/date selection, source drilldown and CSV. | Existing cutoff/reconciliation/foreign-carrying-value tests; read-only rendering at all sizes. CSV uses shared formula-safe text encoding. |
| GL Setup | Base currency, control-account roles, period lock and Save. | New saved baseline protects draft settings on exit/restore; existing configuration/domain/trigger tests protect used accounts and base currency. |

## Business windows and supporting dialogs

The existing 11 singleton categories remain: Task, Invoice, Accounting, Journal, Ledger, Reconciliation, Checks, Payment, Customer, Vendor and Pay Bills. Invoice create/view share a slot; Bill and Account share Accounting. Reopening focuses the existing window and preserves its draft. Read-only windows do not acquire an editing lock merely by being open.

- **X / close:** native title-bar X remains the ordinary close control. Native Ctrl+W uses the same guarded route when no modal is open. Save & Close is a separate operation. Embedded dialogs keep the close/cancel control they need. The existing central close dialog supports only valid save paths; Bill and settlement close routes go through review rather than silently recording.
- **Receive Payment:** remains scoped to one invoice. Review freezes source, amount, remaining balance, account, quote and request identity; recording rechecks them atomically. Changed/retried requests and chronology are covered by existing tests. Receipt void uses the shared reason/confirmation surface.
- **Pay Bills:** vendor/currency/account/date and allocations remain one draft. Scope changes require explicit clearing of allocations. Record requires source reconciliation and matching bank currency; Payment History void uses the shared reason/date review. Used bank currency is fixed by domain rules.
- **AP bank setup / reconciliation:** specialized dialogs remain because configuring a bank currency and attesting source reconciliation are different operations. They use existing dedicated validation; no universal Save callback is introduced. Review evidence is not a balancing journal entry.
- **Invoice template:** shared close/reset guards retain pending input; Save and Print Preview have distinct meanings. PDF generation preserves document snapshots. File dialogs do not grant permission to overwrite an existing output silently.
- **Lookup, autocomplete and date popup:** a popup consumes its own selection/cancel interaction; parent close checks respect popup state. Service selection preserves the originating task line. Account/party fields retain stable row IDs.
- **File / Lists / Transactions / Setup / Reports / Window / Help:** module commands derive headings/create labels from one catalog. Backup/Verify remain explicit read/copy operations. Restore requires shared review, rechecks open drafts and uses the existing safety backup/validation path. Window recovery preserves creation-only geometry; it does not continuously reposition a dragged window.
- **App exit:** dirty business forms, product/template input, company/numbering/tax/GL drafts and active PDF/print work participate in the exit guard. Discard & Exit is explicit and receives no initial focus. Filters and read-only selections are not financial writes.

## Verification and limits

Final release evidence is recorded in [AUDIT.md](../AUDIT.md). Run `py -3.11 scripts/audit_ui.py` to regenerate `target/ui-audit-0200/source-inventory.json`. This text inventory indexes source functions and literal action sites; dynamically constructed actions, nested dialogs and domain semantics require the review matrix and tests, not a regex-based assertion of correctness.

Automated evidence includes all-page read-only rendering, common review keyboard/default-focus/blocked-submit behavior, narrow review footer bounds, navigation/draft retention, transactional Bill review, tax cash revalidation, journal preview/final period checks, and the existing invoice/receipt/AP/backup/upgrade/drag/scale tests. Actual egui meshes are rasterized for visual inspection; those are real widgets rendered offscreen, not photographs of physical desktop windows.

Still requiring separate work or acceptance:

1. Physical mixed-DPI monitors, Windows taskbar/snap, real drag-to-close, GPU rendering, screen-reader behavior and complete keyboard-only traversal. Automated geometry tests do not replace these.
2. The proposed comfort-density preference, global command palette, every proposed shortcut (notably universal Ctrl+S/Ctrl+N), fully virtualized typed registers everywhere and a complete accessibility audit are design targets, not shipped claims.
3. Existing untranslated long help/legacy validation messages remain in some older modules. Modified action labels are English; this release does not translate historical business data or claim a complete localization pass.
4. Receipts remain single-invoice. Arbitrary historical AP settlement import, advances, discounts, credit memos, cross-currency bank conversion, bank execution and online tax filing remain outside implemented scope. No UI advertises these as available.
5. Local SQLite/Windows-account protection does not provide independent user roles or tamper-proof storage. UI consistency and stronger transaction validation do not certify statutory tax compliance.
