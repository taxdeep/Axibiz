use super::*;
use crate::{accounting::Expense, expense_doc::Document};
pub(super) struct ExpenseSheet {
    pub doc: Document,
    date: chrono::NaiveDate,
    due: chrono::NaiveDate,
    has_due: bool,
    due_manual: bool,
    keys: Vec<u64>,
    next_key: u64,
    default_tax_code: i64,
    baseline: Box<Document>,
    advanced: bool,
}
impl ExpenseSheet {
    pub(super) fn normalized(&self) -> Document {
        let mut d = self.doc.clone();
        d.lines.retain(Self::started);
        d
    }

    pub(super) fn dirty(&self) -> bool {
        self.doc != *self.baseline
    }
    pub(super) fn mark_clean(&mut self) {
        *self.baseline = self.doc.clone();
    }
    pub(super) fn started(line: &Expense) -> bool {
        line.id > 0
            || !line.amount.trim().is_empty()
            || !line.description.trim().is_empty()
            || line.task.is_some()
    }
    pub fn apply_vendor_defaults(&mut self, vendor: &crate::vendor::Vendor) {
        if self.doc.id != 0 {
            return;
        }
        self.default_tax_code = vendor.tax_code;
        if self.doc.terms.is_empty() {
            self.doc.terms = vendor.profile.payment_terms.clone();
        }
        self.doc.vendor = Some(vendor.id);
        if self
            .doc
            .lines
            .iter()
            .all(|line| line.amount.trim().is_empty())
        {
            self.doc.currency = vendor.currency.clone();
            for line in &mut self.doc.lines {
                line.currency = vendor.currency.clone();
            }
        }
        if !vendor.profile.default_accounts.is_empty()
            && self.doc.lines.iter().all(|line| !Self::started(line))
        {
            self.doc.lines.clear();
            self.keys.clear();
            for account in &vendor.profile.default_accounts {
                self.add_line(*account);
            }
        }
        for line in &mut self.doc.lines {
            if line.id == 0 && line.tax.code == 0 {
                line.tax.code = vendor.tax_code;
            }
        }
        if !self.has_due {
            let days = if vendor.profile.payment_terms == "Due on receipt" {
                Some(0)
            } else {
                vendor
                    .profile
                    .payment_terms
                    .strip_prefix("Net ")
                    .and_then(|v| v.parse::<u64>().ok())
            };
            if let Some(due) =
                days.and_then(|days| self.date.checked_add_days(chrono::Days::new(days)))
            {
                self.due = due;
                self.has_due = true;
                self.doc.due_date = Some(due.to_string());
            }
        }
    }
    pub fn new(doc: Document) -> Self {
        let date = chrono::NaiveDate::parse_from_str(&doc.date, "%Y-%m-%d")
            .unwrap_or_else(|_| crate::today());
        let due = doc
            .due_date
            .as_ref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or(date);
        let count = doc.lines.len() as u64;
        let advanced = doc
            .lines
            .iter()
            .any(|l| l.task.is_some() || l.tax.inclusive || l.tax.recovery != 100);
        Self {
            advanced,
            has_due: doc.due_date.is_some(),
            due_manual: doc.due_date.is_some(),
            baseline: Box::new(doc.clone()),
            doc,
            date,
            due,
            keys: (1..=count).collect(),
            next_key: count + 1,
            default_tax_code: 0,
        }
    }
    pub fn ready(&self) -> bool {
        self.readiness().is_none()
    }
    pub(super) fn readiness(&self) -> Option<String> {
        if !self.doc.vendor.is_some_and(|v| v > 0) {
            return Some("Select a vendor to continue.".into());
        }
        let mut started = false;
        for (i, line) in self
            .doc
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| Self::started(l))
        {
            started = true;
            let issue = if line.account <= 0 {
                Some("select an account")
            } else if line.description.trim().is_empty() {
                Some("enter a description")
            } else if !crate::parse_money(&line.amount).is_ok_and(|v| v > 0) {
                Some("enter a positive amount")
            } else if line.task.is_some_and(|v| v <= 0) {
                Some("select a task or unlink this line")
            } else {
                None
            };
            if let Some(issue) = issue {
                return Some(format!("Line {}: {issue}.", i + 1));
            }
        }
        (!started)
            .then(|| "Add an expense line with an account, description and positive amount.".into())
    }
    fn add_line(&mut self, account: i64) {
        self.doc.lines.push(Expense {
            tax: crate::tax::Input {
                code: self.default_tax_code,
                ..Default::default()
            },
            id: 0,
            date: self.date.to_string(),
            description: String::new(),
            account,
            vendor: self.doc.vendor,
            currency: self.doc.currency.clone(),
            amount: String::new(),
            task: None,
        });
        self.keys.push(self.next_key);
        self.next_key += 1;
    }
    pub fn clear(&mut self) {
        let account = self.doc.lines.first().map(|l| l.account).unwrap_or(0);
        self.doc.lines.clear();
        self.keys.clear();
        self.doc.reference.clear();
        self.doc.memo.clear();
        self.has_due = false;
        self.due_manual = false;
        self.add_line(account);
    }
    pub fn next(&mut self) {
        self.doc.id = 0;
        self.clear();
        self.mark_clean();
    }
}
impl Desktop {
    pub(super) fn expense_sheet(&mut self, ui: &mut egui::Ui, sheet: &mut ExpenseSheet) {
        self.show_gl_source(ui, "expense", sheet.doc.id);
        sheet.default_tax_code = self
            .db
            .query_row(
                "SELECT code FROM tax_assignments WHERE entity='vendor' AND record=?",
                [sheet.doc.vendor.unwrap_or(0)],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let previous_vendor = sheet.doc.vendor;
        let previous_date = sheet.date;
        let previous_due = sheet.due;
        let previous_terms = sheet.doc.terms.clone();
        let historical_vendor = if sheet.doc.id > 0 {
            self.db
                .query_row(
                    "SELECT vendor FROM expense_documents WHERE id=?",
                    [sheet.doc.id],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten()
                .unwrap_or(0)
        } else {
            0
        };
        let vendors = self.rows(&format!(
            "SELECT id,name FROM vendors WHERE active=1 OR id={} ORDER BY name",
            historical_vendor
        ));
        let accounts=self.rows("SELECT id,code || ' · ' || name FROM accounts WHERE active=1 AND kind='Expense' ORDER BY code");
        let tasks=self.rows("SELECT id,title,number FROM tasks WHERE cancelled=0 AND NOT EXISTS(SELECT 1 FROM invoice_tasks WHERE task=tasks.id) ORDER BY id DESC");
        let body_bottom = ui.ctx().content_rect().bottom() - 170.0;
        egui::Frame::new()
            .fill(Color32::from_rgb(240, 247, 250))
            .inner_margin(6)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Vendor *");
                    picker_with_width(
                        ui,
                        &format!("expense_vendor_{}", self.accounting.epoch),
                        sheet.doc.vendor.get_or_insert(0),
                        &vendors,
                        200.0,
                    );
                    ui.label("Date");
                    ui.add(
                        egui_extras::DatePickerButton::new(&mut sheet.date)
                            .id_salt("expense_doc_date"),
                    );
                    ui.label("Currency");
                    currency_picker(ui, "expense_doc_currency", &mut sheet.doc.currency);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Reference");
                    ui.add(
                        egui::TextEdit::singleline(&mut sheet.doc.reference).desired_width(170.0),
                    );
                    ui.label("Terms");
                    egui::ComboBox::from_id_salt("bill_terms")
                        .width(100.0)
                        .selected_text(if sheet.doc.terms.is_empty() {
                            "Not specified"
                        } else {
                            &sheet.doc.terms
                        })
                        .show_ui(ui, |ui| {
                            for term in
                                ["", "Due on receipt", "Net 15", "Net 30", "Net 45", "Net 60"]
                            {
                                ui.selectable_value(
                                    &mut sheet.doc.terms,
                                    term.into(),
                                    if term.is_empty() {
                                        "Not specified"
                                    } else {
                                        term
                                    },
                                );
                            }
                        });
                    ui.checkbox(&mut sheet.has_due, "Due date");
                    ui.add_enabled_ui(sheet.has_due, |ui| {
                        ui.add(
                            egui_extras::DatePickerButton::new(&mut sheet.due)
                                .id_salt("expense_doc_due"),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Memo");
                    let width = invoice_sheet::finite_width(ui).max(120.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut sheet.doc.memo)
                            .desired_width((width - 8.0).max(100.0)),
                    );
                });
                if let Some(vendor) = sheet.doc.vendor {
                    let contact = crate::vendor::load(&self.db, vendor)
                        .map(|v| {
                            [
                                &v.profile.company,
                                &v.profile.contact_name,
                                &v.profile.phone,
                                &v.profile.email,
                                &v.profile.address,
                                &v.contact,
                            ]
                            .into_iter()
                            .filter(|s| !s.trim().is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n")
                        })
                        .unwrap_or_default();
                    if !contact.is_empty() {
                        ui.collapsing("Vendor details", |ui| {
                            ui.label(contact);
                        });
                    }
                }
                if vendors.is_empty() {
                    ui.colored_label(
                        Color32::DARK_RED,
                        "Add a vendor before entering an expense.",
                    );
                }
            });
        if sheet.due != previous_due {
            sheet.due_manual = true;
        }
        if sheet.doc.terms != previous_terms || (sheet.date != previous_date && !sheet.due_manual) {
            let days = if sheet.doc.terms == "Due on receipt" {
                Some(0)
            } else {
                sheet
                    .doc
                    .terms
                    .strip_prefix("Net ")
                    .and_then(|s| s.parse::<u64>().ok())
            };
            if let Some(due) =
                days.and_then(|days| sheet.date.checked_add_days(chrono::Days::new(days)))
            {
                sheet.due = due;
                sheet.has_due = true;
                sheet.due_manual = false;
            }
        }
        sheet.doc.date = sheet.date.to_string();
        sheet.doc.due_date = sheet.has_due.then(|| sheet.due.to_string());
        if sheet.doc.vendor != previous_vendor {
            if previous_vendor.is_none_or(|id| id == 0) {
                if let Ok(vendor) = crate::vendor::load(&self.db, sheet.doc.vendor.unwrap_or(0)) {
                    sheet.apply_vendor_defaults(&vendor);
                }
            }
            let code = self
                .db
                .query_row(
                    "SELECT code FROM tax_assignments WHERE entity='vendor' AND record=?",
                    [sheet.doc.vendor.unwrap_or(0)],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0);
            sheet.default_tax_code = code;
            for line in &mut sheet.doc.lines {
                if line.id == 0 && line.tax.code == 0 {
                    line.tax.code = code;
                }
            }
        }
        let mut remove = None;
        let width = invoice_sheet::finite_width(ui).max(if sheet.advanced { 860.0 } else { 640.0 });
        let widths = [
            24.0,
            190.0,
            90.0,
            width - if sheet.advanced { 678.0 } else { 416.0 },
            140.0,
            80.0,
            52.0,
            70.0,
            32.0,
        ];
        let visible: &[usize] = if sheet.advanced {
            &[0, 1, 2, 3, 4, 5, 6, 7, 8]
        } else {
            &[0, 1, 2, 3, 5, 8]
        };
        let headers = [
            "#",
            "ACCOUNT",
            "AMOUNT",
            "DESCRIPTION",
            "TASK",
            "TAX",
            "INCL.",
            "RECOV.%",
            "",
        ];
        let widths: Vec<_> = visible.iter().map(|i| widths[*i]).collect();
        let headers: Vec<_> = visible.iter().map(|i| headers[*i]).collect();
        let height = (body_bottom - ui.cursor().min.y - 115.0).max(84.0);
        invoice_sheet::sheet_columns(
            ui,
            "expense_lines",
            height,
            &headers,
            &widths,
            sheet.doc.lines.len(),
            |ui, index, column| {
                let column = visible[column];
                let key = sheet.keys[index];
                let line = &mut sheet.doc.lines[index];
                match column {
                    0 => {
                        ui.label((index + 1).to_string());
                    }
                    1 => {
                        let width = (ui.available_width()
                            - crate::autocomplete::ARROW_WIDTH
                            - ui.spacing().item_spacing.x)
                            .max(50.0);
                        picker_with_width(
                            ui,
                            &format!("expense_account_{}_{}", self.accounting.epoch, key),
                            &mut line.account,
                            &accounts,
                            width,
                        );
                    }
                    2 => {
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::singleline(&mut line.amount)
                                .id(egui::Id::new(("expense_line", key)))
                                .frame(false)
                                .horizontal_align(egui::Align::RIGHT)
                                .hint_text("0.00"),
                        );
                    }
                    3 => {
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::singleline(&mut line.description)
                                .id(egui::Id::new(("expense_memo", key)))
                                .frame(false)
                                .hint_text("Description"),
                        );
                    }
                    4 => {
                        let mut linked = line.task.is_some();
                        if ui.checkbox(&mut linked, "").changed() {
                            line.task = linked.then_some(0);
                        }
                        if let Some(task) = &mut line.task {
                            let width = (ui.available_width()
                                - crate::autocomplete::ARROW_WIDTH
                                - ui.spacing().item_spacing.x)
                                .max(35.0);
                            picker_with_width(
                                ui,
                                &format!("expense_task_{}_{}", self.accounting.epoch, key),
                                task,
                                &tasks,
                                width,
                            );
                        } else {
                            ui.small("None");
                        }
                    }
                    5 => invoice_sheet::tax_code(ui, &self.db, key, &mut line.tax.code),
                    6 => {
                        ui.checkbox(&mut line.tax.inclusive, "");
                    }
                    7 => {
                        ui.add(egui::DragValue::new(&mut line.tax.recovery).range(0..=100));
                    }
                    _ => {
                        if theme::trash_button(ui, "Delete line").clicked() {
                            remove = Some(index);
                        }
                    }
                }
            },
        );
        if let Some(index) = remove {
            sheet.doc.lines.remove(index);
            sheet.keys.remove(index);
        }
        let add = ui
            .horizontal(|ui| {
                let add = icons::action(ui, icons::Icon::Add, "Add line", true);
                ui.checkbox(&mut sheet.advanced, "Advanced columns")
                    .on_hover_text(
                        "Task, tax-included and recovery percentage. Hidden values are retained.",
                    );
                add
            })
            .inner;
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("expense_add_rect"), add.rect));
        if add.clicked() {
            sheet.add_line(accounting_default(&self.db));
            if let Some(line) = sheet.doc.lines.last_mut() {
                line.tax.code = self
                    .db
                    .query_row(
                        "SELECT code FROM tax_assignments WHERE entity='vendor' AND record=?",
                        [sheet.doc.vendor.unwrap_or(0)],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
            }
        }
        ui.add_space(4.0);
        let amounts = (|| -> Result<(i64, i64, i64)> {
            let (mut gross, mut credit, mut cost) = (0i64, 0i64, 0i64);
            for line in sheet.doc.lines.iter().filter(|l| ExpenseSheet::started(l)) {
                let mut draft = line.clone();
                draft.date = sheet.doc.date.clone();
                draft.currency = sheet.doc.currency.clone();
                let calc = crate::tax::expense_calc(&self.db, &draft)?;
                gross = gross
                    .checked_add(calc["gross"].as_i64().unwrap())
                    .ok_or_else(|| anyhow::anyhow!("Total is too large."))?;
                credit = credit
                    .checked_add(calc["recoverable"].as_i64().unwrap())
                    .ok_or_else(|| anyhow::anyhow!("Total is too large."))?;
                cost = cost
                    .checked_add(calc["cost"].as_i64().unwrap())
                    .ok_or_else(|| anyhow::anyhow!("Total is too large."))?;
            }
            Ok((gross, credit, cost))
        })();
        let total = amounts.as_ref().ok().map(|a| a.0);
        ui.allocate_ui_with_layout(
            egui::vec2(invoice_sheet::finite_width(ui), theme::ROW_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let total = ui.strong(format!(
                    "Total: {} {}",
                    sheet.doc.currency,
                    total.map(crate::format_money).unwrap_or_else(|| "—".into())
                ));
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("expense_total_rect"), total.rect));
                #[cfg(not(test))]
                let _ = total;
            },
        );
        if let Ok((_, credit, cost)) = amounts {
            ui.label(format!(
                "Recoverable tax: {} {}  |  Cost: {} {}",
                sheet.doc.currency,
                crate::format_money(credit),
                sheet.doc.currency,
                crate::format_money(cost)
            ));
        }
        ui.collapsing("Tax and entry notes", |ui| {
            ui.small("Recovery applies only to eligible registered taxes. PST is normally included in cost.");
            ui.small("All lines save together. Invoiced or cancelled tasks cannot be changed.");
        });
    }
    pub(super) fn save_expense_sheet(&mut self, sheet: &ExpenseSheet) -> Result<i64> {
        self.prepare_expense_quotes(sheet)?;
        crate::expense_doc::save(&mut self.db, &sheet.normalized())
    }
    pub(super) fn prepare_expense_quotes(&mut self, sheet: &ExpenseSheet) -> Result<()> {
        let needs_quote = if sheet.doc.id == 0 {
            true
        } else {
            self.db.query_row("SELECT e.currency!=?1 OR e.date!=?2 FROM gl_sources s JOIN gl_entries e ON e.id=s.entry WHERE s.kind='expense' AND s.record=?3",rusqlite::params![sheet.doc.currency,sheet.doc.date,sheet.doc.id],|r|r.get::<_,bool>(0)).optional()?.unwrap_or(false)
        };
        if needs_quote && !self.prepare_gl_fx(&sheet.doc.currency, &sheet.doc.date) {
            anyhow::bail!(self.message.clone());
        }
        for line in sheet.doc.lines.iter().filter(|l| ExpenseSheet::started(l)) {
            let taxable: bool = self.db.query_row(
                "SELECT kind='taxable' FROM tax_codes WHERE id=?",
                [line.tax.code],
                |r| r.get(0),
            )?;
            if taxable && self.missing_dated_fx(&sheet.doc.currency, "CAD", &sheet.doc.date) {
                anyhow::bail!(self.message.clone());
            }
            if let Some(task) = line.task {
                let target: String =
                    self.db
                        .query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
                            r.get(0)
                        })?;
                if self.missing_fx(&sheet.doc.currency, &target) {
                    anyhow::bail!(self.message.clone());
                }
            }
        }
        Ok(())
    }
}
fn accounting_default(db: &Connection) -> i64 {
    crate::accounting::default_account(db).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_column_visibility_retains_task_and_tax_inputs() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 6;
        app.open_accounting_form(0);
        let Some(super::accounting_ui::Form::Expense { sheet }) = &mut app.accounting.form else {
            panic!("Bill expected")
        };
        assert!(!sheet.advanced);
        sheet.doc.lines[0].task = Some(42);
        sheet.doc.lines[0].tax.inclusive = true;
        sheet.doc.lines[0].tax.recovery = 50;
        let mut sheet = ExpenseSheet::new(sheet.doc.clone());
        assert!(sheet.advanced);
        let before = sheet.normalized();
        sheet.advanced = false;
        assert_eq!(sheet.normalized(), before);
        sheet.advanced = true;
        assert_eq!(sheet.normalized(), before);
        Ok(())
    }
    #[test]
    fn default_account_rows_save_only_started_lines_and_snapshot_terms() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let account = crate::accounting::default_account(&db)?;
        let other: i64 = db.query_row("SELECT id FROM accounts WHERE code='5100'", [], |r| {
            r.get(0)
        })?;
        let mut vendor = crate::vendor::Vendor {
            name: "Defaulted supplier".into(),
            ..Default::default()
        };
        vendor.profile.default_accounts = vec![account, other];
        vendor.profile.payment_terms = "Net 30".into();
        vendor.id = crate::vendor::save(&mut db, &vendor)?;
        let mut app = Desktop::new(db, PathBuf::from("bill-defaults-test.db"))?;
        app.page = 6;
        app.open_accounting_form(0);
        let Some(accounting_ui::Form::Expense { mut sheet }) = app.accounting.form.take() else {
            panic!("Bill editor expected")
        };
        sheet.apply_vendor_defaults(&vendor);
        sheet.mark_clean();
        assert!(!sheet.dirty());
        assert_eq!(sheet.doc.lines.len(), 2);
        assert!(!sheet.ready());
        sheet.doc.lines[0].description = "Services".into();
        sheet.doc.lines[0].amount = "10.00".into();
        assert!(sheet.ready());
        let id = app.save_expense_sheet(&sheet)?;
        let saved = crate::expense_doc::load(&app.db, id)?;
        assert_eq!(saved.lines.len(), 1);
        assert_eq!(saved.terms, "Net 30");
        assert_eq!(saved.lines[0].account, account);
        sheet.doc.lines[1].description = "Started but incomplete".into();
        assert!(!sheet.ready());
        Ok(())
    }

    #[test]
    fn vendor_defaults_apply_to_new_lines_without_overwriting_manual_values_or_saved_documents() {
        let document = Document {
            id: 0,
            vendor: None,
            date: "2026-10-30".into(),
            currency: "CAD".into(),
            reference: String::new(),
            due_date: None,
            memo: String::new(),
            terms: String::new(),
            lines: vec![],
        };
        let mut sheet = ExpenseSheet::new(document);
        sheet.add_line(1);
        let mut vendor = crate::vendor::Vendor {
            id: 1,
            currency: "USD".into(),
            tax_code: 1,
            ..Default::default()
        };
        vendor.profile.payment_terms = "Net 30".into();
        sheet.apply_vendor_defaults(&vendor);
        assert_eq!(sheet.doc.vendor, Some(1));
        assert_eq!(sheet.doc.currency, "USD");
        assert_eq!(sheet.doc.due_date, Some("2026-11-29".into()));
        sheet.doc.lines[0].tax.code = 3;
        sheet.add_line(1);
        assert_eq!(sheet.doc.lines[0].tax.code, 3);
        assert_eq!(sheet.doc.lines[1].tax.code, 1);
        sheet.doc.currency = "EUR".into();
        sheet.doc.lines[0].amount = "42.00".into();
        sheet.apply_vendor_defaults(&vendor);
        assert_eq!(sheet.doc.currency, "EUR");
        assert_eq!(sheet.doc.lines[0].amount, "42.00");
        assert_eq!(sheet.doc.lines[0].tax.code, 3);
        sheet.doc.id = 42;
        vendor.id = 2;
        vendor.currency = "CNY".into();
        vendor.profile.payment_terms = "Net 60".into();
        sheet.apply_vendor_defaults(&vendor);
        assert_eq!(sheet.doc.vendor, Some(1));
        assert_eq!(sheet.doc.currency, "EUR");
        assert_eq!(sheet.doc.due_date, Some("2026-11-29".into()));
    }
}
