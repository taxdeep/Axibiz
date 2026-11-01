# Vendor workspace redesign

Implementation update: the core native workflow is implemented in 0.18.0 / schema 18. See [release behavior and operating instructions](VENDORS.md) for the actual supported scope. The HTML prototype remains a separate sample. Native implementation uses a random 128-bit idempotency key, rejects settlement backdating across later events, requires explicit bank currency configuration, and keeps any bill with allocation history financially fixed even after payment voids. Legacy source gaps/differences block settlement; arbitrary historical settlement imports and credit/advance/cross-currency workflows remain outside this release.


Design baseline: 2026-10-30, replacing the card-based 0.17.0 direction. Core implementation is now in 0.18.0; see the update above and [actual release behavior](VENDORS.md). The original proposal below is retained for design traceability.

Option 2 expands this proposal to the full supplier A/P workflow. See [Supplier bills, payments and allocations](AP_WORKFLOW_PLAN.md) for posting, partial payments, dated voids, reconciliation and implementation gates. The prototype now includes a working sample Pay Bills screen; this does not enable native payments.

Open the interactive layout prototype (internal reference artwork; not distributed). The prototype uses example data and keeps changes in the preview session.

Payment previews: Pay Bills (internal reference artwork; not distributed), History (internal reference artwork; not distributed), Small window (internal reference artwork; not distributed).

Browser previews: Vendor Center (internal reference artwork; not distributed), Vendor editor (internal reference artwork; not distributed), Bill entry (internal reference artwork; not distributed).

## Reading the reference correctly

The eight reference screens describe three different workspaces and a shared close contract:

| Reference | Purpose | Layout implication |
| --- | --- | --- |
| Vendor Center | Select a supplier, inspect identity and process transactions without losing the selection | Persistent master list at left; compact identity/context region above a large transaction work area |
| Address Info | Maintain identity, contacts and source addresses | Persistent identity header; vertical category navigation; aligned contact fields and two address blocks |
| Payment Settings | Store the supplier account reference and payment defaults | A sparse settings page is appropriate; it does not need unrelated financial summary cards |
| Sales Tax Settings | Configure defaults and supplier classification | Separate tax defaults from filing eligibility and tax-agency integration |
| Account Settings | Reduce repetitive expense-account entry | Validated account defaults must actually prefill new document lines |
| Additional Info | Classify a vendor and retain extensible metadata | Keep classification and notes out of the primary contact form |
| Bill | Record a payable document with accounting splits | Fixed command strip, compact document header, dominant editable grid and a subordinate vendor context pane |
| Recording Transaction | Resolve a dirty document before closing | Save, explicit discard and return to editing are separate actions |

The previous design reused too much of the customer-card arrangement. It gave identity, actions and totals a large card while treating transactions as another tab. The reference gives most of the working area to transactions and keeps the selected supplier and document context continuously visible.

## Verified accounting boundary

The current expense posting path already credits Accounts Payable: `src/gl.rs`, `expense`, creates a vendor-associated payable line. It does not immediately credit Bank. The vendor form, expense documents, source links, posted journals, reversals, tax snapshots, period locks and date-aware FX already exist.

The missing layer is supplier payment recording and allocation to individual documents, including partial payments, credits, voids, historical cutoffs and foreign-exchange differences. Summing recorded expenses does not establish the amount still owed. A manual A/P journal can also affect a supplier's ledger without allocating a payment to a document.

The initial redesign can use the current posting path. It must identify it clearly as a bill/payable document. It must not relabel historical documents as immediate bank expenses, mark them Paid based on absence of an allocation record, or present expense totals as Open Balance.

## 1. Vendor Center

```text
Application menu
New Vendor | New Transaction v | Print | Export
---------------------------------------------------------------
Vendor list       | Vendor Information                | Note
Vendors/Transactions  Company / Contact / Billed From  | Quick links
Active filter     | Currency / Terms / Status         | Context
Supplier search   |----------------------------------+-----------
Name / Recorded   | Transactions | Contacts | Notes | Catalog
                  | Show / Date / Currency / Search transactions
                  |----------------------------------------------
                  | Type | Number | Date | Account | ... | Amount
                  |
                  | Large, independently scrolling transaction grid
                  |
                  |----------------------------------------------
List count        | Selected transaction actions        Row count
```

### Spatial hierarchy

- Replace the large rounded outer card and explanatory page subtitle with a flat desktop workspace and a 38–42 point command strip.
- Keep application navigation collapsible. On this dense workspace, its compact rail should be about 64 points. A full 220-point application menu plus another supplier sidebar consumes too much transaction space.
- Use a resizable supplier list, default 230 points and bounded to 200–300. Search and Active/All/Inactive filtering belong to this list, not to an unrelated page-wide search row.
- Use ordinary 28–30 point supplier rows with aligned name and amount columns. Keep the selected row distinct, allow keyboard navigation and show the complete truncated value on hover.
- Call the current amount column **Recorded**, show its currency and never combine currencies into one total. A true Balance column is a later, reconciled A/P feature.
- Limit the upper information region to approximately 150–180 points at normal sizes. Show identity, company/contact, billed-from address, terms/currency/status and an edit action. Do not place a second large supplier-name card here.
- Put note preview and relevant links in a 210–240 point context pane at the upper right. At smaller widths, move it into an explicit Info drawer instead of shrinking the transaction grid.
- Place the detail tabs immediately above the transaction filter row. Default to Transactions. Keep the supplier selection when changing tabs, opening a document or returning from its editor.
- Give the transaction work area the remaining height. Only the list and grid scroll independently; the command strip and supplier context remain available.

### Interaction and command ownership

| Control | Behavior |
| --- | --- |
| New Vendor | Open/focus the singleton supplier editor |
| New Transaction | Start a bill using the selected active supplier; add payment/credit choices only when their workflows exist |
| Edit supplier | Open/focus the selected supplier's editor; an existing unrelated draft is retained |
| Double-click transaction / number | Open its document in the singleton document window |
| Vendors / Transactions | Switch between supplier-scoped work and the all-supplier transaction register; retain each mode's filters separately |
| Supplier search | Filter only the supplier list |
| Transaction search | Filter only the visible transaction register |
| Date / Currency | Filter displayed rows; do not change the supplier's stored preference |
| Print / Export | Operate on the visible report/filter scope, with an explicit report title and currency columns |
| QuickReport | A read-only supplier report with a date/currency scope; requires a real report query, not a decorative link |

Current transaction columns: Type, Number, Date, Expense account(s), Reference/Memo, Currency and Gross Amount. Multiple accounting splits display as Split with a drilldown. Once payment allocation exists, add original amount, applied amount and remaining amount with independently verified meanings.

Empty-state text belongs inside the grid body, just below its header. It should explain the selected supplier/type/date filters. Avoid placing the explanation at the bottom of an otherwise empty, tall striped table.

## 2. New / Edit Vendor

```text
Native title bar                                      system X
Vendor Name * [.................................................]
Currency *    [CAD v]            Status [Active]
----------------------------------------------------------------
Address Info       | Company and contact identity
Payment Settings   | Phone / Mobile          Email / Website
Sales Tax Settings | Contact details         Other details
Account Settings   |
Additional Info    | Billed From             Shipped From
                   | [address........]       [address........]
                   |                   Copy billed address >
----------------------------------------------------------------
Inactive [ ]                       Save      Save & Close
```

Use a fixed identity header, a 165–185 point category column, one bounded content area and a fixed action footer. Switching categories never saves or clears the draft. The footer remains visible at the native minimum size. At narrow sizes, paired fields stack within the content area; the category navigation must remain usable without moving Save off screen.

### Address Info

- Vendor name and currency stay in the fixed header.
- Company name and contact name are separate. The current free-form contact name remains valid; first/middle/last name and job title are optional extensions, not new requirements.
- Pair phone/mobile with email/website. Add work phone and CC email only where they are useful and persist through the profile save path.
- Separate **Billed From** and **Shipped From**. The current `profile.address` migrates as the existing billed-from address. Do not overwrite or silently reinterpret it. A separate remit-to address, if required for payments, belongs to the payment details.
- Copy billed address is an explicit action. A Same as billed setting may keep it linked until the user turns that setting off.
- Preserve original unstructured contact details in an Other details field; do not guess their phone/email components.

### Payment Settings

Use account reference and payment terms. Show a cheque print name only with an implemented cheque workflow. Credit limit and billing-rate levels must have defined behavior before appearing as editable financial controls. Do not expose an Opening Balance input on a normal profile save.

### Sales Tax Settings

Use default purchase tax code, country and business number. T4A/T5018 eligibility must be treated as classification metadata if introduced; checking a flag must not imply that filing, reporting or eligibility validation is implemented. Tax-agency status needs a real link to tax agencies and their posting rules, rather than a standalone checkbox that does nothing.

### Account Settings

This is a functional category missing from the first redesign. Configure up to three default expense-account IDs, selected only from active Expense accounts. Reject duplicates. Labels can reflect account code/name, but persistence uses immutable IDs.

A new bill uses these account defaults. Automatic blank template lines do not create amounts or block saving an otherwise complete document; a line the user starts editing must be validated. Selecting another supplier must not overwrite existing entered splits. If a configured account is later deactivated, retain it visibly in settings and require replacement before using it for new work.

### Additional Info

Use supplier type and notes. Custom fields need a defined field schema, field-type validation and audited persistence before adding a Define Fields action. Notes remain directly accessible from Vendor Center.

### Opening balances

Opening A/P requires a dedicated, auditable setup/import workflow with an as-of date, currency and offset account, plus a strategy for document-level allocation. It must not be a text field that posts again on every Edit Vendor save. Place it under a separate setup action only after that workflow is implemented.

## 3. Bill / Expense document entry

```text
Native title bar                                                X
New | Save v | Print | View Journal             (real actions only)
----------------------------------------------------------------
A/P Account [configured control account]       Document status
Supplier [..................................]  Date [..........]
Billed From [address.........................] Reference [.....]
Terms [....] Currency [....]                   Due date [.......]
Memo [........................................................]
-------------------------------------------------+-------------
Expenses                                         | Vendor
Account | Tax | Amount | Memo | Task               | Summary
                                                  | Recent bills
Large editable striped accounting-split grid      | Notes
                                                  |
--------------------------------------------------+-------------
FX / tax context                    Subtotal / Tax / Total
                                     Save & Close | Save & New
```

- The upper command area contains operations on the whole document. Entry fields belong to the document header. Avoid a mixture of page navigation, profile actions and document-save actions in the same row.
- A/P account reflects the actual posting rule. If it is configurable per document, it must be persisted and included in revalidation; otherwise show the configured account read-only.
- Use vendor, date/reference/terms/due date, source address, currency and memo as a compact header. Keep the editable accounting grid dominant.
- The right context pane is read-only and subordinate. It can show current vendor contact/notes and recent documents. Vendor currency changes must not replace the currency of an existing bill.
- The Expenses grid maps to the existing expense splits and Task cost associations. An Items grid must represent genuine purchase-item behavior before it is added; the existing service sales catalog alone does not provide purchase inventory or billable-item accounting.
- Total is computed and read-only. Do not add an independently editable Amount Due that can disagree with the splits.
- FX remains date-aware, local-cache-first and automatic when missing. A manual refresh button is not part of this layout.
- Save & New clears the completed document's identity/reference/memo/amounts and creates a new draft only after successful saving. Any defaults retained for the new draft must be explicit and predictable.

An immediate-paid expense, if added later, must be a separate document/payment behavior with a selected bank account. It cannot be created by renaming the current A/P posting screen.

## 4. Closing and multi-window behavior

| State | Native X / Escape |
| --- | --- |
| Read-only document or clean existing profile | Close immediately |
| Untouched new draft | Close without a redundant discard prompt |
| Dirty profile or document | Show Save & Close, Discard Changes and Keep Editing horizontally |
| Save fails | Keep the draft and window; show validation and focus the affected field |
| Save succeeds | Release the singleton window and refresh affected views |

The close dialog must reflect actual dirty state, not simply the presence of an open form. Keep Editing is the safe initial focus. Saving a bill invokes the same validation and atomic posting path as its normal Save action, once. Discard never posts a document. Repeated X events must not recreate the draft or move the window.

Retain one Vendor editor and one document editor at a time. Reopening activates the existing draft. A later Pay Bills screen should have its own singleton slot, with payment/application creation transactional and idempotent.

Native geometry is restored only at window creation. Keep the 0.16.3 fix: live drag/resize/DPI observations do not become move/resize commands. Scroll/filter/detail changes do not change a native window's position or size.

## Sizing, accessibility and acceptance

- Use 13-point body/input/button text, 11-point labels/table headers and a 20-point workspace heading. Financial amounts use one body size; emphasis is by weight and alignment.
- Inputs and ordinary actions are 28–32 points high. Numeric columns are right aligned and retain at least 100–120 points.
- At 1440x900, target about 150–180 points for supplier information and at least 55–65% of the workspace height for transactions.
- At 1200x800, collapse the application navigation and hide the context pane behind Info; retain the supplier list where it fits.
- At 900x650, use the supplier selector or narrow master pane, a two-row command/filter arrangement and a bounded grid. Primary save/close actions stay visible in editor windows.
- Test 100%/150%/200% scale, keyboard-only supplier selection and form navigation, long names/addresses, empty and many-row registers, inactive suppliers, multiple currencies and both negative-coordinate and disconnected monitors.
- Financial acceptance includes posted A/P reconciliation, partial/voided allocations, credits, foreign currencies and manual-journal differences before calling any value Open Balance.

## Implementation sequence

1. Rebuild the Vendor Center hierarchy: compact command strip, list-scoped search, resizable master/detail division, upper identity/context region and transaction-dominant grid. Keep existing records and actions.
2. Rebuild the profile editor around fixed identity/vertical categories/fixed footer. Add only fields with compatible storage and functional behavior; implement account defaults with validation.
3. Reorganize the current payable-document form and implement dirty-state close handling through existing transactional saves. Preserve singleton, FX, locks and stable window geometry.
4. Build supplier payments/allocations, reconcile document remaining amounts to the ledger, and then enable Open Balance / Pay Bills. Add reports/export controls only with their actual report/export paths.

The planning artifacts make stages 1–4 reviewable, including a session-only Pay Bills simulation. Native implementation and accounting acceptance remain separate; no live company data is changed.
