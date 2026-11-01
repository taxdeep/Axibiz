//! One catalog for navigation, page headings, search and creation entry points.
use super::*;
pub(super) struct Page {
    pub title: &'static str,
    pub description: &'static str,
    pub create: Option<&'static str>,
    pub search: bool,
}
const fn page(
    title: &'static str,
    description: &'static str,
    create: Option<&'static str>,
    search: bool,
) -> Page {
    Page {
        title,
        description,
        create,
        search,
    }
}
pub(super) const PAGES: [Page; 18] = [
    page(
        "Tasks",
        "Business tasks and invoicing status",
        Some("＋ New Task"),
        true,
    ),
    page(
        "Customers",
        "Customer profiles and receivables",
        Some("+ New Customer"),
        false,
    ),
    page(
        "Vendors",
        "Supplier profiles and payables",
        Some("+ New Vendor"),
        false,
    ),
    page(
        "Products & Services",
        "Service items, prices and currencies",
        Some("＋ New Item"),
        true,
    ),
    page(
        "Exchange Rates",
        "Saved quotes and automatic background updates",
        None,
        false,
    ),
    page(
        "Company Settings",
        "Contact, legal and registration details, reporting preferences and default currency",
        None,
        false,
    ),
    page(
        "Bills",
        "Supplier bills and outstanding payables",
        Some("+ Enter Bill"),
        true,
    ),
    page(
        "Chart of Accounts",
        "Account codes and classifications",
        Some("＋ New Account"),
        true,
    ),
    page(
        "Invoices",
        "Invoices, receipts and outstanding balances",
        Some("＋ New Invoice"),
        true,
    ),
    page(
        "Document Numbering",
        "Prefixes and numbering sequences",
        None,
        false,
    ),
    page(
        "Audit Log",
        "Recorded additions, changes and reversals",
        None,
        true,
    ),
    page(
        "Sales Tax",
        "Tax accounts, remittances and period reports",
        None,
        false,
    ),
    page(
        "Sales Tax Setup",
        "Agencies, historical rates and tax assignments",
        None,
        false,
    ),
    page(
        "Journal Entries",
        "Drafts, source postings and dated reversals",
        Some("＋ New Journal"),
        true,
    ),
    page(
        "General Ledger",
        "General ledger and trial balance",
        None,
        false,
    ),
    page(
        "GL Setup",
        "Base currency, control accounts and period locks",
        None,
        false,
    ),
    page(
        "Settings",
        "Software preferences for backups and time display",
        None,
        false,
    ),
    page(
        "Users",
        "Company users and optional password sign-in",
        None,
        false,
    ),
];
impl Desktop {
    pub(super) fn inline_settings_dirty(&self) -> bool {
        self.gl.setup_dirty()
            || self.tax_ui.dirty()
            || self.number_rules.as_ref().is_some_and(|draft| {
                crate::numbering::rules(&self.db).map_or(true, |saved| *draft != saved)
            })
    }

    pub(super) fn navigate(&mut self, page: usize) {
        if page >= PAGES.len() || self.page == page {
            return;
        }
        self.page_queries[self.page] = self.list_query.clone();
        self.page_selected[self.page] = self.selected;
        self.page = page;
        self.list_query = self.page_queries[page].clone();
        self.selected = self.page_selected[page];
    }
    pub(super) fn create_on(&mut self, page: usize) {
        if PAGES.get(page).is_some_and(|p| p.create.is_some()) {
            self.navigate(page);
            self.open_creation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_preserves_search_selection_and_numbering_drafts() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.list_query = "Task search".into();
        app.selected = 42;
        app.navigate(6);
        app.list_query = "Bill search".into();
        app.selected = 24;
        app.navigate(0);
        assert_eq!(app.list_query, "Task search");
        assert_eq!(app.selected, 42);
        app.navigate(6);
        assert_eq!(app.list_query, "Bill search");
        assert_eq!(app.selected, 24);
        app.number_rules = Some(crate::numbering::rules(&app.db)?);
        assert!(!app.inline_settings_dirty());
        app.number_rules.as_mut().unwrap()[0].prefix = "NEW".into();
        app.navigate(9);
        app.navigate(0);
        app.navigate(9);
        assert_eq!(app.number_rules.as_ref().unwrap()[0].prefix, "NEW");
        assert!(app.has_business_editors());
        app.reset_after_restore()?;
        assert!(!app.inline_settings_dirty());
        assert!(app.number_rules.is_none());
        Ok(())
    }
    #[test]
    fn every_page_renders_read_only_at_three_sizes_empty_and_populated() -> Result<()> {
        for populated in [false, true] {
            let db = Connection::open_in_memory()?;
            crate::init(&db)?;
            if populated {
                db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',100,200,'CAD');")?;
            }
            let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
            let changes = app.db.total_changes();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            for size in [
                egui::vec2(900.0, 650.0),
                egui::vec2(1440.0, 900.0),
                egui::vec2(1920.0, 1080.0),
            ] {
                for page in 0..PAGES.len() {
                    app.navigate(page);
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ctx| app.draw(ctx),
                    );
                    assert_eq!(
                        app.db.total_changes(),
                        changes,
                        "Rendering page {page} must not save data"
                    );
                }
            }
        }
        Ok(())
    }
}
