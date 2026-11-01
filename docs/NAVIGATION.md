# Desktop Navigation

Introduced in 0.22.0. Current release 0.25.0 uses company schema 21, including optional company users.

- Expanded pane: 220 logical pixels, icons and 13-point labels.
- Compact pane: 56 logical pixels, icons only, full route names on hover and in semantic button labels.
- Both modes share one registry and order: Tasks, Invoices, Customers, Vendors, Products & Services, Bills, Journal Entries, General Ledger, Chart of Accounts and Sales Tax. Settings is fixed at the bottom.
- The fixed Settings route opens software preferences (backup directory, daily startup backup and time display). Company information is a separate **Setup > Company Settings** route. Both drafts survive navigation and participate in exit/restore checks.
- **Setup > Users...** manages company-local accounts and optional password sign-in. **File > Lock / Switch User** hides the workspace after drafts are saved/discarded. The navigation pane is not rendered while a protected company is signed out.
- Rows use 40-pixel targets. Selected rows have a subtle background, green left indicator and selected accessibility state. Keyboard focus has a visible outline and supports normal button activation.
- The menu toggle and all icon centers retain their horizontal and vertical anchors between modes. Longer content scrolls independently of the fixed Settings entry.
- Switching modes changes presentation only. Existing `navigate` logic retains module searches/selections; native business-window singleton and dirty-input protections remain authoritative.

`src/gui/navigation.rs` owns the registry, layout and preference handling; `src/gui/icons.rs` owns the shared vector shapes. Distinct icon silhouettes separate Invoice/Bill, Customer/Vendor and Journal/Ledger/Accounts.

The current company's expanded/collapsed preference is written only when the user toggles the menu, in a separate sidecar: `company.db` → `company.ui.json`. It contains no business data. Missing, malformed or unreadable metadata falls back to expanded navigation. A save failure retains the user's current in-memory choice and reports that the preference could not be saved. Financial database backup/restore does not replace this UI preference; native geometry remains in the existing `.windows.json` file.

Verification includes actual pointer activation, identical route coverage/targets in both modes, icon/toggle anchoring, keyboard invocation, search/selection/open-draft retention, restart/restore behavior and corrupt preference fallback. Actual widget renders cover expanded/narrow and icon-only layouts. Physical mixed-DPI, screen-reader and complete keyboard-only acceptance remain separate checks.
