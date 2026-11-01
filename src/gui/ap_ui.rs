use super::*;
use crate::ap;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Entry {
    amount: String,
    remaining: i64,
    base_remaining: i64,
}

pub(super) struct PayForm {
    vendor: i64,
    currency: String,
    date: chrono::NaiveDate,
    bank: i64,
    method: String,
    reference: String,
    memo: String,
    entries: BTreeMap<i64, Entry>,
    request: String,
    baseline: String,
    error: String,
    review: Option<ap::Draft>,
    close_after: bool,
    review_focus: bool,
    history: bool,
    reconcile: bool,
    evidence: String,
    bank_settings: bool,
    configure_account: i64,
    configure_currency: String,
    scope: Option<(i64, String)>,
    void: Option<(i64, chrono::NaiveDate, String)>,
    clear_selection: bool,
}
impl PayForm {
    fn readiness(&self) -> Option<String> {
        if self.vendor <= 0 {
            return Some("Select a vendor to continue.".into());
        }
        if self.bank <= 0 {
            return Some("Select a matching-currency bank account.".into());
        }
        if self.entries.is_empty() {
            return Some("Select at least one bill to pay.".into());
        }
        let mut sum = 0i64;
        for entry in self.entries.values() {
            let amount = match crate::parse_money(&entry.amount) {
                Ok(a) if a > 0 && a <= entry.remaining => a,
                _ => {
                    return Some(
                        "Enter a positive payment no greater than each bill's open amount.".into(),
                    )
                }
            };
            let Some(next) = sum.checked_add(amount) else {
                return Some("The payment total is too large.".into());
            };
            sum = next;
        }
        None
    }
    fn fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                self.vendor,
                &self.currency,
                self.date,
                self.bank,
                &self.method,
                &self.reference,
                &self.memo,
                &self.entries
            )
        )
    }
    pub(super) fn dirty(&self) -> bool {
        self.fingerprint() != self.baseline
    }
    fn reset_scope(&mut self, db: &Connection) {
        self.entries.clear();
        self.review = None;
        self.bank=db.query_row("SELECT b.id FROM ap_banks b JOIN accounts a ON a.id=b.id WHERE b.currency=? AND a.active=1 AND a.kind='Asset' ORDER BY a.code LIMIT 1",[&self.currency],|r|r.get(0)).unwrap_or(0);
    }
    fn draft(&self, db: &Connection) -> Result<ap::Draft> {
        let allocations = self
            .entries
            .iter()
            .map(|(id, e)| {
                Ok(ap::Allocation {
                    source: *id,
                    amount: crate::parse_money(&e.amount).map_err(anyhow::Error::msg)?,
                    remaining: e.remaining,
                    base_remaining: e.base_remaining,
                })
            })
            .collect::<Result<_>>()?;
        Ok(ap::Draft {
            request: self.request.clone(),
            vendor: self.vendor,
            currency: self.currency.clone(),
            date: self.date.to_string(),
            bank: self.bank,
            method: self.method.clone(),
            reference: self.reference.clone(),
            memo: self.memo.clone(),
            quote: ap::quote(db, &self.currency, &self.date.to_string())?,
            allocations,
        })
    }
}

impl Desktop {
    pub(super) fn open_pay_bills(&mut self, vendor: i64, document: Option<i64>) {
        if self.reuse_business(windows::Kind::PayBills) {
            return;
        }
        let result = (|| -> Result<PayForm> {
            let currency = if let Some(id) = document {
                self.db.query_row(
                    "SELECT currency FROM expense_documents WHERE id=?",
                    [id],
                    |r| r.get::<_, String>(0),
                )?
            } else if vendor > 0 {
                crate::vendor::load(&self.db, vendor)?.currency
            } else {
                self.settings.currency.clone()
            };
            let mut f = PayForm {
                vendor,
                currency,
                date: crate::today(),
                bank: 0,
                method: "Bank transfer".into(),
                reference: String::new(),
                memo: String::new(),
                entries: BTreeMap::new(),
                request: ap::request_id(&self.db)?,
                baseline: String::new(),
                error: String::new(),
                review: None,
                close_after: false,
                review_focus: false,
                history: false,
                reconcile: false,
                evidence: String::new(),
                bank_settings: false,
                configure_account: 0,
                configure_currency: self.settings.currency.clone(),
                scope: None,
                void: None,
                clear_selection: false,
            };
            f.reset_scope(&self.db);
            if let Some(doc) = document {
                if let Some(b) = ap::bills(&self.db, vendor, &f.currency, &f.date.to_string())?
                    .into_iter()
                    .find(|b| b.document == doc && b.remaining > 0 && b.verified)
                {
                    f.entries.insert(
                        b.source,
                        Entry {
                            amount: crate::format_money(b.remaining),
                            remaining: b.remaining,
                            base_remaining: b.base_remaining,
                        },
                    );
                }
            }
            f.baseline = f.fingerprint();
            Ok(f)
        })();
        match result {
            Ok(f) => self.pay_bills = Some(f),
            Err(e) => {
                self.message = format!("Cannot open Pay Bills: {e:#}");
                self.error = true
            }
        }
    }
    pub(super) fn review_supplier_payment(&mut self, close_after: bool) {
        let Some(mut form) = self.pay_bills.take() else {
            return;
        };
        self.prepare_supplier_review(&mut form, close_after);
        self.pay_bills = Some(form);
    }
    fn prepare_supplier_review(&mut self, form: &mut PayForm, close_after: bool) {
        if !self.prepare_gl_fx(&form.currency, &form.date.to_string()) {
            form.error = self.message.clone();
            return;
        }
        match form.draft(&self.db).and_then(|d| {
            ap::preview(&self.db, &d)?;
            Ok(d)
        }) {
            Ok(d) => {
                form.error.clear();
                form.review = Some(d);
                form.review_focus = true;
                form.close_after = close_after
            }
            Err(e) => form.error = format!("Payment was not recorded: {e:#}"),
        }
    }
    pub(super) fn pay_bills_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.pay_bills.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("pay_bills"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 40.0).max(400.0));
            ui.horizontal(|ui| {
                ui.heading("Pay Bills");
                ui.selectable_value(&mut f.history, false, "Outstanding Bills");
                ui.selectable_value(&mut f.history, true, "Payment History");
                if ui.button("Bank Accounts").clicked() {
                    f.bank_settings = true;
                    f.configure_currency = f.currency.clone()
                }
            });
            ui.separator();
            let vendors = self.rows("SELECT id,name FROM vendors ORDER BY name");
            let old = (f.vendor, f.currency.clone());
            ui.horizontal_wrapped(|ui| {
                ui.label("Vendor");
                picker_with_width(ui, "pay_vendor", &mut f.vendor, &vendors, 210.0);
                ui.label("Currency");
                currency_picker(ui, "pay_currency", &mut f.currency);
                ui.label("Date");
                ui.add(egui_extras::DatePickerButton::new(&mut f.date).id_salt("pay_date"));
            });
            if f.vendor != old.0 {
                if let Ok(v) = crate::vendor::load(&self.db, f.vendor) {
                    f.currency = v.currency
                }
            }
            if old != (f.vendor, f.currency.clone()) {
                if f.entries.is_empty() {
                    f.reset_scope(&self.db)
                } else {
                    f.scope = Some((f.vendor, f.currency.clone()));
                    f.review_focus = true;
                    f.vendor = old.0;
                    f.currency = old.1;
                }
            }
            let banks=(||->rusqlite::Result<Vec<Vec<String>>>{
                let mut stmt=self.db.prepare("SELECT b.id,a.code || ' - ' || a.name FROM ap_banks b JOIN accounts a ON a.id=b.id WHERE a.active=1 AND a.kind='Asset' AND b.currency=? ORDER BY a.code")?;
                let rows=stmt.query_map([&f.currency],|r|Ok(vec![r.get::<_,i64>(0)?.to_string(),r.get::<_,String>(1)?]))?.collect();rows
            })().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                ui.label("Pay from");
                picker_with_width(ui, "pay_bank", &mut f.bank, &banks, 210.0);
                ui.label("Method");
                egui::ComboBox::from_id_salt("pay_method")
                    .selected_text(&f.method)
                    .show_ui(ui, |ui| {
                        for method in ["Bank transfer", "Cheque", "Cash"] {
                            ui.selectable_value(&mut f.method, method.into(), method);
                        }
                    });
                ui.label("Reference");
                ui.add(egui::TextEdit::singleline(&mut f.reference).desired_width(150.0));
            });
            if banks.is_empty() {
                ui.small("Configure a dedicated bank/cash account and its currency before recording payments.");
            }
            ui.horizontal(|ui| {
                ui.label("Memo");
                ui.add(
                    egui::TextEdit::singleline(&mut f.memo)
                        .desired_width(ui.available_width() - 8.0),
                );
            });
            ui.separator();
            let remaining = (ctx.content_rect().bottom() - ui.cursor().min.y - 172.0).max(84.0);
            if f.history {
                self.supplier_payment_history(ui, &mut f, remaining)
            } else {
                let r = ap::reconciliation(&self.db, f.vendor, &f.currency, &f.date.to_string());
                ui.horizontal_wrapped(|ui| {
                    match &r {
                        Ok(r) if r.ready() => {
                            ui.label(format!(
                                "Open balance: {} {}",
                                f.currency,
                                crate::format_money(r.open)
                            ));
                        }
                        Ok(_) => {
                            ui.colored_label(Color32::DARK_RED, "Needs reconciliation");
                        }
                        Err(e) => {
                            ui.colored_label(Color32::DARK_RED, format!("{e:#}"));
                        }
                    }
                    if ui.button("Review A/P").clicked() {
                        f.reconcile = true
                    }
                    if ui.button("View General Ledger").clicked() {
                        self.windows.ledger = true;
                        self.focus_business(windows::Kind::Ledger)
                    }
                });
                self.supplier_allocation_grid(ui, &mut f, (remaining - 32.0).max(70.0));
            }
            ui.horizontal_wrapped(|ui| {
                let sum = f.entries.values().try_fold(0i64, |sum, e| {
                    crate::parse_money(&e.amount)
                        .ok()
                        .and_then(|a| sum.checked_add(a))
                });
                ui.strong(format!(
                    "PAYMENT  {} {}",
                    f.currency,
                    sum.map(crate::format_money)
                        .unwrap_or_else(|| "Invalid amount".into())
                ));
                if let Ok(q) = ap::quote(&self.db, &f.currency, &f.date.to_string()) {
                    ui.small(format!(
                        "1 {} = {:.6} {} | {} {}",
                        f.currency,
                        q.rate as f64 / crate::FX_SCALE as f64,
                        q.target,
                        q.provider,
                        q.effective_date
                    ));
                } else if f.vendor > 0 {
                    self.prepare_gl_fx(&f.currency, &f.date.to_string());
                    ui.small("Looking up the missing historical exchange rate...");
                }
            });
            if !f.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("pay_error")
                    .max_height(36.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::DARK_RED, &f.error);
                    });
            }
            let reason = f.readiness();
            controls::footer(ui, reason.as_deref(), |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !f.entries.is_empty(),
                            egui::Button::new("Clear allocations"),
                        )
                        .clicked()
                    {
                        f.clear_selection = true;
                        f.review_focus = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let record = controls::primary(ui, "Review Payment", reason.is_none());
                        #[cfg(test)]
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("supplier_record_rect"), record.rect)
                        });
                        if record.clicked() {
                            self.prepare_supplier_review(&mut f, false);
                        }
                        if ctx.viewport_id() == egui::ViewportId::ROOT
                            && ui.button("Cancel").clicked()
                        {
                            self.request_business_close(windows::Kind::PayBills);
                        }
                    });
                });
            });
        });
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.request_business_close(windows::Kind::PayBills)
        }
        if f.clear_selection {
            match controls::review(ctx,controls::Review {cancel:"Keep Selection",id:"supplier_clear_allocations",title:"Clear Payment Allocations?",values:&[("Selected bills",f.entries.len().to_string())],error:None,confirm:"Clear Allocations",notice:"Clears selected bills and entered payment amounts only. Vendor, bank account, reference and memo are retained."},&mut f.review_focus) {
                Some(controls::ReviewAction::Keep)=>f.clear_selection=false,
                Some(controls::ReviewAction::Confirm)=>{f.entries.clear();f.review=None;f.error.clear();f.clear_selection=false;},
                None=>{}
            }
        }
        if f.scope.is_some() {
            match controls::review(ctx,controls::Review { cancel: "Keep Selection",id:"payment_scope",title:"Change Payment Scope?",values:&[],error:None,confirm:"Clear Allocations & Continue",notice:"Changing vendor or currency clears allocations. Reference and memo remain in the draft."},&mut f.review_focus) {
                Some(controls::ReviewAction::Keep)=>f.scope=None,
                Some(controls::ReviewAction::Confirm)=>if let Some((vendor,currency))=f.scope.take() {f.vendor=vendor;f.currency=currency;f.reset_scope(&self.db)},
                None=>{}
            }
        }
        if f.bank_settings {
            self.supplier_bank_settings(ctx, &mut f)
        }
        if f.reconcile {
            self.supplier_reconciliation(ctx, &mut f)
        }
        if let Some(draft) = f.review.clone() {
            let preview = ap::preview(&self.db, &draft);
            let error = preview.as_ref().err().map(|e| e.to_string());
            let mut values = vec![
                (
                    "Supplier",
                    crate::vendor::load(&self.db, draft.vendor)
                        .map(|v| v.name)
                        .unwrap_or_default(),
                ),
                ("Date", draft.date.clone()),
                (
                    "Bank account",
                    self.db
                        .query_row(
                            "SELECT code || ' - ' || name FROM accounts WHERE id=?",
                            [draft.bank],
                            |r| r.get::<_, String>(0),
                        )
                        .unwrap_or_else(|_| "Unavailable".into()),
                ),
                ("Reference", draft.reference.clone()),
            ];
            if let Ok(p) = &preview {
                values.push((
                    "Payment",
                    format!(
                        "{} {} / {} bills",
                        draft.currency,
                        crate::format_money(p.amount),
                        draft.allocations.len()
                    ),
                ));
                values.push((
                    "A/P released",
                    format!("{} {}", draft.quote.target, crate::format_money(p.released)),
                ));
                values.push((
                    "Bank base value",
                    format!(
                        "{} {}",
                        draft.quote.target,
                        crate::format_money(p.bank_base)
                    ),
                ));
                values.push((
                    "Realized FX",
                    format!("{} {}", draft.quote.target, crate::format_money(p.fx)),
                ));
            }
            match people_ui::settlement_review(
                ctx,
                "supplier_payment_review",
                "Review Supplier Payment",
                &values,
                error.as_deref(),
                &mut f.review_focus,
            ) {
                Some(people_ui::ReviewAction::Keep) => f.review = None,
                Some(people_ui::ReviewAction::Record) => match ap::record(&mut self.db, &draft) {
                    Ok(id) => {
                        self.message = format!("Supplier payment #{id} recorded.");
                        self.error = false;
                        f.review = None;
                        f.entries.clear();
                        f.reference.clear();
                        f.memo.clear();
                        f.request = ap::request_id(&self.db).unwrap_or_default();
                        f.baseline = f.fingerprint();
                        close = f.close_after;
                    }
                    Err(e) => {
                        f.review = None;
                        f.error = format!("Payment was not recorded: {e:#}");
                    }
                },
                None => {}
            }
        }
        if f.void.is_some() {
            self.supplier_void_dialog(ctx, &mut f)
        }
        if !close {
            self.pay_bills = Some(f)
        }
    }
    fn supplier_allocation_grid(&mut self, ui: &mut egui::Ui, f: &mut PayForm, height: f32) {
        let rows = match ap::bills(&self.db, f.vendor, &f.currency, &f.date.to_string()) {
            Ok(r) => r
                .into_iter()
                .filter(|b| b.remaining > 0)
                .collect::<Vec<_>>(),
            Err(e) => {
                f.error = format!("{e:#}");
                Vec::new()
            }
        };
        let width = invoice_sheet::finite_width(ui).max(790.0);
        invoice_sheet::sheet_columns(
            ui,
            "supplier_allocations",
            height,
            &[
                "",
                "BILL",
                "DATE",
                "DUE",
                "REFERENCE",
                "ORIGINAL",
                "OPEN",
                "PAYMENT",
            ],
            &[26.0, 84.0, 92.0, 92.0, width - 618.0, 100.0, 104.0, 120.0],
            rows.len(),
            |ui, index, column| {
                let b = &rows[index];
                match column {
                    0 => {
                        let mut checked = f.entries.contains_key(&b.source);
                        if ui
                            .add_enabled(b.verified, egui::Checkbox::without_text(&mut checked))
                            .changed()
                        {
                            if checked {
                                f.entries.insert(
                                    b.source,
                                    Entry {
                                        amount: crate::format_money(b.remaining),
                                        remaining: b.remaining,
                                        base_remaining: b.base_remaining,
                                    },
                                );
                            } else {
                                f.entries.remove(&b.source);
                            }
                        }
                    }
                    1 => {
                        if ui.button(&b.number).on_hover_text("Open bill").clicked() {
                            self.open_accounting_form(b.document)
                        }
                    }
                    2 => {
                        ui.label(&b.date);
                    }
                    3 => {
                        ui.label(&b.due);
                    }
                    4 => {
                        ui.add(egui::Label::new(&b.reference).truncate())
                            .on_hover_text(&b.reference);
                    }
                    5 => invoice_sheet::amount(ui, &crate::format_money(b.gross)),
                    6 => invoice_sheet::amount(ui, &crate::format_money(b.remaining)),
                    _ => {
                        if let Some(e) = f.entries.get_mut(&b.source) {
                            ui.add(
                                egui::TextEdit::singleline(&mut e.amount)
                                    .desired_width(110.0)
                                    .horizontal_align(egui::Align::RIGHT),
                            );
                        } else {
                            ui.small(if b.verified {
                                "Select to pay"
                            } else {
                                "Review required"
                            });
                        }
                    }
                }
            },
        );
        if rows.is_empty() {
            ui.small("No outstanding bills in this vendor, currency and date scope.");
        }
    }
    fn supplier_bank_settings(&mut self, ctx: &egui::Context, f: &mut PayForm) {
        egui::Modal::new(egui::Id::new("supplier_banks")).show(ctx,|ui|{
            ui.heading("Bank / Cash Accounts");ui.label("Choose a dedicated Asset account and explicitly confirm its currency.");
            let accounts=self.rows("SELECT id,code || ' - ' || name FROM accounts WHERE active=1 AND kind='Asset' ORDER BY code");
            picker_with_width(ui,"supplier_bank_config",&mut f.configure_account,&accounts,300.0);currency_picker(ui,"supplier_bank_config_currency",&mut f.configure_currency);
            ui.small("A used bank currency is fixed. Accounts with postings in another currency cannot be configured.");
            ui.horizontal(|ui|{
                if ui.button("Return").clicked(){f.bank_settings=false}
                if ui.button("Save Bank Currency").clicked(){match ap::configure_bank(&mut self.db,f.configure_account,&f.configure_currency){Ok(())=>{if f.configure_currency==f.currency{f.bank=f.configure_account}f.bank_settings=false;f.error.clear()},Err(e)=>f.error=format!("{e:#}")}}
            });
            if !f.error.is_empty(){ui.colored_label(Color32::DARK_RED,&f.error);}
        });
    }
    fn supplier_reconciliation(&mut self, ctx: &egui::Context, f: &mut PayForm) {
        egui::Modal::new(egui::Id::new("supplier_ap_review")).show(ctx,|ui|{
            ui.heading("Review Supplier A/P");ui.label(format!("Cutoff: {} | {}",f.date,f.currency));
            match ap::reconciliation(&self.db,f.vendor,&f.currency,&f.date.to_string()){
                Ok(r)=>{
                    ui.label(format!("Open-item schedule: {} {} | Base {}",f.currency,crate::format_money(r.open),crate::format_money(r.base_open)));
                    ui.label(format!("Posted ledger: {} {} | Base {}",f.currency,crate::format_money(r.ledger),crate::format_money(r.base_ledger)));
                    ui.label(format!("Unreviewed bills: {} | Missing posting sources: {} | Unassigned A/P lines: {}",r.unverified,r.missing,r.unassigned));
                    ui.label("Compare supplier statements and historical settlements before approving. Differences need ledger/source correction; approval does not create a balancing entry.");
                    ui.add(egui::TextEdit::singleline(&mut f.evidence).hint_text("Statement reference / review evidence").desired_width(420.0));
                    ui.horizontal(|ui|{
                        if ui.button("Keep Reviewing").clicked(){f.reconcile=false}
                        if ui.add_enabled(f.vendor>0&&r.balanced()&&!f.evidence.trim().is_empty(),egui::Button::new("Confirm Reconciled Items")).clicked(){match ap::review(&mut self.db,f.vendor,&f.currency,&f.date.to_string(),&f.evidence){Ok(())=>{f.reconcile=false;f.error.clear()},Err(e)=>f.error=format!("{e:#}")}}
                    });
                },Err(e)=>{ui.colored_label(Color32::DARK_RED,format!("{e:#}"));if ui.button("Return").clicked(){f.reconcile=false}}
            }
        });
    }
    fn supplier_payment_history(&mut self, ui: &mut egui::Ui, f: &mut PayForm, height: f32) {
        let rows=self.rows(&format!("SELECT p.id,p.number,p.date,p.currency,p.amount,COALESCE(v.date,''),p.reference FROM vendor_payments p LEFT JOIN ap_voids v ON v.payment=p.id WHERE p.vendor={} OR {}=0 ORDER BY p.date DESC,p.id DESC",f.vendor,f.vendor));
        let width = invoice_sheet::finite_width(ui).max(760.0);
        invoice_sheet::sheet_columns(
            ui,
            "supplier_payment_history",
            height,
            &[
                "NUMBER",
                "DATE",
                "CURR.",
                "AMOUNT",
                "REFERENCE",
                "STATUS",
                "",
            ],
            &[95.0, 95.0, 60.0, 115.0, width - 555.0, 105.0, 85.0],
            rows.len(),
            |ui, index, column| {
                let r = &rows[index];
                let id = r[0].parse().unwrap_or(0);
                match column {
                    0 => {
                        if ui
                            .button(&r[1])
                            .on_hover_text("View payment journal")
                            .clicked()
                        {
                            if let Ok(entry)=self.db.query_row("SELECT entry FROM gl_sources WHERE kind='vendor_payment' AND record=?",[id],|r|r.get::<_,i64>(0)){self.open_journal(entry)}
                        }
                    }
                    1 => {
                        ui.label(&r[2]);
                    }
                    2 => {
                        ui.label(&r[3]);
                    }
                    3 => invoice_sheet::amount(ui, &crate::format_money(r[4].parse().unwrap_or(0))),
                    4 => {
                        ui.add(egui::Label::new(&r[6]).truncate());
                    }
                    5 => {
                        ui.label(if r[5].is_empty() {
                            "Recorded".into()
                        } else {
                            format!("Void {}", r[5])
                        });
                    }
                    _ => {
                        if r[5].is_empty() && ui.button("Void").clicked() {
                            f.void = Some((id, crate::today(), String::new()));
                            f.review_focus = true;
                            f.error.clear();
                        }
                    }
                }
            },
        );
    }
    fn supplier_void_dialog(&mut self, ctx: &egui::Context, f: &mut PayForm) {
        let Some((id, mut date, mut reason)) = f.void.take() else {
            return;
        };
        let error = if reason.trim().is_empty() {
            Some("A reason is required.")
        } else if !f.error.is_empty() {
            Some(f.error.as_str())
        } else {
            None
        };
        let values = [("Payment", id.to_string())];
        let before = (date, reason.clone());
        let action=controls::review_content(ctx,controls::Review{ cancel: "Keep Editing",id:"supplier_void",title:"Void Supplier Payment",values:&values,error,confirm:"Confirm & Void",notice:"The original payment remains. Exact source/base amounts reverse on the selected date."},&mut f.review_focus,|ui| {
            ui.label("Reversal date");ui.add(egui_extras::DatePickerButton::new(&mut date).id_salt("supplier_void_date"));
            ui.label("Reason *");ui.add(egui::TextEdit::multiline(&mut reason).desired_rows(2).desired_width(f32::INFINITY));
        });
        if before != (date, reason.clone()) {
            f.error.clear();
        }
        let mut keep = true;
        match action {
            Some(controls::ReviewAction::Keep) => keep = false,
            Some(controls::ReviewAction::Confirm) => {
                match ap::void(&mut self.db, id, &date.to_string(), &reason) {
                    Ok(()) => {
                        keep = false;
                        f.error.clear();
                        self.message = "Supplier payment voided; allocated bills reopened.".into()
                    }
                    Err(e) => f.error = format!("{e:#}"),
                }
            }
            None => {}
        }
        if keep {
            f.void = Some((id, date, reason))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_allocations_requires_confirmation_and_keeps_payment_details() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO vendors(id,name,contact) VALUES(1,'Supplier','')",
            [],
        )?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_pay_bills(1, None);
        let form = app.pay_bills.as_mut().unwrap();
        form.reference = "Retain reference".into();
        form.memo = "Retain memo".into();
        form.entries.insert(
            42,
            Entry {
                amount: "25.00".into(),
                remaining: 2500,
                base_remaining: 2500,
            },
        );
        form.clear_selection = true;
        form.review_focus = true;
        let ctx = egui::Context::default();
        let frame = |app: &mut Desktop, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.pay_bills_dialog(ctx),
            );
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        assert_eq!(app.pay_bills.as_ref().unwrap().entries.len(), 1);
        let pos = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::new((
                    "supplier_clear_allocations",
                    "review_confirm",
                )))
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
        let form = app.pay_bills.as_ref().unwrap();
        assert!(form.entries.is_empty());
        assert_eq!(form.reference, "Retain reference");
        assert_eq!(form.memo, "Retain memo");
        assert_eq!(form.vendor, 1);
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM vendor_payments", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn payment_window_retains_drafts_requires_review_and_keeps_actions_visible() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let vendor = crate::vendor::save(
            &mut db,
            &crate::vendor::Vendor {
                name: "Payable supplier".into(),
                ..Default::default()
            },
        )?;
        let bank = crate::gl::role(&db, "bank")?;
        ap::configure_bank(&mut db, bank, "CAD")?;
        let mut app = Desktop::new(db, PathBuf::from("ap-ui-test.db"))?;
        app.open_pay_bills(vendor, None);
        let request = app.pay_bills.as_ref().unwrap().request.clone();
        app.pay_bills.as_mut().unwrap().reference = "Keep this input".into();
        app.open_pay_bills(999, None);
        assert_eq!(app.pay_bills.as_ref().unwrap().request, request);
        assert_eq!(app.pay_bills.as_ref().unwrap().vendor, vendor);
        app.review_supplier_payment(true);
        assert!(app.pay_bills.as_ref().unwrap().review.is_none());
        assert!(!app.pay_bills.as_ref().unwrap().error.is_empty());
        let changes = app.db.total_changes();
        for (width, height) in [(900.0, 650.0), (1200.0, 800.0), (1600.0, 1000.0)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let viewport = egui::ViewportId::from_hash_of("supplier-payment-layout");
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
            for _ in 0..4 {
                let mut viewports = egui::ViewportIdMap::default();
                viewports.insert(
                    viewport,
                    egui::ViewportInfo {
                        parent: Some(egui::ViewportId::ROOT),
                        ..Default::default()
                    },
                );
                let _ = ctx.run(
                    egui::RawInput {
                        viewport_id: viewport,
                        viewports,
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| app.pay_bills_dialog(ctx),
                );
            }
            let record = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("supplier_record_rect")))
                .unwrap();
            assert!(
                screen.contains_rect(record),
                "Record Payment clipped at {width}x{height}: {record:?}"
            );
            let grid = ctx
                .data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new((
                        "sheet_rows_viewport",
                        "supplier_allocations",
                    )))
                })
                .unwrap();
            assert!(
                grid.bottom() < record.top(),
                "The allocation grid must leave room for fixed actions."
            );
        }
        assert_eq!(app.db.total_changes(), changes);
        assert!(app.pay_bills.as_ref().unwrap().dirty());
        assert_eq!(app.pay_bills.as_ref().unwrap().reference, "Keep this input");
        app.reset_after_restore()?;
        assert!(app.pay_bills.is_none());
        Ok(())
    }
}
