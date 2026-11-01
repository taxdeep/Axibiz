use super::*;
use crate::accounting::{self, Expense};
#[derive(Default)]
pub(super) struct AccountingUi {
    pub(super) form: Option<Form>,
    error: String,
    pub(super) epoch: u64,
    baseline: String,
    review: Option<BillReview>,
    review_focus: bool,
    clear_guard: controls::CloseGuard,
    after_record: AfterRecord,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AfterRecord {
    Stay,
    #[default]
    Close,
    New,
}
struct BillReview {
    doc: crate::expense_doc::Document,
    signature: String,
    gross: i64,
    recoverable: i64,
    after: AfterRecord,
}
pub(super) enum Form {
    Account {
        id: i64,
        code: String,
        name: String,
        kind: String,
        active: bool,
    },
    Expense {
        sheet: super::expense_ui::ExpenseSheet,
    },
}
impl Form {
    fn fingerprint(&self) -> String {
        match self {
            Self::Account {
                id,
                code,
                name,
                kind,
                active,
            } => format!("{id:?}{code:?}{name:?}{kind:?}{active:?}"),
            Self::Expense { .. } => String::new(),
        }
    }
}
impl AccountingUi {
    pub(super) fn dirty(&self) -> Option<bool> {
        self.form.as_ref().map(|f| match f {
            Form::Expense { sheet } => sheet.dirty(),
            _ => f.fingerprint() != self.baseline,
        })
    }
}
impl Desktop {
    fn baseline_account(&mut self, form: &Form) {
        self.accounting.baseline = form.fingerprint();
    }
    pub(super) fn save_accounting_form(&mut self, form: &mut Form) -> bool {
        self.save_accounting_checked(form, None)
    }
    fn save_accounting_checked(&mut self, form: &mut Form, expected: Option<&str>) -> bool {
        if matches!(form,Form::Expense{sheet} if sheet.doc.id>0&&!sheet.dirty()) {
            return true;
        }
        let result = (|| -> Result<i64> {
            match form {
                Form::Account {
                    id,
                    code,
                    name,
                    kind,
                    active,
                } => {
                    let saved = accounting::save_account(&self.db, *id, code, name, kind, *active)?;
                    *id = saved;
                    Ok(saved)
                }
                Form::Expense { sheet } => {
                    let id = if let Some(signature) = expected {
                        self.prepare_expense_quotes(sheet)?;
                        crate::expense_doc::save_reviewed(
                            &mut self.db,
                            &sheet.normalized(),
                            signature,
                        )?
                    } else {
                        self.save_expense_sheet(sheet)?
                    };
                    sheet.doc.id = id;
                    *sheet = super::expense_ui::ExpenseSheet::new(crate::expense_doc::load(
                        &self.db, id,
                    )?);
                    Ok(id)
                }
            }
        })();
        match result {
            Ok(id) => {
                self.baseline_account(form);
                self.message = format!("Saved #{id}");
                self.error = false;
                self.accounting.error.clear();
                self.accounting.epoch = self.accounting.epoch.wrapping_add(1);
                true
            }
            Err(e) => {
                self.accounting.error = format!("{e:#}");
                false
            }
        }
    }
    fn prepare_bill_review(&mut self, form: &Form, after: AfterRecord) {
        let Form::Expense { sheet } = form else {
            return;
        };
        let result = (|| -> Result<BillReview> {
            self.prepare_expense_quotes(sheet)?;
            let doc = sheet.normalized();
            let p = crate::expense_doc::review(&self.db, &doc)?;
            Ok(BillReview {
                doc,
                signature: p.signature,
                gross: p.gross,
                recoverable: p.recoverable,
                after,
            })
        })();
        match result {
            Ok(review) => {
                self.accounting.review = Some(review);
                self.accounting.review_focus = true;
                self.accounting.error.clear()
            }
            Err(e) => self.accounting.error = format!("{e:#}"),
        }
    }
    pub(super) fn review_accounting_close(&mut self) {
        if let Some(form) = self.accounting.form.take() {
            self.prepare_bill_review(&form, AfterRecord::Close);
            self.accounting.form = Some(form);
        }
    }
    pub(super) fn open_accounting_form(&mut self, id: i64) {
        if self.reuse_business(windows::Kind::Accounting) {
            return;
        }
        self.accounting.error.clear();
        self.accounting.review = None;
        self.accounting.epoch = self.accounting.epoch.wrapping_add(1);
        let result = (|| -> Result<Form> {
            if self.page == 7 {
                if id == 0 {
                    Ok(Form::Account {
                        id: 0,
                        code: String::new(),
                        name: String::new(),
                        kind: "Expense".into(),
                        active: true,
                    })
                } else {
                    Ok(self.db.query_row(
                        "SELECT id,code,name,kind,active FROM accounts WHERE id=?",
                        [id],
                        |r| {
                            Ok(Form::Account {
                                id: r.get(0)?,
                                code: r.get(1)?,
                                name: r.get(2)?,
                                kind: r.get(3)?,
                                active: r.get(4)?,
                            })
                        },
                    )?)
                }
            } else {
                let doc = if id > 0 {
                    crate::expense_doc::load(&self.db, id)?
                } else {
                    crate::expense_doc::Document {
                        id: 0,
                        date: crate::today().to_string(),
                        vendor: None,
                        currency: Settings::load(&self.db)?.currency,
                        reference: String::new(),
                        due_date: None,
                        memo: String::new(),
                        terms: String::new(),
                        lines: vec![Expense {
                            tax: Default::default(),
                            id: 0,
                            date: crate::today().to_string(),
                            description: String::new(),
                            account: accounting::default_account(&self.db)?,
                            vendor: None,
                            currency: Settings::load(&self.db)?.currency,
                            amount: String::new(),
                            task: None,
                        }],
                    }
                };
                Ok(Form::Expense {
                    sheet: super::expense_ui::ExpenseSheet::new(doc),
                })
            }
        })();
        match result {
            Ok(form) => {
                self.baseline_account(&form);
                self.accounting.form = Some(form)
            }
            Err(e) => {
                self.message = format!("{e:#}");
                self.error = true;
            }
        }
    }
    pub(super) fn accounts_page(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "会计科目表 · 按资产、负债、权益、收入、费用分类；费用单只能使用启用的 Expense 科目。",
        );
        ui.small("内置服务业默认科目模板，首次打开自动添加；可编辑编号与名称，或停用不需要的科目。已有科目不会被覆盖。");
        let rows=self.rows("SELECT id,code,name,kind,CASE active WHEN 1 THEN '启用' ELSE '停用' END FROM accounts ORDER BY code,id");
        let rows: Vec<_> = rows.into_iter().filter(|r| self.row_matches(r)).collect();
        if let Some(id) = accounting_table(
            ui,
            "accounts_table",
            &["ID", "科目编号", "名称", "Type", "状态", "操作"],
            &rows,
        ) {
            self.open_accounting_form(id);
        }
        ui.small("默认费用科目通过固定关联保存；可修改编号与名称，其他科目可停用。");
    }
    pub(super) fn expenses_page(&mut self, ui: &mut egui::Ui) {
        ui.label("独立费用可不关联 Task；关联 Task 时按保存的汇率换算，并计入该任务成本。");
        let rows=self.rows("SELECT d.id,COALESCE(d.number,''),COALESCE(d.date,''),COALESCE(v.name,''),d.reference,CASE WHEN length(trim(d.memo))>0 THEN d.memo ELSE group_concat(e.description,'; ') END,d.currency,printf('%d.%02d',sum(COALESCE(t.gross,e.amount))/100,sum(COALESCE(t.gross,e.amount))%100),count(e.id),COALESCE(d.due_date,'') FROM expense_documents d JOIN expense_document_lines x ON x.document=d.id JOIN ledger_expenses e ON e.id=x.expense LEFT JOIN expense_tax t ON t.expense=e.id LEFT JOIN vendors v ON v.id=d.vendor GROUP BY d.id ORDER BY d.date DESC,d.id DESC");
        let rows: Vec<_> = rows.into_iter().filter(|r| self.row_matches(r)).collect();
        if let Some(id) = accounting_table(
            ui,
            "expenses_table",
            &[
                "ID",
                "Expense #",
                "Date",
                "Vendor",
                "Ref No.",
                "Memo",
                "Currency",
                "Total",
                "Lines",
                "Due Date",
                "",
            ],
            &rows,
        ) {
            self.open_accounting_form(id);
        }
        if rows.is_empty() {
            ui.label("暂无费用，点击右上角 New Expense 添加。");
        }
        ui.separator();
        let result = (|| -> Result<std::collections::BTreeMap<String, i128>> {
            let mut stmt = self
                .db
                .prepare("SELECT l.currency,COALESCE(t.gross,l.amount) FROM ledger_expenses l LEFT JOIN expense_tax t ON t.expense=l.id")?;
            let mut totals = std::collections::BTreeMap::new();
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (currency, amount) = row?;
                *totals.entry(currency).or_insert(0i128) += i128::from(amount);
            }
            Ok(totals)
        })();
        match result {
            Ok(totals) => {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("全部费用合计（按原币种）：");
                    for (currency, amount) in totals {
                        ui.label(format!("{} {}.{:02}", currency, amount / 100, amount % 100));
                    }
                });
            }
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
            }
        }
    }
    pub(super) fn accounting_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.accounting.form.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("accounting_form"), |ui| {
            invoice_sheet::compact(ui);
            let width = if matches!(form, Form::Expense { .. }) {
                (ctx.content_rect().width() - 48.0).max(300.0)
            } else {
                620.0_f32
            };
            ui.set_width(width.min((ctx.content_rect().width() - 48.0).max(300.0)));
            ui.horizontal(|ui| {
                ui.heading(match &form {
                    Form::Account { id, .. } => {
                        if *id == 0 {
                            "New Account"
                        } else {
                            "Edit Account"
                        }
                    }
                    Form::Expense { sheet, .. } => {
                        if sheet.doc.id == 0 {
                            "New Bill"
                        } else {
                            "Bill"
                        }
                    }
                });
                if let Form::Expense { sheet } = &form {
                    if sheet.doc.id > 0 {
                        if let Ok(number) = self.db.query_row(
                            "SELECT number FROM expense_documents WHERE id=?",
                            [sheet.doc.id],
                            |r| r.get::<_, String>(0),
                        ) {
                            ui.label(number);
                        }
                    }
                }
            });
            if let Form::Expense { sheet } = &form {
                ui.small(if sheet.dirty() {
                    "Unsaved changes"
                } else if sheet.doc.id > 0 {
                    "Recorded"
                } else {
                    "New bill - not recorded"
                });
            }
            if let Form::Expense { sheet } = &form {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            sheet.doc.id > 0 && !sheet.dirty(),
                            egui::Button::new("Pay Bill"),
                        )
                        .on_hover_text("Save changes before opening Pay Bills")
                        .clicked()
                    {
                        self.open_pay_bills(sheet.doc.vendor.unwrap_or(0), Some(sheet.doc.id));
                    }
                    if let Ok(account) = crate::gl::role(&self.db, "payable") {
                        let label = self
                            .db
                            .query_row(
                                "SELECT code || ' - ' || name FROM accounts WHERE id=?",
                                [account],
                                |r| r.get::<_, String>(0),
                            )
                            .unwrap_or_default();
                        ui.small(format!("A/P ACCOUNT  {label}"));
                    }
                    if sheet.doc.id > 0
                        && crate::ap::allow_revision(&self.db, sheet.doc.id).is_err()
                    {
                        ui.small("Payment history exists. Financial details are fixed.");
                    }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("accounting_fields")
                .max_height(if ui.available_height().is_finite() {
                    (ui.available_height() - 104.0).max(100.0)
                } else {
                    (ctx.content_rect().height() - 220.0).max(100.0)
                })
                .show(ui, |ui| match &mut form {
                    Form::Account {
                        code,
                        name,
                        kind,
                        active,
                        ..
                    } => {
                        field(ui, "Account code *", code);
                        field(ui, "Name *", name);
                        ui.horizontal(|ui| {
                            ui.label("Type");
                            egui::ComboBox::from_id_salt("account_kind")
                                .selected_text(kind.as_str())
                                .show_ui(ui, |ui| {
                                    for value in accounting::TYPES {
                                        ui.selectable_value(kind, value.into(), value);
                                    }
                                });
                        });
                        ui.checkbox(active, "Active");
                    }
                    Form::Expense { sheet } => self.expense_sheet(ui, sheet),
                });
            if !self.accounting.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("accounting_error")
                    .max_height(36.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::from_rgb(190, 45, 45), &self.accounting.error);
                    });
            }
            let reason = match &form {
                Form::Expense { sheet } => sheet.readiness(),
                Form::Account { code, name, .. } => {
                    if code.trim().is_empty() || name.trim().is_empty() {
                        Some("Enter an account code and name.".into())
                    } else {
                        None
                    }
                }
            };
            controls::footer(ui, reason.as_deref(), |ui| {
                ui.horizontal(|ui| {
                    if matches!(&form,Form::Expense{sheet} if sheet.doc.id==0)
                        && ui.button("Reset form").clicked()
                    {
                        self.accounting.clear_guard.request();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = match &form {
                            Form::Expense { sheet } => sheet.ready(),
                            _ => reason.is_none(),
                        };
                        let expense = matches!(form, Form::Expense { .. });
                        let save_close = controls::primary(
                            ui,
                            if expense {
                                "Review Bill"
                            } else {
                                "Save & Close"
                            },
                            ready,
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("expense_save_close_rect"), save_close.rect)
                        });
                        if save_close.clicked() {
                            if expense {
                                self.prepare_bill_review(&form, self.accounting.after_record);
                            } else {
                                close = self.save_accounting_form(&mut form);
                            }
                        }
                        if expense {
                            egui::ComboBox::from_id_salt("bill_after_record")
                                .width(130.0)
                                .selected_text(match self.accounting.after_record {
                                    AfterRecord::Stay => "Keep open",
                                    AfterRecord::New => "New bill",
                                    AfterRecord::Close => "Close window",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.accounting.after_record,
                                        AfterRecord::Stay,
                                        "Keep open",
                                    );
                                    ui.selectable_value(
                                        &mut self.accounting.after_record,
                                        AfterRecord::New,
                                        "New bill",
                                    );
                                    ui.selectable_value(
                                        &mut self.accounting.after_record,
                                        AfterRecord::Close,
                                        "Close window",
                                    );
                                });
                            ui.label("After recording");
                        } else if ui.add_enabled(ready, egui::Button::new("Save")).clicked() {
                            self.save_accounting_form(&mut form);
                        }
                        if ctx.viewport_id() == egui::ViewportId::ROOT
                            && ui.button("Cancel").clicked()
                        {
                            self.request_business_close(windows::Kind::Accounting);
                        }
                    });
                });
            });
        });
        let dirty = matches!(&form,Form::Expense{sheet} if sheet.dirty());
        if self.accounting.clear_guard.show(ctx, "bill_clear", dirty) {
            if let Form::Expense { sheet } = &mut form {
                sheet.clear();
                self.accounting.review = None;
                self.accounting.error.clear();
                self.accounting.epoch = self.accounting.epoch.wrapping_add(1);
            }
        }
        if let Some(review) = self.accounting.review.take() {
            let valid = match &form {
                Form::Expense { sheet } if sheet.normalized() == review.doc => {
                    crate::expense_doc::review(&self.db, &review.doc)
                        .map(|p| p.signature == review.signature)
                        .unwrap_or(false)
                }
                _ => false,
            };
            let error = if !valid {
                Some("The bill changed. Keep editing and review it again.")
            } else if !self.accounting.error.is_empty() {
                Some(self.accounting.error.as_str())
            } else {
                None
            };
            let vendor = self
                .db
                .query_row(
                    "SELECT name FROM vendors WHERE id=?",
                    [review.doc.vendor.unwrap_or(0)],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_else(|_| "Unavailable".into());
            let values = [
                ("Vendor", vendor),
                ("Date", review.doc.date.clone()),
                (
                    "Amount",
                    format!(
                        "{} {}",
                        review.doc.currency,
                        crate::format_money(review.gross)
                    ),
                ),
                ("Recoverable", crate::format_money(review.recoverable)),
                ("Lines", review.doc.lines.len().to_string()),
                ("Reference", review.doc.reference.clone()),
            ];
            match controls::review(
                ctx,
                controls::Review {
                    cancel: "Keep Editing",
                    id: "bill_review",
                    title: "Review Bill",
                    values: &values,
                    error,
                    confirm: "Confirm & Record",
                    notice: "Records the bill and its accounting entries.",
                },
                &mut self.accounting.review_focus,
            ) {
                Some(controls::ReviewAction::Keep) => {}
                Some(controls::ReviewAction::Confirm) => {
                    if self.save_accounting_checked(&mut form, Some(&review.signature)) {
                        match review.after {
                            AfterRecord::Close => close = true,
                            AfterRecord::Stay => {}
                            AfterRecord::New => {
                                if let Form::Expense { sheet } = &mut form {
                                    sheet.next();
                                    if let Some(v) = sheet
                                        .doc
                                        .vendor
                                        .and_then(|id| crate::vendor::load(&self.db, id).ok())
                                    {
                                        sheet.apply_vendor_defaults(&v);
                                        sheet.mark_clean();
                                    }
                                }
                            }
                        }
                    } else {
                        self.accounting.review = Some(review)
                    }
                }
                None => self.accounting.review = Some(review),
            }
        }
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            self.request_business_close(windows::Kind::Accounting);
        }
        if !close {
            self.accounting.form = Some(form);
        }
    }
}
fn accounting_table(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Option<i64> {
    controls::register(ui, id, headers, rows, &mut 0, true).1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_bill_review_records_once_then_applies_each_selected_outcome() -> Result<()> {
        for after in [AfterRecord::Stay, AfterRecord::New, AfterRecord::Close] {
            let db = Connection::open_in_memory()?;
            crate::init(&db)?;
            db.execute(
                "INSERT INTO vendors(id,name,contact) VALUES(1,'Supplier','')",
                [],
            )?;
            let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
            app.page = 6;
            app.open_accounting_form(0);
            if let Some(Form::Expense { sheet }) = &mut app.accounting.form {
                sheet.doc.vendor = Some(1);
                sheet.doc.lines[0].description = "Office supplies".into();
                sheet.doc.lines[0].amount = "10.00".into();
            }
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let frame = |app: &mut Desktop, events| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.accounting_dialog(ctx),
                );
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let form = app.accounting.form.take().unwrap();
            app.prepare_bill_review(&form, after);
            app.accounting.form = Some(form);
            assert!(app.accounting.review.is_some(), "{}", app.accounting.error);
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            assert_eq!(
                app.db
                    .query_row("SELECT count(*) FROM expense_documents", [], |r| r
                        .get::<_, i64>(0))?,
                0
            );
            let pos = ctx
                .data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new(("bill_review", "review_confirm")))
                })
                .unwrap()
                .center();
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(
                app.db
                    .query_row("SELECT count(*) FROM expense_documents", [], |r| r
                        .get::<_, i64>(0))?,
                1,
                "{}",
                app.accounting.error
            );
            match (after, &app.accounting.form) {
                (AfterRecord::Close, None) => {}
                (AfterRecord::New, Some(Form::Expense { sheet })) => assert_eq!(sheet.doc.id, 0),
                (AfterRecord::Stay, Some(Form::Expense { sheet })) => assert!(sheet.doc.id > 0),
                _ => panic!("The recorded bill did not apply its continuation"),
            }
            crate::storage::validate_projections(&app.db)?;
        }
        Ok(())
    }
    #[test]
    fn compact_expense_keeps_right_columns_totals_and_actions_visible() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','Long contact details')",
            [],
        )?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 6;
        app.open_accounting_form(0);
        if let Some(Form::Expense { sheet }) = &mut app.accounting.form {
            let mut doc = sheet.doc.clone();
            doc.vendor = Some(1);
            doc.lines[0].description = "Office supplies".into();
            doc.lines[0].amount = "10.00".into();
            doc.lines[0].tax.inclusive = true; // Existing advanced data must reveal its columns.
            doc.lines = vec![doc.lines[0].clone(); 40];
            *sheet = super::super::expense_ui::ExpenseSheet::new(doc);
        }
        for (width, height) in [
            (900.0, 650.0),
            (1000.0, 600.0),
            (1200.0, 800.0),
            (1920.0, 1080.0),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
            app.accounting.error = "Validation details. ".repeat(20);
            for _ in 0..5 {
                let viewport_id = egui::ViewportId::from_hash_of("expense_layout_test");
                let mut viewports = egui::ViewportIdMap::default();
                viewports.insert(
                    viewport_id,
                    egui::ViewportInfo {
                        parent: Some(egui::ViewportId::ROOT),
                        ..Default::default()
                    },
                );
                let _ = ctx.run(
                    egui::RawInput {
                        viewport_id,
                        viewports,
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| app.accounting_dialog(ctx),
                );
            }
            for id in [
                "expense_add_rect",
                "expense_total_rect",
                "expense_save_close_rect",
            ] {
                let rect = ctx
                    .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id)))
                    .unwrap();
                assert!(
                    screen.contains_rect(rect),
                    "{id} clipped at {width} x {height}: {rect:?}"
                );
            }
            for column in [6, 7, 8] {
                let rect = ctx
                    .data(|d| {
                        d.get_temp::<egui::Rect>(egui::Id::new((
                            "sheet_cell",
                            "expense_lines",
                            column,
                        )))
                    })
                    .unwrap();
                assert!(
                    screen.contains_rect(rect),
                    "Expense column {column} is clipped: {rect:?}"
                );
            }
        }
        if let Some(Form::Expense { sheet }) = &app.accounting.form {
            assert_eq!(sheet.doc.lines.len(), 40);
            assert_eq!(sheet.doc.lines[0].description, "Office supplies");
            assert_eq!(sheet.doc.lines[0].amount, "10.00");
        } else {
            panic!("Expense draft was lost");
        }
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM expense_documents", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn save_keeps_created_ids_and_failures_keep_expense_draft() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','')",
            [],
        )?;
        let account = accounting::default_account(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let mut form = Form::Account {
            id: 0,
            code: "6999".into(),
            name: "Office".into(),
            kind: "Expense".into(),
            active: true,
        };
        assert!(app.save_accounting_form(&mut form));
        assert!(app.save_accounting_form(&mut form));
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM accounts WHERE code='6999'", [], |r| r
                    .get::<_, i64>(0))?,
            1
        );
        let doc = crate::expense_doc::Document {
            id: 0,
            vendor: Some(1),
            date: crate::today().to_string(),
            currency: "CAD".into(),
            reference: "REF".into(),
            due_date: None,
            memo: String::new(),
            terms: String::new(),
            lines: vec![Expense {
                tax: Default::default(),
                id: 0,
                date: String::new(),
                description: "Office".into(),
                account,
                vendor: None,
                currency: String::new(),
                amount: "10.00".into(),
                task: None,
            }],
        };
        let mut form = Form::Expense {
            sheet: super::super::expense_ui::ExpenseSheet::new(doc),
        };
        assert!(app.save_accounting_form(&mut form));
        assert!(app.save_accounting_form(&mut form));
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM expense_documents", [], |r| r
                    .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM ledger_expenses", [], |r| r
                    .get::<_, i64>(0))?,
            1
        );
        if let Form::Expense { sheet } = &mut form {
            sheet.doc.lines[0].amount = "invalid".into();
        }
        assert!(!app.save_accounting_form(&mut form));
        if let Form::Expense { sheet } = &form {
            assert_eq!(sheet.doc.lines[0].amount, "invalid");
            assert!(sheet.doc.id > 0);
        }
        assert_eq!(
            app.db
                .query_row("SELECT amount FROM ledger_expenses", [], |r| r
                    .get::<_, i64>(0))?,
            1000
        );
        Ok(())
    }
    #[test]
    fn accounting_pages_and_forms_render() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let ctx = egui::Context::default();
        for page in [6, 7] {
            app.page = page;
            let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
            app.open_accounting_form(0);
            let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
            app.accounting.form = None;
        }
        app.page = 7;
        app.open_accounting_form(1);
        assert!(app.accounting.form.is_some());
        Ok(())
    }
}
