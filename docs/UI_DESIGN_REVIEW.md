# Designer Review of the Current UI

Navigation update in 0.22.0: the expanded icon/text pane and icon-only rail now share all routes, order and icon anchors; Settings is fixed at the bottom and the collapse preference survives restart. The selected row uses a subtle background and green indicator. See [Navigation](NAVIGATION.md). Physical/accessibility acceptance remains separately scoped.

Date: 2026-10-31. Baseline reviewed: 0.20.0 UI source and the `target/native-0200-preview` widget captures. The observations below describe that baseline and the proposed refinements.

Implementation update: 0.21.0 delivers the shared vector icon layer, row-sized icon targets, separated party tabs/commands, embedded search/clear icons, active filter counts, aligned numeric headers, shared workflow footers, one Bill review action with an after-recording selector, optional advanced Bill columns, compact receipt allocation/summary and protected payment/allocation reset. Confirmation labels now reflect the operation. Source domain validation, singleton windows and unsaved-input checks are retained. New layout/interaction regressions and actual widget captures accompany this update.

Physical mixed-DPI monitors, full keyboard/accessibility acceptance and additional user layout preferences remain separate acceptance work. The original SVG is a proposal, not a claim that every pictured detail is implemented. Future template work is described separately in [Document Template System Plan](DOCUMENT_TEMPLATE_PLAN.md).

The shared desktop shell, compact tables, guarded business windows and reviewed financial writes provide a useful foundation. The next improvement should make actions predictable, reduce competing choices and explain state clearly. More icons alone will not resolve these interaction problems.

See the proposed layout and icon language (internal reference artwork; not distributed). Sample names and amounts are illustrative. The proposed outcome selector consolidates the existing Bill Stay / New / Close continuations; it does not introduce a new posting capability.

## Usability findings

| Observation | Effect on users | Recommended change |
| --- | --- | --- |
| Bill footer exposes Review Bill, Review & New and Review & Close together. | Users choose a continuation before they understand what is being recorded. The wording suggests closing might happen before review. | Keep one primary Review Bill action. Move Stay / New / Close into a clearly labelled After recording selector or secondary action menu. Every path uses the same review and domain transaction. |
| Bill and receipt action bars use different sides of the window. | Muscle memory does not transfer between related forms. | Place primary workflow actions consistently at the bottom right. Place destructive reset at the opposite end. Keep object actions in the header and line actions in the line. |
| A disabled submit button often carries no visible explanation. | Users must guess whether customer, amount, tax, FX, reconciliation or bank configuration is missing. | Show a short reason near the action and beside the relevant field. Example: Enter a positive amount on line 1. Show a fix action when possible, not just an error. |
| Receipt currently supports one invoice but uses a large allocation table with decorative empty rows. | The page looks like a multi-invoice selection workflow and spends space on non-data. | Preserve familiar columns but fit the grid to actual rows with a modest minimum. Make the invoice, amount due, applied amount and resulting balance the strongest elements. Reserve batch allocation for an implemented batch workflow. |
| Clear is used for unrelated scopes: search, selection, allocations and an entire document. | Users cannot predict how much input will disappear. | Use Clear search, Clear selection and Reset form as distinct actions. A document reset needs an explicit discard guard or reliable undo. It never voids saved history. |
| Root and native window actions are not equally discoverable when several windows are open. | Reopening a singleton can feel like nothing happened. | Retain focus-existing behavior and show a brief Open on another window notice, a discoverable Open Windows list and existing monitor recovery. Recovery must remain explicit and must not reposition a dragged window every frame. |

## Layout and professional presentation

- Keep the current body/heading hierarchy. Reduce redundant page descriptions and excessively tall branding before reducing readable text. Table headings may be smaller; financial amounts and input values should remain body size.
- Align numeric headings with right-aligned numeric values. Use consistent precision and tabular digits where supported. Keep source currency visible beside totals; show converted/base amounts as a secondary labelled line.
- Give totals a stable position near the grid and above the footer. Current Bill capture has a large gap between grid, total and actions. A resizable data region with an adjacent summary uses that space better.
- In Customer/Vendor centers, separate view tabs from commands visually. New Customer, New Transaction, Customers and Transactions currently share one crowded toolbar level. The selected party name, status and open balance should form one clear overview.
- Use subtle separators and row hover/selection. Never communicate paid, overdue, draft or voided state only with color. Accompany the state with text; display zero, unknown and not configured as distinct values.
- Reveal advanced columns progressively: Task, tax-included and recovery settings can be an explicit advanced-columns choice while required controls and exceptions remain visible. Preserve values when columns are hidden. Avoid a new accounting default merely to simplify the layout.
- Long headers and names need truncation with full-text access, resizable columns where practical, and horizontal scrolling for genuinely wide documents. Test logical window sizes and DPI independently.

## Icon policy

At the baseline, the trash target was 32×32 while table rows were 28 high. The shared 0.21.0 icon layer now uses a 16-unit glyph within a 28×28 row target. Compact labelled commands use the existing control height; any later larger toolbar variant must remain separate from row actions. A larger icon target must never overlap the next row.

| Action | Icon | Presentation | Important meaning |
| --- | --- | --- | --- |
| Delete an unsaved line | Trash can | Icon only in the action column, tooltip Delete line | Remove this draft line; never a payment reversal. |
| Edit | Pencil | Icon only per row; icon + Edit Customer/Vendor in a profile header | Enter an editor; do not turn all master fields into accidental inline edits. |
| Copy line | Two sheets | Icon only per row, tooltip Duplicate line | Copy pending input, not issue another financial document. |
| Search | Magnifier | In the search field | Not a competing standalone Search button when search already runs as users type. |
| Clear search | Small X | Inside the search field | Clear only the query; never use the trash can for search clearing. |
| Filter | Funnel | Icon + Filters when a panel opens; indicate active filter count | Do not hide the fact that a list is filtered. |
| Print / PDF / Export | Printer / document / outward arrow | Icon + short label in the top command area | Print, Save PDF and Export CSV remain distinct. |
| More actions | Ellipsis | Icon only with a labelled menu | Place low-frequency commands here; avoid hiding the primary action. |
| Receive Payment / Pay Bills | Payment symbol | Icon + full action label | A monetary action needs explicit meaning. |
| Post / Void / Reverse | Optional supporting icon | Always retain the verb, object and final review text | A trash icon is inappropriate for immutable accounting history. |

Use one outline family, consistent optical weight and neutral default colors. Suggested component model: `Icon`, `IconButton`, `ActionButton`, `CommandBar` and `FormFooter`. Reuse the existing shared controls and review guards; keep accounting callbacks in their domain modules. Do not add an additional generic financial-save layer.

Every icon-only action needs a semantic accessible name, hover text, visible keyboard focus, appropriate enabled/disabled appearance and a target that fits its row. Hover alone is insufficient for keyboard or assistive-technology users. These are implementation acceptance criteria, not a claim that a full accessibility audit has passed.

## Review and error behavior

Use a consistent progression: edit → review → explicit financial commit → success. After a successful commit, show the saved document number and state, then apply Stay / New / Close. On failure, retain all input and show the cause and next step. Keep idempotency and final transaction validation.

Choose the confirmation wording by purpose. Keep Editing fits a financial review. Keep Changes or Cancel fits a reset/discard prompt. Return or Cancel fits restore review. A universal Keep Editing label on every kind of dialog loses context. Cancel never saves or records first; native X retains the existing guard.

Distinguish feedback states: Unsaved changes, Saved, Draft, Recorded and Posted. A generic Ready status is useful for idle state but cannot replace document state. For missing FX, display Looking up USD/CAD for [date], then rate source/date on success or a retained-input recovery path on failure. Do not add an invoice refresh button.

## Delivery order and acceptance

1. **Interaction and sizing:** correct 32/28 icon target mismatch, standardize footer placement/labels, explain disabled actions and audit Clear/Reset scope. Preserve all singleton/dirty guards and financial transactions.
2. **Shared icon actions:** move existing manual icon drawing into one size-aware component; migrate row trash/edit/copy and document actions. Check native theme, focus and disabled states at 100%, 125%, 150% and 200% DPI.
3. **Page refinement:** consolidate Bill continuations; tighten receipt summary/grid; separate center tabs and commands; align headers/amounts and summaries. Test narrow and wide windows without removing valid input.
4. **Efficiency and acceptance:** keyboard-only entry, logical Tab order, declared shortcuts that actually work, saved user layout preferences and physical multi-monitor acceptance. Do not claim these from offscreen raster tests.

Acceptance should include realistic tasks: create a Bill with two tax treatments; fix an invalid amount without losing input; receive a partial payment and inspect the new balance; reopen an existing singleton on another monitor; cancel/reset a dirty form; reverse an existing record with explicit date and reason. Measure task completion, misclicks, recovery and keyboard usability before further visual changes.

## Reference principles

- [Microsoft command bars](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar): common commands, labelled icons and overflow as space narrows.
- [SAP action placement](https://www.sap.com/design-system/fiori-design-web/v1-120/foundations/best-practices/global-patterns/action-placement): actions near their scope, clear primary emphasis and consistent workflow placement.
- [W3C target size](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum): target size and spacing matter. Its CSS-pixel requirements are web criteria; the proposed desktop logical sizes here are a design choice, not an assertion of WCAG conformance.
