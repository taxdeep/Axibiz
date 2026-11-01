# Vendors, Bills and Pay Bills

Updated in Axibiz Desktop Beta 0.19.0 / schema 19; supplier settlement records were introduced in schema 18.

## Vendor Center

Customer and Vendor use the same application navigation and workspace shell. Use the hamburger menu icon (**Hide main menu** / **Show main menu** on hover) to switch between icon/text and icon-only navigation. Both modes retain all business entries in the same order and keep Settings at the bottom. The choice is retained across launches for this company in its separate `.ui.json` preference file. The supplier list has its own Active/All/Inactive filter and name/contact search. Rows show **Recorded** gross bill amounts, with currencies kept separate. Drag the vertical divider to resize the list; drag the divider below the supplier information to adjust its height. These dividers never reposition the native window.

The upper region shows identity, contacts, an address and a bounded note preview. The remaining area is the striped transaction grid. Transactions, Contacts, Notes and Catalog retain the selected supplier. Transaction search and date filters are independent of supplier search; the Transactions workspace searches all vendors. Bill numbers open the native document editor. Export CSV follows the supplier/date/text scope and escapes spreadsheet formula prefixes.

**Recorded** is not the unpaid balance. **Open** is shown only when the source schedule and vendor/control-account ledger agree and source coverage is reviewed. Otherwise the screen shows **Needs reconciliation**. Unknown source dates, historical expenses without posted A/P sources and unassigned control-account journals prevent a false zero balance. Fully effective linked manual reversals cancel their original unassigned entries for later cutoffs.

Native preview images: [Vendor Center](screenshots/vendor-0180.png), [Supplier editor](screenshots/vendor-editor-0180.png), [Pay Bills](screenshots/pay-bills-0180.png).

## Supplier settings

New/Edit Vendor is a protected single-instance native window. Its identity header and footer remain available while fields scroll.

| Category | Supported fields and behavior |
| --- | --- |
| Address Info | Company/contact names, phone/mobile, main/CC email, website, preserved free-form contact details, Billed From and Shipped From, copy/same-address controls |
| Payment Settings | Supplier account reference and payment terms; profile save does not create an opening liability |
| Sales Tax Settings | Default expense tax code, country and business number |
| Account Settings | Up to three distinct active Expense account IDs, applied to new bill splits |
| Additional Info | Supplier classification and notes |

Name and currency are required. Existing IDs, names, free-form contact data and currencies are preserved. Changing a supplier preference does not convert old bills or change their saved tax/FX snapshots. Address-copy actions do not parse or overwrite the legacy contact field. Inactivation prevents new bills/catalog work while keeping existing liabilities payable and historical data available.

Clean forms close directly with native X/Escape. Changed supplier/Bill forms offer Save & Close, Discard and Return to editing; unsuccessful saves retain the draft. Reopening a business editor activates its current draft. Moving between monitors retains the creation-only window geometry behavior from 0.16.3.

## Enter Bill

Enter Bill uses the existing expense-document IDs and A/P posting path. It does not create an immediate bank expense. The header contains vendor/date/currency/reference/terms/due date/memo; the accounting grid supports expense accounts, tax/recovery and optional Task costs. Review recovery settings rather than assuming every tax is recoverable.

New supplier-started bills receive account, tax, currency and terms defaults. Blank account-prefill rows do not create zero-cost expenses; every started row still requires a valid account, positive amount and description. Terms are saved with the bill. Changing terms recalculates a suggested due date; changing the date follows the suggestion until the due date is manually overridden. Old bills retain their stored due dates and do not inherit new profile terms. Save & New starts a clean draft and reapplies supplier defaults only after successful saving.

Save before selecting **Pay Bill**. It opens Pay Bills with this supplier/currency/bill selected; an already-open payment draft is focused and retained. Posted A/P control accounts and original carrying amounts are taken from the actual posting source. Financial changes to a bill with any payment-allocation history are blocked, even after payment voids. Use a separate documented correction rather than rewriting earlier paid-period reports.

## Pay Bills

1. Open **New Transaction > Pay Bills**, the supplier **Pay Bills** action or a saved Bill's **Pay Bill** action.
2. Under **Bank Accounts**, choose a dedicated active Asset bank/cash account and explicitly set its currency. Control accounts cannot double as banks. Accounts with postings in another currency cannot be used; a used bank currency is fixed.
3. Check the supplier/currency and payment date. Use **Review A/P** for historical bills: compare the schedule and posted ledger with supplier statements, enter a review reference and confirm only if both source/base amounts reconcile and no sources are missing. Approval writes evidence, not a balancing journal. Existing manual settlements or missing sources require source/opening-item work; this release does not guess their allocation.
4. Select bills and adjust Payment amounts for partial settlement. Each payment covers one vendor/currency and one matching-currency bank. It may allocate across several bills. Changing supplier/currency confirms before clearing entered allocations. Total Payment is the sum of selected row amounts; no unapplied credit is created.
5. Record Payment reviews vendor, bank, date, amount, bill count, released A/P, bank base value and realized FX. Confirm once. This records the accounting event; it does not transmit a bank transfer.

Saved local date-specific FX is used first. Missing quotes are fetched in the background before a write transaction; quote and balance snapshots are revalidated on commit. A changed quote/balance keeps the draft and requires another review. Missing rates, wrong bank currencies, overpayment, duplicate bill selection, locked/future dates and stale source revisions prevent posting. Backdating before the latest supplier settlement/reversal is deliberately blocked.

Payment, allocations, VP number, journal/source links and audit records commit together. A random 128-bit request key makes identical retries return the original result; changed payloads with the same key fail. Independent connections cannot both consume the same remaining balance. Tax/Task costs are not recalculated by settlement.

## History, voids and cutoff reporting

Payment History lists immutable VP records. Click a payment number to view its journal. Void requires a reason and an explicit unlocked date on or after the latest settlement/reversal. It reverses the original source/base/FX amounts exactly and preserves the original payment and allocations. Allocated liabilities reopen from that reversal date; earlier cutoff reports keep the original payment effect. A repeat void cannot post twice.

Partial payments release original liability carrying values proportionally; the final payment consumes the exact remaining base amount. The bank uses the settlement-date quote; the difference posts to realized FX. Historical A/P account IDs remain in use even after role mapping changes. The cutoff reconciliation report includes per-supplier source-currency and base-carrying comparisons; unresolved source coverage is labelled for review.

## Compatibility and validation

Schema 18 is additive: old expense/document/line/journal identities, tax snapshots and source currencies remain. Startup uses the existing pre-upgrade backup mechanism. Existing source-backed bills enter the A/P schedule as unreviewed; old records without sources are never marked Paid or treated as verified unpaid items automatically. Reviewed new unpaid bills can acquire a validated new source revision; bills with allocation history stay fixed.

Tests cover atomic failures, concurrent attempts, partial/multiple allocations, FX gains/losses/final-cent rounding, unchanged idempotent retries, period locks, inactive suppliers, changed quotes, historical void cutoffs, source/ledger gaps and protected drafts. UI checks exercise compact layouts and stable native dragging, including Pay Bills. Physical multi-monitor/DPI acceptance remains a hardware check; software tests do not establish bank execution or tax eligibility.

## Shared workspace (0.19.0)

Customer and Vendor share toolbar placement, compact master rows, information/actions, detail tabs, transaction type/date/text filters, the transaction register and CSV encoding. Vendor events include recorded supplier payments and voids as well as bills. Empty results provide a clear filter reset or Enter Bill action. Read failures show an error rather than a false empty report. Profile settings use the same contact/address controls as Customer.

See [architecture and control review](ARCHITECTURE_REVIEW.md) for security changes, cache invalidation, migration policy and remaining scope.
