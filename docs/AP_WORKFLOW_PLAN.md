# Supplier bills, payments and allocation workflow

Implementation update: the core native workflow is implemented in 0.18.0 / schema 18. See [release behavior and operating instructions](VENDORS.md) for the actual supported scope. The HTML prototype remains a separate sample. Native implementation uses a random 128-bit idempotency key, rejects settlement backdating across later events, requires explicit bank currency configuration, and keeps any bill with allocation history financially fixed even after payment voids. Legacy source gaps/differences block settlement; arbitrary historical settlement imports and credit/advance/cross-currency workflows remain outside this release.


Design baseline: option 2 scope, 2026-10-30. The implementation update above and [release instructions](VENDORS.md) describe 0.18.0; the original proposal below also records later-scope requirements. The interactive prototype (internal reference artwork; not distributed) remains session-only sample data.

Previews: Pay Bills (internal reference artwork; not distributed), Payment History (internal reference artwork; not distributed), 900x650 window (internal reference artwork; not distributed).

## Scope and accounting boundary

The existing Expense document already posts a vendor-associated credit to Accounts Payable (`gl::expense`). Extend that document into a clearly named Bill workflow; retain document IDs, split costs, tax snapshots, source links and historical postings. Customer receipts in `billing.rs` are a different business flow and must not be reused as supplier payments.

Deliver three connected workspaces: Vendor Center for selecting suppliers and reviewing history; Bill for recording liabilities; Pay Bills for settling verified liabilities. Each payment belongs to one vendor and one document currency and may pay multiple bills, fully or partially. Initially the bank account must use that same currency. Cross-currency bank conversion, bulk payments across vendors, advances, discounts and credit applications require distinct workflows and must not be silently approximated by an amount adjustment.

The prototype shows Pay Bills, multiple partial allocations, a payment journal preview, payment history and dated voids. It does not execute bank transfers, persist records, fetch rates, implement credits or reproduce all native validation. Its clock is fixed at 2026-10-30; payment/void dates are intentionally limited to that date. Displayed quotes are labelled sample saved quotes. All seeded bills are explicitly treated as reconciled sample opening items; this is not a migration rule for existing company data.

## Window and layout contract

```text
Pay Bills                                 [system X]
Payment History | Preview Journal
----------------------------------------------------
Vendor           Currency         Bank account
Payment date     Method           Reference / Memo
----------------------------------------------------
Select | Bill | Date | Due | Original | Open | Payment
             independently scrolling allocation grid
----------------------------------------------------
Selected due   Applied   Remaining after payment
Source amount | A/P carrying value | Bank CAD | FX CAD
Validation / quote status        Record Payment
```

- Use a dedicated singleton window, independent of Customer Payment and the Bill editor. Reopen focuses its current draft; it must not change vendor, selections or amounts underneath the user. A Bill Pay action carries a vendor/currency/bill suggestion only when opening an unused payment editor.
- Default to the selected vendor and its bill currency. From all-vendor mode require an explicit supplier. List only verified, posted, non-void bills for that scope; inactive suppliers' existing liabilities remain payable.
- Select a row to fill its remaining amount; typing a smaller positive amount creates a partial allocation. Clearing a selection removes its amount. Changing vendor/currency with entered allocations requires confirmation before clearing that selection; Keep Selection is the default. Never silently apply a payment to another supplier, currency or bill.
- Original, Open and Payment have aligned, right-justified amounts. Show dates and supplier references. The footer stays visible at 900x650; only the grid and optional history scroll. Hide secondary context before shrinking amount columns.
- The initial payment amount equals the sum of explicit row allocations and is read-only. No unapplied remainder or automatic credit is created. Auto Apply, if later introduced, must be an explicit previewable action with deterministic due-date/ID order.
- Record Payment validates, shows vendor/bank/date/amount summary, and commits once. Keep the draft during background quote lookup and saving; block repeat submission and closing during commit, then report success or preserve inputs on failure. A reference is descriptive, not a deduplication key.
- Native X/Escape close pristine drafts directly. Dirty drafts offer Record & Close, Discard Changes, Keep Editing in one row; Keep Editing has initial focus. Record & Close uses the same validation and explicit payment review. Dismissal never records a payment.
- Preserve creation-only native geometry and monitor/DPI handling. Background balance refresh can flag changed rows; it cannot move the window, replace inputs or silently change entered amounts.

## Document and payment states

| Object | States and transitions |
| --- | --- |
| Bill draft | Editable, no document number or posting until successful save |
| Posted bill | Open / Partially Paid / Paid are derived from effective allocations, not editable flags |
| Allocated bill | Financial edits, vendor/currency changes and voids are blocked until dependent payments/credits are validly reversed; contact-only metadata must not rewrite posting snapshots |
| Payment draft | No bank or A/P effect; selected bills and entered amounts remain local |
| Recorded payment | Immutable numbered payment, immutable allocations, one source journal |
| Voided payment | Original preserved; linked opposite journal on an explicit permitted date and required reason; affected bills reopen from that date |
| Needs reconciliation | Legacy/manual source coverage is incomplete; exclude affected items from payable selections and show a reconciliation action |

Deleting a supplier does not remove history. Inactivation blocks new bills while retaining payment access to existing liabilities. Vendor credit/refund, write-off and bank fees must have explicit document types, accounts, dates and application links. Do not reduce a bill or write a negative payment to represent them.

## Posting, currency and rounding

Bill save retains the existing debit Expense/recoverable tax and credit A/P behavior. Payment save debits A/P at the original carrying value of the allocated liability and credits the selected bank at the settlement-date rate. The difference is realized FX. Tax is not recalculated at payment time. Persist both rates, original monetary values, carrying amounts and quote provenance; a later quote update must not change a recorded payment.

For example, a USD 1,000 bill recorded at 1.35 CAD/USD has CAD 1,350 A/P. Paying USD 400 at 1.37 releases CAD 540 A/P, credits Bank CAD 548 and debits realized FX CAD 8. The bill retains USD 600 / CAD 810 carrying value. Paying the remaining USD 600 at 1.34 releases CAD 810, credits Bank CAD 804 and credits realized FX CAD 6. The source-currency balance reaches zero and its carrying balance reaches zero independently of the bank rate.

Use integer minor units, checked arithmetic and fixed-point/rational rates in the native engine. Allocate original carrying value proportionally; the final settlement consumes the exact remaining base amount so rounding cannot leave a cent on a fully paid bill. Aggregate released carrying amounts by original control-account ID and source snapshot. Do not assume every historical payable used today's account-role mapping. Bank and FX lines must balance with those debits in base currency and comply with the engine's source/base-currency representation.

Void reverses the exact recorded source and base amounts, including realized FX; it never fetches a new quote or recomputes the original settlement at today's rate. Do not replay or rewrite later valid payments. Credit/opening-balance applications use the same historical carrying discipline when those later workflows are built.

Use an applicable saved date-specific quote first. Fetch missing historical data automatically, save provenance and validate the actual quote/effective date under existing weekend/non-business-day policy. If no permissible quote exists, retain the draft and prevent posting; do not fall back to an unrelated current quote or rate 1. The native release must validate cash-account currency and availability, task locks, source integrity and period locks on every entry point.

## Proposed storage and atomic commands

Names below are design names, not an applied migration:

| Record | Required information |
| --- | --- |
| A/P document link | Existing expense/document ID, vendor, currency, original gross, original base carrying value, original control account(s), source journal/snapshot, due date, posting/void dates, reconciliation evidence |
| Vendor payment | ID/number, vendor, currency, date, bank account ID, method, reference/memo, source amount, bank base amount, rate/provenance, immutable request UUID, source journal, created/audit metadata |
| Allocation | Payment ID, bill/source revision ID, source amount, released base amount, original A/P account ID, deterministic rounding snapshot |
| Payment reversal | Payment ID unique, reversal date/reason, linked reversal journal, audit metadata; original allocation remains immutable |
| Reconciliation evidence | Cutover date, vendor/currency/control account, linked sources, differences, reviewer confirmation and historical coverage state |

Introduce tables additively with foreign keys, CHECK constraints, unique request UUIDs, one reversal per payment and audit triggers. Do not overload the existing customer `payments` table or infer supplier-payment records from its numbering sequence.

Fetch any missing quote in the background before acquiring the write transaction; freeze a proposed quote and draft revision for review. Never hold a database write lock during a network request. `record_vendor_payment` then runs in one write transaction: resolve/reuse an existing request UUID; validate dates and locks; reload original documents and latest effective allocations; verify vendor/currency/source revisions and account status; validate the complete event timeline; revalidate the reviewed quote/provenance and draft revision; compute checked source/base amounts; allocate a number; insert payment, allocations, journal/source links and audit records; commit all or none. A UUID retry returns the original committed result only for an identical payload. A mismatched payload fails. Two payment windows/processes must not over-allocate the same liability.

Balance validation must hold for every effective cutoff affected by backdated activity, not only today. A later void can restore capacity today without allowing a second earlier payment during an interval where the liability was fully settled. Either reject such backdating or validate all subsequent bill/allocation/reversal event boundaries. Never change older reports just by overwriting a current Paid flag. Voids similarly revalidate the subsequent timeline and reversal date against the payment date and period locks.

## Historical data and reconciliation gate

1. Inventory every existing expense posting and reversal with its vendor, source currency and original control account. Retain tax/source snapshots. Do not duplicate the existing A/P credit.
2. Reconcile each supplier/currency/control-account group to the posted GL at an explicit cutoff. List manual A/P journals, unknown dates, source revisions, legacy adjustments and unsupported source coverage as differences.
3. Link actual historical settlements using evidence, or import an explicitly reviewed outstanding-items schedule. Opening liabilities require a dated, balanced import with documented offsets and no duplicate historical posting. Never automatically create a settlement or mark old bills unpaid because no allocation record exists.
4. Mark coverage verified only for reconciled items and periods. The current Recorded Expenses metric stays available independently. Until cutover completes, show Needs reconciliation rather than an authoritative Open Balance; do not offer unverified items for payment.
5. Reconcile source-currency open items and base-currency carrying value separately. FX revaluation, if supported later, needs reversible adjustment sources distinct from settlement FX.

For cutoff D, a payment is effective only when payment.date <= D and no reversal is effective by D. Apply the same rule to bill reversals and credits. Never use only today's void flag for an older report. Retain and expose unapplied manual entries separately until reviewed; a vendor-level GL balance alone is insufficient to pay a specific bill.

## Delivery and acceptance

Implement in this order: (1) flat Vendor Center/editor and account defaults; (2) Bill layout and genuine dirty-state close; (3) A/P source inventory, cutoff reports and reconciliation gate; (4) immutable payment/allocation engine, idempotent posting and dated voids; (5) native Pay Bills singleton and register drilldown. Enable Open Balance/Pay Bills only after the engine and reconciliation gate pass. Keep credit applications, advances, bank fees, multi-vendor batch issuance and cross-currency bank settlement behind separate future designs.

Required native acceptance cases include:

- Full, partial and multiple-bill payment; repeated attempts; concurrent allocation; amount greater than remaining; wrong vendor/currency/bank; inactive supplier with valid outstanding bills.
- Correct original-control-account release after role changes; exact full-settlement rounding; realized FX gain/loss; missing and stale quotes; immutable source snapshots after rate changes.
- Bill revision/void after allocation; payment void and repeated void; locked dates; rollback on each intermediate failure; mismatched UUID retries; no duplicate numbering/source records.
- Historical cutoff before payment, between payment and reversal, and after reversal; backdated allocations across subsequent events; legacy/manual journal differences cannot bypass the reconciliation gate.
- Native X and dirty review, failed record preserving input, repeated opening preserving draft, visible actions at 900x650, high DPI and moving between negative-coordinate monitors without jitter.

Prototype checks are interaction evidence only. They do not substitute for these native accounting and migration tests.

## Prototype validation (original design iteration)

Headless Microsoft Edge interaction checks passed for overpayment rejection, supplier/currency scope, matching-bank and date validation, partial settlement, multiple-bill settlement, original carrying value and realized FX journal, repeated Record protection, payment history, reason-required voids and restored outstanding amounts. They also cover preventing financial edits to allocated bills, preserving an existing payment draft when reopened, confirmation before clearing a bill selection, Record & Close review/cancellation, explicit discard and visible actions at 900x650.

The existing vendor editor/Bill regression checks also passed: account-default validation/prefill, clean and dirty close, retained drafts, historical due dates, selected-vendor versus all-vendor creation, successful preview saves and the compact editor footer. Actual browser screenshots were inspected. No Rust source, schema migration, distribution executable or company database was modified for this design iteration.
