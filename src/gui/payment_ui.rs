use super::*;

pub(super) struct PaymentForm {
    invoice: i64,
    number: String,
    customer: String,
    currency: String,
    date: chrono::NaiveDate,
    amount: String,
    method: String,
    reference: String,
    error: String,
    saved: bool,
    selected: bool,
    request: String,
    baseline: String,
    review: Option<crate::receipts::Draft>,
    review_focus: bool,
    reset_guard: controls::CloseGuard,
}
impl PaymentForm {
    fn readiness(&self, c: &Connection) -> Option<String> {
        if !self.selected {
            return Some("Select the invoice to apply this payment.".into());
        }
        let balance = match crate::billing::balance(c, self.invoice) {
            Ok(b) => b,
            Err(e) => return Some(format!("Invoice balance unavailable: {e}")),
        };
        if balance.remaining <= 0 {
            return Some("This invoice has no outstanding balance.".into());
        }
        match crate::parse_money(&self.amount) {
            Ok(n) if n > balance.remaining => {
                Some("Payment exceeds the amount due. Enter a smaller amount.".into())
            }
            Ok(n) if n > 0 => None,
            _ => Some("Enter a positive payment amount.".into()),
        }
    }
    fn fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                &self.currency,
                self.date,
                &self.amount,
                &self.method,
                &self.reference,
                self.selected
            )
        )
    }
    pub(super) fn dirty(&self) -> bool {
        self.fingerprint() != self.baseline
    }

    fn clear_input(&mut self) {
        self.amount.clear();
        self.reference.clear();
        self.error.clear();
        self.date = crate::today();
        self.method = "Bank transfer".into();
        self.selected = false;
    }
}

impl Desktop {
    pub(super) fn open_payment(&mut self, invoice: i64) {
        if self.reuse_business(windows::Kind::Payment) {
            return;
        }
        let result = (|| -> Result<PaymentForm> {
            let balance = crate::billing::balance(&self.db, invoice)?;
            anyhow::ensure!(
                balance.remaining > 0,
                "This invoice has no outstanding balance."
            );
            let (number, snapshot): (String, String) = self.db.query_row(
                "SELECT number,snapshot FROM invoices WHERE id=?",
                [invoice],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let snapshot: serde_json::Value = serde_json::from_str(&snapshot)?;
            Ok(PaymentForm {
                invoice,
                number,
                customer: snapshot["customer"].as_str().unwrap_or("").into(),
                currency: balance.currency,
                date: crate::today(),
                amount: crate::format_money(balance.remaining),
                method: snapshot["customer_id"]
                    .as_i64()
                    .and_then(|id| crate::customer::load(&self.db, id).ok())
                    .map(|customer| customer.profile.payment_method)
                    .filter(|method| !method.is_empty())
                    .unwrap_or_else(|| "Bank transfer".into()),
                reference: String::new(),
                error: String::new(),
                saved: false,
                selected: true,
                request: crate::posting::request_id(&self.db)?,
                baseline: String::new(),
                review: None,
                review_focus: false,
                reset_guard: Default::default(),
            })
        })();
        match result {
            Ok(mut form) => {
                form.baseline = form.fingerprint();
                self.payment = Some(form)
            }
            Err(error) => {
                self.message = format!("Cannot receive payment: {error:#}");
                self.error = true;
            }
        }
    }

    fn save_payment_form(&mut self, form: &mut PaymentForm) -> bool {
        if form.saved {
            return true;
        }
        let result = (|| -> Result<()> {
            anyhow::ensure!(
                form.selected,
                "Select the invoice before saving the payment."
            );
            let balance = crate::billing::balance(&self.db, form.invoice)?;
            anyhow::ensure!(
                balance.currency == form.currency,
                "The invoice currency has changed. Reopen the payment window."
            );
            let amount = crate::parse_money(&form.amount)?;
            anyhow::ensure!(
                amount > 0 && amount <= balance.remaining,
                "Enter an amount greater than zero and no more than the outstanding balance."
            );
            anyhow::ensure!(
                self.prepare_gl_fx(&form.currency, &form.date.to_string()),
                "{}",
                self.message
            );
            let draft = if let Some(d) = &form.review {
                d.clone()
            } else {
                crate::receipts::prepare(
                    &self.db,
                    &form.request,
                    form.invoice,
                    &form.date.to_string(),
                    &form.amount,
                    &form.method,
                    &form.reference,
                )?
            };
            crate::receipts::record(&mut self.db, &draft)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                form.saved = true;
                form.error.clear();
                self.message = format!("Payment recorded for invoice {}.", form.number);
                self.error = false;
                true
            }
            Err(error) => {
                form.error = format!("Payment was not saved: {error:#}");
                false
            }
        }
    }

    fn prepare_receipt_review(&mut self, form: &mut PaymentForm) {
        if !form.selected {
            form.error = "Select the invoice before recording a receipt.".into();
            return;
        }
        if !self.prepare_gl_fx(&form.currency, &form.date.to_string()) {
            form.error = self.message.clone();
            return;
        }
        match crate::receipts::prepare(
            &self.db,
            &form.request,
            form.invoice,
            &form.date.to_string(),
            &form.amount,
            &form.method,
            &form.reference,
        )
        .and_then(|d| {
            crate::receipts::validate_draft(&self.db, &d)?;
            Ok(d)
        }) {
            Ok(d) => {
                form.review = Some(d);
                form.review_focus = true;
                form.error.clear()
            }
            Err(e) => form.error = format!("Receipt was not recorded: {e:#}"),
        }
    }
    pub(super) fn review_receipt_close(&mut self) {
        if let Some(mut form) = self.payment.take() {
            self.prepare_receipt_review(&mut form);
            self.payment = Some(form)
        }
    }
    pub(super) fn payment_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.payment.take() else {
            return;
        };
        let mut saved = false;
        let response = windows::surface(ctx, egui::Id::new("receive_payment"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 48.0).max(300.0));
            ui.heading("Customer Payment");
            ui.small(if form.dirty() {
                "Unsaved changes"
            } else {
                "New receipt - not recorded"
            });
            let body_height = (ctx.content_rect().height() - 185.0).max(150.0);
            egui::ScrollArea::vertical()
                .id_salt("receive_payment_body")
                .min_scrolled_height(body_height)
                .max_height(body_height)
                .auto_shrink([false,false])
                .show(ui, |ui| {
                    let balance = crate::billing::balance(&self.db, form.invoice);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("RECEIVED FROM");
                        ui.strong(&form.customer);
                        ui.add_space(16.0);
                        ui.label("INVOICE BALANCE");
                        ui.strong(format!(
                            "{} {}",
                            form.currency,
                            balance
                                .as_ref()
                                .map(|b| crate::format_money(b.remaining))
                                .unwrap_or_else(|_| "Unavailable".into())
                        ));
                    });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("PAYMENT AMOUNT");
                        ui.label(&form.currency);
                        let amount = ui.add(
                            egui::TextEdit::singleline(&mut form.amount)
                                .desired_width(140.0)
                                .horizontal_align(egui::Align::RIGHT),
                        );
                        #[cfg(test)]
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("receive_amount_rect"), amount.rect)
                        });
                        #[cfg(not(test))]
                        let _ = amount;
                        ui.label("DATE");
                        ui.add(
                            egui_extras::DatePickerButton::new(&mut form.date)
                                .id_salt("receive_payment_date"),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("PMT. METHOD");
                        egui::ComboBox::from_id_salt("receive_payment_method")
                            .width(140.0)
                            .selected_text(&form.method)
                            .show_ui(ui, |ui| {
                                for method in
                                    ["Bank transfer", "Cash", "Cheque", "Credit card", "Other"]
                                {
                                    ui.selectable_value(&mut form.method, method.into(), method);
                                }
                            });
                        ui.label("REFERENCE / MEMO");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.reference).desired_width(180.0),
                        );
                    });
                    ui.collapsing("Posting accounts and exchange rate", |ui| {
                        for role in ["bank", "receivable"] {
                            let account = if role == "receivable" {
                                self.db.query_row("SELECT l.account FROM gl_sources s JOIN gl_lines l ON l.entry=s.entry WHERE s.kind='invoice' AND s.record=? ORDER BY l.position LIMIT 1", [form.invoice], |r| r.get::<_, i64>(0)).ok()
                                    .or_else(|| crate::gl::role(&self.db, role).ok())
                            } else {
                                crate::gl::role(&self.db, role).ok()
                            };
                            let account = account
                                .and_then(|id| {
                                    self.db
                                        .query_row(
                                            "SELECT code || ' - ' || name FROM accounts WHERE id=?",
                                            [id],
                                            |r| r.get::<_, String>(0),
                                        )
                                        .ok()
                                })
                                .unwrap_or_else(|| "Not configured".into());
                            ui.label(format!(
                                "{}: {account}",
                                if role == "bank" {
                                    "Deposit to"
                                } else {
                                    "A/R account"
                                }
                            ));
                        }
                        if let (Ok(base), Ok(rate)) = (
                            crate::gl::base(&self.db),
                            crate::gl::quote(&self.db, &form.currency, &form.date.to_string()),
                        ) {
                            ui.label(format!(
                                "Exchange rate: 1 {} = {}.{:08} {}",
                                form.currency,
                                rate / crate::FX_SCALE,
                                rate % crate::FX_SCALE,
                                base
                            ));
                        } else {
                            ui.label("The applicable exchange rate will be retrieved when saving.");
                        }
                    });
                    if let Ok(balance) = &balance {
                        let width = invoice_sheet::finite_width(ui).max(560.0);
                        let columns = [32.0, 100.0, width - 462.0, 110.0, 110.0, 110.0];
                        let raw: String = self
                            .db
                            .query_row(
                                "SELECT snapshot FROM invoices WHERE id=?",
                                [form.invoice],
                                |r| r.get(0),
                            )
                            .unwrap_or_default();
                        let snapshot: serde_json::Value =
                            serde_json::from_str(&raw).unwrap_or_default();
                        let amount = crate::parse_money(&form.amount).ok();
                        invoice_sheet::register_columns(
                            ui,
                            "payment_allocation",
                            84.0,
                            &["", "DATE", "NUMBER", "ORIG. AMT.", "AMT. DUE", "PAYMENT"],
                            &columns,
                            1,
                            |ui, _, column| match column {
                                0 => {
                                    ui.checkbox(&mut form.selected, "");
                                }
                                1 => {
                                    ui.label(
                                        snapshot["invoice_date"].as_str().unwrap_or("Unknown"),
                                    );
                                }
                                2 => {
                                    ui.label(&form.number);
                                }
                                3 => invoice_sheet::amount(ui, &crate::format_money(balance.total)),
                                4 => invoice_sheet::amount(
                                    ui,
                                    &crate::format_money(balance.remaining),
                                ),
                                _ => invoice_sheet::amount(
                                    ui,
                                    &if form.selected {
                                        amount
                                            .map(crate::format_money)
                                            .unwrap_or_else(|| "--".into())
                                    } else {
                                        "0.00".into()
                                    },
                                ),
                            },
                        );
                        let applied=if form.selected {amount.filter(|a|*a>0&&*a<=balance.remaining)} else {Some(0)};
                        invoice_sheet::money_summary(ui,"PAYMENT SUMMARY",&form.currency,&[("Amount due",Some(balance.remaining)),("Applied",applied),("Remaining after payment",applied.map(|a|balance.remaining-a))]);
                    } else if let Err(error) = &balance {
                        ui.colored_label(Color32::DARK_RED, error.to_string());
                    }
                    if !form.error.is_empty() {
                        egui::ScrollArea::vertical()
                            .id_salt("receive_payment_error")
                            .max_height(36.0)
                            .show(ui, |ui| {
                                ui.colored_label(Color32::DARK_RED, &form.error);
                            });
                    }
                });
            let reason = form.readiness(&self.db);
            controls::footer(ui, reason.as_deref(), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Reset payment").clicked() {
                        form.reset_guard.request();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save = controls::primary(ui, "Review Receipt", reason.is_none());
                        #[cfg(test)]
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("receive_save_rect"), save.rect)
                        });
                        if save.clicked() {
                            self.prepare_receipt_review(&mut form);
                        }
                        if ctx.viewport_id() == egui::ViewportId::ROOT
                            && ui.button("Cancel").clicked()
                        {
                            self.request_business_close(windows::Kind::Payment);
                        }
                    });
                });
            });
        });
        let dirty = form.dirty();
        if form.reset_guard.show(ctx, "payment_reset", dirty) {
            form.clear_input();
            form.review = None;
        }
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.request_business_close(windows::Kind::Payment);
        }
        if let Some(draft) = form.review.clone() {
            let error = crate::receipts::validate_draft(&self.db, &draft)
                .err()
                .map(|e| e.to_string());
            let values = [
                ("Customer", form.customer.clone()),
                ("Invoice", form.number.clone()),
                (
                    "Amount",
                    format!("{} {}", draft.currency, crate::format_money(draft.amount)),
                ),
                ("Date", draft.date.clone()),
                (
                    "Deposit account",
                    self.db
                        .query_row(
                            "SELECT code || ' - ' || name FROM accounts WHERE id=?",
                            [draft.bank],
                            |r| r.get::<_, String>(0),
                        )
                        .unwrap_or_else(|_| "Unavailable".into()),
                ),
                ("Method", draft.method.clone()),
                ("Reference", draft.reference.clone()),
            ];
            match people_ui::settlement_review(
                ctx,
                "receipt_review",
                "Review Customer Receipt",
                &values,
                error.as_deref(),
                &mut form.review_focus,
            ) {
                Some(people_ui::ReviewAction::Keep) => form.review = None,
                Some(people_ui::ReviewAction::Record) => saved = self.save_payment_form(&mut form),
                None => {}
            }
        }
        if !saved {
            self.payment = Some(form);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clearing_or_deselecting_allocation_cannot_record_a_receipt() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        app.open_payment(invoice);
        let mut form = app.payment.take().unwrap();
        form.selected = false;
        assert!(!app.save_payment_form(&mut form));
        form.reference = "Unsaved memo".into();
        form.clear_input();
        assert!(!form.selected);
        assert!(form.amount.is_empty());
        assert!(form.reference.is_empty());
        assert_eq!(form.invoice, invoice);
        assert_eq!(form.customer, "Customer");
        assert!(!app.save_payment_form(&mut form));
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }

    fn fixture() -> Result<(Desktop, i64)> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','ea',0,10000,'CAD');")?;
        crate::add_task(&db, "Task", 1, "CAD")?;
        crate::add_item(&db, 1, 1, 1, None, None)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        crate::set_task_status(&mut app.db, 1, false)?;
        let invoice = crate::billing::merge(&mut app.db, &[1])?;
        Ok((app, invoice))
    }
    #[test]
    fn invalid_payment_explains_why_and_reset_requires_explicit_discard() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        app.open_payment(invoice);
        let mut form = app.payment.take().unwrap();
        form.amount = "100000.00".into();
        assert!(form.readiness(&app.db).unwrap().contains("exceeds"));
        form.amount = "25.00".into();
        form.reference = "Keep reference".into();
        assert!(form.readiness(&app.db).is_none());
        form.reset_guard.request();
        app.payment = Some(form);
        let ctx = egui::Context::default();
        let frame = |app: &mut Desktop, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.payment_dialog(ctx),
            );
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        assert_eq!(app.payment.as_ref().unwrap().reference, "Keep reference");
        let pos = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("payment_reset", "review_confirm"))))
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
        let form = app.payment.as_ref().unwrap();
        assert!(form.amount.is_empty());
        assert!(form.reference.is_empty());
        assert!(form.readiness(&app.db).is_some());
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn receipt_review_requires_confirmation_and_stale_review_preserves_input() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        app.open_payment(invoice);
        let mut form = app.payment.take().unwrap();
        form.amount = "25.00".into();
        form.reference = "Keep reference".into();
        let changes = app.db.total_changes();
        app.prepare_receipt_review(&mut form);
        assert!(form.review.is_some());
        assert_eq!(
            app.db.total_changes(),
            changes,
            "Opening a review must not post"
        );
        crate::billing::add_payment(
            &mut app.db,
            invoice,
            &crate::today().to_string(),
            "10",
            "Cash",
            "",
        )?;
        assert!(!app.save_payment_form(&mut form));
        assert_eq!(form.amount, "25.00");
        assert_eq!(form.reference, "Keep reference");
        assert_eq!(crate::billing::balance(&app.db, invoice)?.received, 1000);
        app.prepare_receipt_review(&mut form);
        assert!(app.save_payment_form(&mut form), "{}", form.error);
        assert_eq!(crate::billing::balance(&app.db, invoice)?.received, 3500);
        Ok(())
    }
    #[test]
    fn receipt_uses_customer_preferred_method() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        let mut customer = crate::customer::load(&app.db, 1)?;
        customer.profile.payment_method = "Cheque".into();
        crate::customer::save(&mut app.db, &customer)?;
        app.open_payment(invoice);
        assert_eq!(app.payment.as_ref().unwrap().method, "Cheque");
        Ok(())
    }

    #[test]
    fn payment_window_preserves_draft_and_partial_receipt_is_not_submitted_twice() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        let changes = app.db.total_changes();
        app.open_payment(invoice);
        assert_eq!(app.db.total_changes(), changes);
        let form = app.payment.as_mut().unwrap();
        assert_eq!(form.amount, "100.00");
        assert_eq!(form.currency, "CAD");
        assert_eq!(form.customer, "Customer");
        form.amount = "invalid".into();
        form.reference = "Keep this reference".into();
        app.open_payment(999999);
        let mut form = app.payment.take().unwrap();
        assert_eq!(form.invoice, invoice);
        assert!(!app.save_payment_form(&mut form));
        assert_eq!(form.amount, "invalid");
        assert_eq!(form.reference, "Keep this reference");
        form.amount = "25.00".into();
        assert!(app.save_payment_form(&mut form), "{}", form.error);
        assert!(app.save_payment_form(&mut form));
        assert_eq!(crate::billing::balance(&app.db, invoice)?.remaining, 7500);
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            1
        );
        crate::storage::validate_projections(&app.db)?;
        Ok(())
    }

    #[test]
    fn payment_save_rechecks_balance_and_rejects_locked_period_receipts() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        app.open_payment(invoice);
        let mut form = app.payment.take().unwrap();
        form.date -= chrono::Duration::days(1);
        app.db.execute(
            "UPDATE gl_settings SET locked_through=? WHERE id=1",
            [form.date.to_string()],
        )?;
        assert!(!app.save_payment_form(&mut form));
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM payments", [], |r| r.get::<_, i64>(0))?,
            0
        );
        form.date = crate::today();
        app.db
            .execute("UPDATE gl_settings SET locked_through='' WHERE id=1", [])?;
        crate::billing::add_payment(
            &mut app.db,
            invoice,
            &form.date.to_string(),
            "50.00",
            "Cash",
            "Other receipt",
        )?;
        assert!(!app.save_payment_form(&mut form));
        assert_eq!(form.amount, "100.00");
        assert_eq!(crate::billing::balance(&app.db, invoice)?.remaining, 5000);
        form.amount = "50.00".into();
        assert!(app.save_payment_form(&mut form), "{}", form.error);
        app.open_payment(invoice);
        assert!(app.payment.is_none());
        assert_eq!(crate::billing::balance(&app.db, invoice)?.remaining, 0);
        Ok(())
    }

    #[test]
    fn payment_controls_fit_native_window_and_restore_clears_draft() -> Result<()> {
        let (mut app, invoice) = fixture()?;
        app.open_payment(invoice);
        app.payment.as_mut().unwrap().error = "Validation details. ".repeat(30);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        for _ in 0..4 {
            let viewport_id = egui::ViewportId::from_hash_of("receive_payment_layout_test");
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
                |ctx| app.payment_dialog(ctx),
            );
        }
        for id in ["receive_amount_rect", "receive_save_rect"] {
            let rect = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id)))
                .unwrap();
            assert!(screen.contains_rect(rect), "{id} outside window: {rect:?}");
        }
        assert!(app.has_business_editors());
        app.request_business_close(windows::Kind::Payment);
        assert!(app.payment.is_some());
        app.reset_after_restore()?;
        assert!(app.payment.is_none());
        Ok(())
    }
}
