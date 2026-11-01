# Customer Center

The Customers page has two views:

- **Customers**: select a customer on the left to see information, open balances by currency, transactions, contacts, notes and linked tasks.
- **Transactions**: browse invoices and payments across customers. Filter by transaction type, open invoices or date period. Invoice numbers open the related invoice; Receive opens the receipt window for an outstanding invoice. Voided receipts are explicitly identified.

New Invoice and New Task use the selected active customer's currency. Existing business windows are activated rather than replaced. Opening an invoice from Customer Center preserves the customer workspace and search.

The toolbar, navigation, status/search controls and transaction register are shared with Vendors. Profile search and transaction search are independent. On wider windows, the customer list has a stable width and the right pane stacks customer information, actions, balances and tabs above the transaction table. Smaller windows replace the list with a customer selector. Wide tables scroll horizontally inside the pane; long contact details and notes scroll vertically without moving the header or tabs off screen. Summary fields show the full value on hover.

Customer rows align names to the left and currency balances to the right. The overview highlights New Invoice and separates the open balance from secondary actions. Contacts show contact information and billing/shipping addresses side by side when space permits, or stacked on smaller windows. Missing essential values read Not provided; an empty profile offers Add Contact Details. Empty Notes and Tasks sections offer Add Notes and New Task. Contact/note actions reuse the protected customer editor and do not replace an already open draft. Inactive customers cannot start new tasks or invoices.

## New and Edit Customer

Customer name and currency are required. Other fields are optional:

| Tab | Fields |
| --- | --- |
| Address Info | Company, contact name, phone, mobile, email, CC email, website, legacy contact details, billing and shipping addresses |
| Payment Settings | Account reference, payment terms, preferred receipt method |
| Sales Tax Settings | Default tax code, country, business number |
| Additional Info | Customer type and notes |

The editor is a single-instance native window. Save keeps it open; Save & Close closes only after a successful save. The title-bar X and Escape use the shared unsaved-input protection. Validation errors keep the draft. The Active customer flag controls availability for new work; inactive customers remain available for history and collection of existing receivables.

Payment terms and billing details are captured in new invoice snapshots. Changing the customer later does not rewrite existing invoices. Receipt entry initially uses the preferred payment method. Default customer tax codes apply through the existing sales-tax assignment rules; item overrides remain effective.

Opening balances require the existing accounting and supporting-invoice workflow. Profile editing does not create or modify financial balances. Job information is represented by linked Tasks. The editor does not store credit-card information or offer unsupported estimates, credit memos, discounts or price-level controls.

## Data compatibility

Schema version 16 adds `customers.active` (existing customers default to active) and a separate `customer_profiles` table. Customer IDs, names, existing free-form contact data, currencies and historical financial records are retained. Profile/base-customer/tax-default saves share one transaction and audit trail. Balances are grouped by invoice currency; they are never added across currencies.

## Receipt review (0.19.0)

Receive opens a protected singleton for one invoice. Enter date, amount, method and reference, then choose **Review Receipt**. **Confirm & Record** commits the receipt and its posting; opening the review does not save. Stale balance/quote/account/source checks run again inside the transaction. Identical request retries return the original receipt, including after its void, rather than recording again. Review failure retains input.

Receipt dates cannot precede the invoice/posting, a later receipt or reversal, or fall in a future/locked period. Historical records remain unchanged. The configured GL **bank** role is the deposit account. An unregistered deposit role accepts only GL base currency; foreign receipts require an explicitly registered matching bank currency. Configure bank currency in **Pay Bills > Bank Accounts** and the deposit role in **GL Setup**. Receivable/tax control accounts and mixed-currency bank postings are rejected. This records accounting only; it does not initiate a transfer.
