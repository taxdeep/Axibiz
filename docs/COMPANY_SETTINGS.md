# Company Settings

Version 0.23.0 introduced the Company Information grouping within the desktop design language. Since 0.24.0, open **Setup > Company Settings**. The navigation pane's Settings entry opens separate [software preferences](SOFTWARE_SETTINGS.md).

Company account/password management is a separate **Setup > Users...** entry; see [Company Users](COMPANY_USERS.md). It follows the company database rather than this computer's software preferences.

| Section | Fields and behavior |
| --- | --- |
| Contact Information | Required company/display name, contact address, country, phone, fax, email, website and retained freeform contact details. Website accepts an explicit HTTP(S) address. |
| Legal Information | Optional legal name, legal address, city, province/state, postal code and country. Selecting the contact address retains the separate legal address so clearing the option restores it. |
| Company Identification | Optional Canadian BN, GST/HST account, QST reference and retained legacy tax reference. BN/account consistency is checked when both are supplied. These do not enable tax collection or alter agency registrations. |
| Reporting Preferences | Optional fiscal-year starting month, tax-year starting month and income-tax-form reference (T2, T1, T5013, Other). These are stored reference preferences in this release, not report filters or tax-filing functionality. |
| Company Preferences | Existing default transaction currency, company ID and guarded legacy currency-label maintenance. Default currency applies to new tasks/prices; it does not convert historical values or change the GL base currency. Software backup/time preferences are managed separately. |

Contact fields use two columns when space permits. Legal information and identification sit beside each other on wide workspaces and stack on narrower ones. Only the form body scrolls; Save, Save & Close and Revert Changes remain outside that scroll area. Revert requires an explicit discard choice. Navigation retains the draft; exit and restore use the existing unsaved-input protection. No redundant Close, sign-in, attachment or nonfunctional Help action is added.

## Persistence and history

- Schema 20 adds a singleton `company_profile` JSON-object table. Existing `settings` columns remain intact. Upgrade leaves new optional values unspecified; it does not infer legal identity, registration, country or year starts from old freeform text.
- Saving validates and normalizes the draft, acquires the existing company-name instance guard, then commits the original settings and typed profile in one SQLite transaction. Both tables use the existing audit triggers. Failure rolls back both writes and their audit entries; the UI retains the draft and reports the error.
- New invoices freeze the structured profile with the existing company snapshot. Phone/email/website augment its printable contact text; GST/HST and QST augment the retained tax reference. Legal/reporting metadata is retained in the snapshot for future document layouts; it does not silently replace the company/display name or contact address on current templates.
- Issued invoice snapshots are never rewritten. Legacy CLI settings edits load and retain the structured profile. Backup/restore uses the existing staged migration and validation workflow.

## Identifier references

The Canadian BN has nine digits; a GST/HST program account adds RT and four digits. This is format checking, not online registration verification. See [CRA business numbers and program accounts](https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/business-registration/business-number-program-account.html) and [CRA registration examples](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/2-1/required-registration.html).

QST is retained as the registration reference issued by the authority. It is not forced into a ten-digit-only field, which would reject account suffixes and special administered accounts. See [Revenu Québec QST validation information](https://www.revenuquebec.ca/en/online-services/tools/application-programming-interface-api-used-to-validate-a-qst-registration-number/).

## Verification

In-memory regressions cover normalized persistence, invalid optional identifiers/contact fields, transaction/audit rollback, schema-19 migration, unchanged issued invoices, and draft retention across navigation and failed saves. Real widget captures at 1440×900 and 900×650 assert the save action remains within the viewport without writing business records. Release and isolated-copy upgrade evidence is recorded in `AUDIT.md`.
