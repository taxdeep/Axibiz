use super::*;
use crate::billing::{self, DirectInvoice, DirectLine};
use serde_json::Value;

#[derive(Default)]
pub(super) struct DirectUi {
    pub(super) form: Option<Form>,
}

struct Row {
    key: u64,
    price: i64,
    description: String,
    unit: String,
    quantity: i64,
    price_text: String,
    override_price: Option<String>,
    tax: Option<i64>,
    pending: bool,
}
impl Default for Row {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            key: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            price: 0,
            description: String::new(),
            unit: "ea".into(),
            quantity: 1,
            price_text: "0.00".into(),
            override_price: None,
            tax: None,
            pending: false,
        }
    }
}
impl Row {
    fn has_content(&self) -> bool {
        self.price > 0
            || !self.description.trim().is_empty()
            || self.override_price.is_some()
            || self.quantity != 1
            || self.unit != "ea"
            || self.tax.is_some()
            || self.pending
    }
    fn line(&self) -> Result<DirectLine> {
        anyhow::ensure!(
            !self.pending,
            "Select an item from the list, or clear the search to enter a custom item."
        );
        Ok(DirectLine {
            price_id: self.price,
            description: self.description.clone(),
            unit: self.unit.clone(),
            quantity: self.quantity,
            unit_price: self
                .override_price
                .as_deref()
                .map(crate::parse_money)
                .transpose()?,
            tax_code: self.tax,
        })
    }
}
pub(super) struct Form {
    key: String,
    customer: i64,
    date: chrono::NaiveDate,
    currency: String,
    title: String,
    rows: Vec<Row>,
    preview: Option<Value>,
    error: String,
    dirty: bool,
    fx_attempt: Option<String>,
}
impl Form {
    fn draft(&self) -> Result<DirectInvoice> {
        Ok(DirectInvoice {
            request_key: self.key.clone(),
            customer: self.customer,
            date: self.date.to_string(),
            currency: self.currency.clone(),
            title: self.title.clone(),
            lines: self
                .rows
                .iter()
                .filter(|r| r.has_content())
                .map(Row::line)
                .collect::<Result<_>>()?,
        })
    }
    fn change_currency(&mut self, currency: String) {
        if self.currency == currency {
            return;
        }
        self.currency = currency;
        self.preview = None;
        self.dirty = true;
        for row in &mut self.rows {
            row.override_price = if row.price == 0 && row.has_content() {
                row.price_text.clear();
                Some(String::new())
            } else {
                None
            };
        }
    }
}
impl Desktop {
    pub(super) fn open_customer_invoice(&mut self, customer: i64) {
        if self.reuse_business(windows::Kind::Invoice) {
            return;
        }
        let profile = match crate::customer::load(&self.db, customer) {
            Ok(profile) if profile.active => profile,
            _ => return,
        };
        self.open_direct_invoice();
        if let Some(form) = &mut self.direct_invoice.form {
            form.customer = customer;
            form.change_currency(profile.currency);
        }
    }
    fn direct_line_sheet(
        &mut self,
        ui: &mut egui::Ui,
        form: &mut Form,
        prices: &[Vec<String>],
        height: f32,
    ) {
        let mut remove = None;
        invoice_sheet::sheet(
            ui,
            "direct_lines",
            height,
            true,
            form.rows.len(),
            |ui, index, column| {
                let row = &mut form.rows[index];
                match column {
                    0 => {
                        let old = row.price;
                        let input_width = (ui.available_width()
                            - crate::autocomplete::ARROW_WIDTH
                            - ui.spacing().item_spacing.x)
                            .max(50.0);
                        let picked = picker_with_width(
                            ui,
                            &format!("direct_price_{}", row.key),
                            &mut row.price,
                            prices,
                            input_width,
                        );
                        row.pending = picked.pending;
                        form.dirty |= old != row.price || picked.pending;
                        if old != row.price && !row.pending {
                            row.override_price = None;
                            row.tax = None;
                            if row.price > 0 {
                                match self.db.query_row(
                                    "SELECT name,unit FROM prices WHERE id=?",
                                    [row.price],
                                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                                ) {
                                    Ok((name, unit)) => {
                                        row.description = name;
                                        row.unit = unit;
                                    }
                                    Err(e) => form.error = e.to_string(),
                                }
                            } else {
                                row.description.clear();
                                row.unit = "ea".into();
                            }
                        }
                    }
                    1 => {
                        form.dirty |= ui
                            .add_sized(
                                ui.available_size(),
                                egui::DragValue::new(&mut row.quantity).range(1..=i64::MAX),
                            )
                            .changed();
                    }
                    2 => {
                        form.dirty |= ui
                            .add_sized(
                                ui.available_size(),
                                egui::TextEdit::singleline(&mut row.description)
                                    .frame(false)
                                    .id(egui::Id::new(("direct_description", row.key))),
                            )
                            .changed();
                    }
                    3 => {
                        form.dirty |= ui
                            .add_sized(
                                ui.available_size(),
                                egui::TextEdit::singleline(&mut row.unit)
                                    .frame(false)
                                    .id(egui::Id::new(("direct_unit", row.key))),
                            )
                            .changed();
                    }
                    4 => {
                        if row.override_price.is_none() {
                            row.price_text = row
                                .line()
                                .and_then(|line| {
                                    billing::direct_unit_price(
                                        &self.db,
                                        &line,
                                        &form.currency,
                                        &form.date.to_string(),
                                    )
                                })
                                .map(crate::format_money)
                                .unwrap_or_default();
                        }
                        if ui
                            .add_sized(
                                ui.available_size(),
                                egui::TextEdit::singleline(&mut row.price_text)
                                    .frame(false)
                                    .horizontal_align(egui::Align::RIGHT)
                                    .id(egui::Id::new(("direct_price_text", row.key))),
                            )
                            .changed()
                        {
                            row.override_price = Some(row.price_text.clone());
                            form.dirty = true;
                        }
                    }
                    5 => {
                        let subtotal = row
                            .line()
                            .and_then(|line| {
                                billing::direct_unit_price(
                                    &self.db,
                                    &line,
                                    &form.currency,
                                    &form.date.to_string(),
                                )
                            })
                            .ok()
                            .and_then(|price| price.checked_mul(row.quantity));
                        invoice_sheet::amount(
                            ui,
                            &subtotal
                                .map(crate::format_money)
                                .unwrap_or_else(|| "—".into()),
                        );
                    }
                    6 => {
                        let mut tax = row.tax.unwrap_or_else(|| {
                            billing::direct_tax_code(&self.db, form.customer, row.price)
                                .unwrap_or(0)
                        });
                        let old = tax;
                        invoice_sheet::tax_code(ui, &self.db, row.key, &mut tax);
                        if tax != old {
                            row.tax = Some(tax);
                            form.dirty = true;
                        }
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
            form.rows.remove(index);
            form.dirty = true;
        }
    }
    pub(super) fn open_direct_invoice(&mut self) {
        if self.reuse_business(windows::Kind::Invoice) {
            return;
        }
        let result = (|| -> Result<Form> {
            Ok(Form {
                key: self
                    .db
                    .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?,
                customer: 0,
                date: crate::today(),
                currency: Settings::load(&self.db)?.currency,
                title: String::new(),
                rows: vec![Row::default()],
                preview: None,
                error: String::new(),
                dirty: false,
                fx_attempt: None,
            })
        })();
        match result {
            Ok(form) => self.direct_invoice.form = Some(form),
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
            }
        }
    }
    fn direct_preview_form(&self, form: &mut Form) {
        match form
            .draft()
            .and_then(|draft| billing::direct_preview(&self.db, &draft))
        {
            Ok(data) => {
                form.preview = Some(data);
                form.error.clear();
            }
            Err(e) => {
                form.error = format!("{e:#}");
                form.preview = None;
            }
        }
    }
    fn direct_confirm_form(&mut self, form: &mut Form) -> bool {
        let result = (|| -> Result<i64> {
            let draft = form.draft()?;
            let expected = form
                .preview
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Preview the invoice before confirming."))?;
            billing::direct_confirm(&mut self.db, &draft, expected)
        })();
        match result {
            Ok(id) => {
                self.open_invoice(id);
                self.message = "Invoice created. Tax and GL postings saved.".into();
                self.error = false;
                true
            }
            Err(e) => {
                form.error = format!("Invoice was not created: {e:#}");
                form.preview = None;
                false
            }
        }
    }
    fn direct_fx_ready(&mut self, form: &mut Form, ctx: &egui::Context) -> bool {
        if form.preview.is_some() {
            return true;
        }
        let mut pairs = std::collections::BTreeSet::new();
        if form.rows.iter().any(Row::has_content) {
            if let Ok(base) = crate::gl::base(&self.db) {
                pairs.insert((form.currency.clone(), base));
            }
            for row in &form.rows {
                if row.price > 0 && row.override_price.is_none() {
                    if let Ok(source) = self.db.query_row(
                        "SELECT currency FROM prices WHERE id=?",
                        [row.price],
                        |r| r.get::<_, String>(0),
                    ) {
                        pairs.insert((source, form.currency.clone()));
                    }
                }
                let code = row.tax.unwrap_or_else(|| {
                    billing::direct_tax_code(&self.db, form.customer, row.price).unwrap_or(0)
                });
                if self
                    .db
                    .query_row(
                        "SELECT kind='taxable' FROM tax_codes WHERE id=?",
                        [code],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(false)
                {
                    pairs.insert((form.currency.clone(), "CAD".into()));
                }
            }
        }
        let missing: Vec<_> = pairs
            .into_iter()
            .filter(|(source, target)| {
                crate::fx::dated(&self.db, source, target, &form.date.to_string()).is_err()
            })
            .collect();
        if missing.is_empty() {
            return true;
        }
        let key = format!("{}:{missing:?}", form.date);
        if form.fx_attempt.as_ref() != Some(&key) {
            form.fx_attempt = Some(key);
            self.request_dated_fx(ctx, &form.date.to_string());
        }
        false
    }
    pub(super) fn direct_invoice_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.direct_invoice.form.take() else {
            return;
        };
        let mut close = false;
        let customers =
            self.rows("SELECT id,name,currency FROM customers WHERE active=1 ORDER BY name,id");
        let mut prices = self.rows("SELECT p.id,p.name,p.currency || ' - ' || v.name FROM prices p JOIN vendors v ON v.id=p.vendor ORDER BY p.name,p.id");
        prices.insert(
            0,
            vec![
                "0".into(),
                "Custom item".into(),
                "Enter a description and rate".into(),
            ],
        );
        let response = windows::surface(ctx, egui::Id::new("direct_invoice"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 40.0).max(300.0));
            ui.heading(if form.preview.is_some() {
                "Review Invoice"
            } else {
                "New Invoice"
            });
            let body_height = (ctx.content_rect().height() - 200.0).max(160.0);
            egui::ScrollArea::vertical()
                .id_salt("direct_invoice_body")
                .max_height(body_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let body_bottom = ui.cursor().min.y + body_height;
                    if let Some(data) = &form.preview {
                        invoice_sheet::band(ui, data, "Assigned on confirmation");
                        if !form.title.is_empty() {
                            ui.label(&form.title);
                        }
                        self.show_invoice_checks(ui, data);
                        invoice_sheet::snapshot_lines(
                            ui,
                            "direct_preview_lines",
                            data,
                            (body_bottom - ui.cursor().min.y - 120.0).max(84.0),
                        );
                        invoice_sheet::totals(
                            ui,
                            &form.currency,
                            data["net_total"].as_i64().unwrap_or(0),
                            data["tax_total"].as_i64().unwrap_or(0),
                            data["total"].as_i64().unwrap_or(0),
                            None,
                        );
                        ui.collapsing("Tax breakdown", |ui| {
                            if let Some(items) = data["items"].as_array() {
                                for (index, item) in items.iter().enumerate() {
                                    if let Some(parts) = item["tax_snapshot"]["parts"].as_array() {
                                        for part in parts {
                                            ui.label(format!(
                                                "Line {} - {} - {}% - {} {}",
                                                index + 1,
                                                part["name"].as_str().unwrap_or(""),
                                                part["rate"].as_i64().unwrap_or(0) as f64 / 10000.0,
                                                form.currency,
                                                crate::format_money(
                                                    part["amount"].as_i64().unwrap_or(0)
                                                )
                                            ));
                                        }
                                    }
                                }
                            }
                        });
                    } else {
                        egui::Frame::new()
                            .fill(invoice_sheet::BLUE)
                            .inner_margin(6)
                            .show(ui, |ui| {
                                ui.set_min_width((ctx.content_rect().width() - 72.0).max(280.0));
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("CUSTOMER").color(Color32::WHITE));
                                    let old = form.customer;
                                    picker_with_width(
                                        ui,
                                        "direct_customer",
                                        &mut form.customer,
                                        &customers,
                                        220.0,
                                    );
                                    if old != form.customer {
                                        form.dirty = true;
                                        if let Some(customer) = customers
                                            .iter()
                                            .find(|r| r[0] == form.customer.to_string())
                                        {
                                            form.change_currency(customer[2].clone());
                                        }
                                    }
                                    ui.label(RichText::new("DATE").color(Color32::WHITE));
                                    form.dirty |= ui
                                        .add(
                                            egui_extras::DatePickerButton::new(&mut form.date)
                                                .id_salt("direct_date"),
                                        )
                                        .changed();
                                    ui.label(RichText::new("CURRENCY").color(Color32::WHITE));
                                    let mut currency = form.currency.clone();
                                    currency_picker(ui, "direct_currency", &mut currency);
                                    form.change_currency(currency);
                                });
                            });
                        if customers.is_empty() {
                            ui.colored_label(
                                Color32::DARK_RED,
                                "Add a customer before creating an invoice.",
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.label("Reference");
                            form.dirty |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut form.title)
                                        .desired_width(360.0),
                                )
                                .changed();
                        });
                        self.direct_line_sheet(
                            ui,
                            &mut form,
                            &prices,
                            (body_bottom - ui.cursor().min.y - 44.0).max(84.0),
                        );
                        ui.horizontal(|ui| {
                            let add = ui.button("+ Add line");
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("direct_add_rect"), add.rect)
                            });
                            if add.clicked() {
                                form.rows.push(Row::default());
                                form.dirty = true;
                            }
                            ui.small("Rates exclude tax. Preview to review totals.");
                        });
                    }
                });
            let fx_ready = self.direct_fx_ready(&mut form, ctx);
            if !fx_ready {
                if self.fx_receiver.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.small("Fetching missing exchange rates...");
                    });
                } else {
                    ui.colored_label(
                        Color32::DARK_RED,
                        "No saved rate for this date. Check Exchange Rates in Setup.",
                    );
                }
            }
            if !form.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("direct_error")
                    .max_height(36.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::DARK_RED, &form.error);
                    });
            }
            let reason = if form.preview.is_some() {
                None
            } else if form.customer <= 0 {
                Some("Select a customer to continue.")
            } else if !form.rows.iter().any(Row::has_content) {
                Some("Add at least one invoice line.")
            } else if !fx_ready {
                Some("An applicable dated exchange rate is required. Missing quotes are retrieved automatically.")
            } else {
                None
            };
            controls::footer(ui, reason, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if form.preview.is_some() {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Confirm & Issue Invoice").color(Color32::WHITE),
                                )
                                .fill(theme::GREEN),
                            )
                            .clicked()
                        {
                            close = self.direct_confirm_form(&mut form);
                        }
                        if ui.button("Back to editing").clicked() {
                            form.preview = None;
                        }
                    } else {
                        let preview = ui.add_enabled(
                            fx_ready && form.customer > 0 && form.rows.iter().any(Row::has_content),
                            egui::Button::new("Preview Invoice"),
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("direct_preview_rect"), preview.rect)
                        });
                        if preview.clicked() {
                            self.direct_preview_form(&mut form);
                        }
                    }
                    if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Cancel").clicked()
                    {
                        self.request_business_close(windows::Kind::Invoice);
                    }
                });
            });
        });
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            if form.preview.is_some() {
                form.preview = None;
            } else {
                self.request_business_close(windows::Kind::Invoice);
            }
        }
        if !close {
            self.direct_invoice.form = Some(form);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_invoice_keeps_add_and_preview_visible_at_different_sizes() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_direct_invoice();
        {
            let form = app.direct_invoice.form.as_mut().unwrap();
            form.customer = 1;
            form.rows[0].description = "Consulting".into();
            form.rows[0].override_price = Some("10".into());
            form.rows[0].price_text = "10".into();
            form.rows.extend((0..40).map(|_| Row::default()));
            form.error = "Validation message. ".repeat(20);
        }
        for (width, height) in [(1000.0, 600.0), (1200.0, 800.0), (1920.0, 1080.0)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
            for _ in 0..5 {
                let viewport_id = egui::ViewportId::from_hash_of("invoice_layout_test");
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
                        screen_rect: Some(screen),
                        viewport_id,
                        viewports,
                        ..Default::default()
                    },
                    |ctx| app.direct_invoice_dialog(ctx),
                );
            }
            for id in ["direct_add_rect", "direct_preview_rect"] {
                let rect = ctx
                    .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id)))
                    .unwrap();
                assert!(
                    screen.contains_rect(rect),
                    "{id} clipped at {width} x {height}: {rect:?}"
                );
            }
            let form = app.direct_invoice.form.as_ref().unwrap();
            assert_eq!(form.rows.len(), 41);
            assert_eq!(form.rows[0].price_text, "10");
        }
        Ok(())
    }
    #[test]
    fn invoice_fx_uses_saved_rates_and_does_not_repeat_missing_requests() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','hour',0,100,'USD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_direct_invoice();
        let mut form = app.direct_invoice.form.take().unwrap();
        form.customer = 1;
        form.rows[0].price = 1;
        let ctx = egui::Context::default();
        let (_sender, receiver) = std::sync::mpsc::channel();
        app.fx_receiver = Some(receiver);
        assert!(!app.direct_fx_ready(&mut form, &ctx));
        app.fx_receiver = None;
        assert!(!app.direct_fx_ready(&mut form, &ctx));
        assert!(app.fx_receiver.is_none());
        app.db.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        let changes = app.db.total_changes();
        assert!(app.direct_fx_ready(&mut form, &ctx));
        assert!(app.fx_receiver.is_none());
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }
    #[test]
    fn new_invoice_form_previews_and_creates_without_task_and_keeps_invalid_input() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 8;
        let writes = app.db.total_changes();
        app.open_creation();
        assert!(app.direct_invoice.form.is_some());
        assert_eq!(creation_label(8), Some("＋ New Invoice"));
        assert_eq!(app.db.total_changes(), writes);
        let mut form = app.direct_invoice.form.take().unwrap();
        form.customer = 1;
        form.rows[0].description = "Consulting".into();
        form.rows[0].quantity = 2;
        form.rows[0].override_price = Some("invalid".into());
        form.rows[0].price_text = "invalid".into();
        app.direct_preview_form(&mut form);
        assert!(form.preview.is_none());
        assert!(!form.error.is_empty());
        assert_eq!(form.rows[0].override_price.as_deref(), Some("invalid"));
        form.change_currency("USD".into());
        assert!(form.rows[0].price_text.is_empty());
        assert_eq!(form.rows[0].override_price.as_deref(), Some(""));
        assert!(form.draft().is_err());
        form.change_currency("CAD".into());
        form.rows[0].override_price = Some("50".into());
        form.rows[0].price_text = "50".into();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        app.direct_invoice.form = Some(form);
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        }
        let mut form = app.direct_invoice.form.take().unwrap();
        assert_eq!(
            form.rows.len(),
            1,
            "visual filler must not create invoice lines"
        );
        assert_eq!(form.rows[0].description, "Consulting");
        assert_eq!(form.rows[0].price_text, "50");
        app.direct_preview_form(&mut form);
        assert_eq!(form.preview.as_ref().unwrap()["total"], 10000);
        assert_eq!(app.db.total_changes(), writes);
        app.direct_invoice.form = Some(form);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| app.draw(ctx),
        );
        let mut form = app.direct_invoice.form.take().unwrap();
        assert!(app.direct_confirm_form(&mut form), "{}", form.error);
        assert!(app.direct_confirm_form(&mut form));
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM invoice_tasks", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| app.draw(ctx),
        );
        crate::storage::validate_projections(&app.db)?;
        Ok(())
    }
}
