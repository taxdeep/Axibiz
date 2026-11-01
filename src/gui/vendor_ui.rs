use super::people_ui::{empty_state, input, section_frame, tab};
use super::*;
use crate::vendor::Vendor;

#[derive(Default)]
pub(super) struct VendorUi {
    pub(super) form: Option<VendorForm>,
    pub(super) browser: party_ui::Browser,
    tab: usize,
}
pub(super) struct VendorForm {
    pub(super) vendor: Vendor,
    tab: usize,
    error: String,
    baseline: Vendor,
}

impl VendorForm {
    pub(super) fn dirty(&self) -> bool {
        self.vendor != self.baseline
    }
}

impl Desktop {
    pub(super) fn open_vendor_editor(&mut self, id: i64) {
        self.open_vendor_editor_tab(id, 0);
    }

    fn open_vendor_editor_tab(&mut self, id: i64, tab: usize) {
        if self.reuse_business(windows::Kind::Vendor) {
            return;
        }
        match if id == 0 {
            Ok(Vendor::default())
        } else {
            crate::vendor::load(&self.db, id)
        } {
            Ok(vendor) => {
                self.vendor_ui.form = Some(VendorForm {
                    baseline: vendor.clone(),
                    vendor,
                    tab,
                    error: String::new(),
                })
            }
            Err(error) => {
                self.message = format!("Cannot open vendor: {error:#}");
                self.error = true;
            }
        }
    }

    pub(super) fn save_vendor_form(&mut self, form: &mut VendorForm) -> bool {
        match crate::vendor::save(&mut self.db, &form.vendor) {
            Ok(id) => {
                form.vendor.id = id;
                self.vendor_ui.browser.selected = id;
                if !form.vendor.active || self.vendor_ui.browser.status == 2 {
                    self.vendor_ui.browser.status = 1;
                }
                if self.page == 2 {
                    self.vendor_ui.browser.query.clear();
                }
                form.baseline = form.vendor.clone();
                form.error.clear();
                self.message = "Vendor saved.".into();
                self.error = false;
                true
            }
            Err(error) => {
                form.error = format!("Vendor was not saved: {error:#}");
                false
            }
        }
    }

    pub(super) fn vendor_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.vendor_ui.form.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("vendor_editor"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 48.0).max(300.0));
            ui.heading(if form.vendor.id == 0 {
                "New Vendor"
            } else {
                "Edit Vendor"
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Vendor name *");
                ui.add(egui::TextEdit::singleline(&mut form.vendor.name).desired_width(260.0));
                ui.label("Currency *");
                currency_picker(ui, "vendor_currency", &mut form.vendor.currency);
            });
            ui.separator();
            let remaining = ui.available_height();
            let height = if remaining.is_finite() {
                (remaining - 94.0).max(100.0)
            } else {
                (ctx.content_rect().height() - 210.0).max(100.0)
            };
            let mut selected = form.tab;
            people_ui::editor_body(
                ui,
                "vendor",
                height,
                &[
                    (0, "Address Info"),
                    (1, "Payment Settings"),
                    (2, "Sales Tax Settings"),
                    (4, "Account Settings"),
                    (3, "Additional Info"),
                ],
                &mut selected,
                |ui, tab| {
                    form.tab = tab;
                    self.vendor_form_fields(ui, &mut form)
                },
            );
            form.tab = selected;
            if !form.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("vendor_form_error")
                    .max_height(36.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::DARK_RED, &form.error);
                    });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut form.vendor.active, "Active vendor");
                let ready = !form.vendor.name.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new("Save")).clicked() {
                    self.save_vendor_form(&mut form);
                }
                let save = controls::primary(ui, "Save & Close", ready);
                #[cfg(test)]
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("vendor_save_rect"), save.rect));
                if save.clicked() && self.save_vendor_form(&mut form) {
                    close = true;
                }
                if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Close").clicked() {
                    self.request_business_close(windows::Kind::Vendor);
                }
            });
        });
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.request_business_close(windows::Kind::Vendor);
        }
        if !close {
            self.vendor_ui.form = Some(form);
        }
    }

    fn vendor_form_fields(&mut self, ui: &mut egui::Ui, form: &mut VendorForm) {
        let profile = &mut form.vendor.profile;
        match form.tab {
            0 => {
                people_ui::contact_fields(
                    ui,
                    "vendor",
                    &mut [
                        ("Company name", &mut profile.company),
                        ("Contact name", &mut profile.contact_name),
                        ("Main phone", &mut profile.phone),
                        ("Mobile", &mut profile.mobile),
                        ("Main email", &mut profile.email),
                        ("CC email", &mut profile.cc_email),
                        ("Website", &mut profile.website),
                        ("Other contact details", &mut form.vendor.contact),
                    ],
                );
                people_ui::addresses(
                    ui,
                    ("Billed From", "Shipped From"),
                    &mut profile.address,
                    &mut profile.shipping_address,
                    &mut profile.same_address,
                );
            }
            1 => {
                egui::Grid::new("vendor_payment_fields")
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        input(ui, "Account reference", &mut profile.account_number);
                        ui.label("Payment terms");
                        egui::ComboBox::from_id_salt("vendor_payment_terms")
                            .width(230.0)
                            .selected_text(if profile.payment_terms.is_empty() {
                                "Not specified"
                            } else {
                                &profile.payment_terms
                            })
                            .show_ui(ui, |ui| {
                                for terms in
                                    ["", "Due on receipt", "Net 15", "Net 30", "Net 45", "Net 60"]
                                {
                                    ui.selectable_value(
                                        &mut profile.payment_terms,
                                        terms.into(),
                                        if terms.is_empty() {
                                            "Not specified"
                                        } else {
                                            terms
                                        },
                                    );
                                }
                            });
                        ui.end_row();
                    });
                ui.small("Terms suggest the due date of new bills. Existing bills keep their saved dates.");
                ui.small("Opening liabilities are reviewed in Pay Bills > Review A/P; saving a profile does not post an opening balance.");
            }
            2 => {
                egui::Grid::new("vendor_tax_fields")
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Default tax code");
                        invoice_sheet::tax_code(ui, &self.db, 0, &mut form.vendor.tax_code);
                        ui.end_row();
                        input(ui, "Country", &mut profile.country);
                        input(ui, "Business number", &mut profile.business_number);
                    });
                ui.small("Defaults apply to new bill lines. Review tax recovery eligibility on each bill.");
            }
            4 => {
                ui.label("DEFAULT EXPENSE ACCOUNTS");
                ui.label("Prefill up to three different accounts when entering a new bill for this vendor.");
                let accounts=self.rows("SELECT id,code || ' - ' || name FROM accounts WHERE active=1 AND kind='Expense' ORDER BY code");
                let mut defaults = profile.default_accounts.clone();
                defaults.resize(3, 0);
                for (i, account) in defaults.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Account {}", i + 1));
                        picker_with_width(
                            ui,
                            &format!("vendor_default_account_{i}"),
                            account,
                            &accounts,
                            260.0,
                        );
                        if ui.small_button("Clear").clicked() {
                            *account = 0
                        }
                    });
                }
                profile.default_accounts = defaults.into_iter().filter(|id| *id > 0).collect();
                if ui.button("Clear All").clicked() {
                    profile.default_accounts.clear()
                }
                ui.small("Entered bill amounts and manual splits are preserved. Blank prefilled lines are ignored on save.");
            }
            _ => {
                egui::Grid::new("vendor_additional_fields").show(ui, |ui| {
                    input(ui, "Vendor type", &mut profile.vendor_type);
                });
                ui.label("Vendor notes");
                ui.add(
                    egui::TextEdit::multiline(&mut profile.notes)
                        .desired_rows(8)
                        .desired_width(400.0),
                );
            }
        }
    }

    pub(super) fn vendor_center(&mut self, ui: &mut egui::Ui) {
        self.party_center(ui, party_ui::Party::Vendor);
    }
    pub(super) fn vendor_action(&mut self, action: party_ui::Action) {
        let id = if self.vendor_ui.browser.mode == 1 {
            0
        } else {
            self.vendor_ui.browser.selected
        };
        if id > 0 && matches!(action, party_ui::Action::Primary) {
            match crate::vendor::load(&self.db, id) {
                Ok(p) if p.active => {}
                Ok(_) => {
                    self.message = "Activate this vendor before creating a transaction.".into();
                    self.error = true;
                    return;
                }
                Err(e) => {
                    self.message = format!("Cannot load vendor: {e:#}");
                    self.error = true;
                    return;
                }
            }
        }
        match action {
            party_ui::Action::New => self.open_vendor_editor(0),
            party_ui::Action::Primary => {
                if let Ok(v) = crate::vendor::load(&self.db, id) {
                    self.open_vendor_expense(&v)
                } else {
                    self.open_accounting_form(0)
                }
            }
            party_ui::Action::Secondary => self.open_pay_bills(id, None),
            party_ui::Action::Edit => self.open_vendor_editor(id),
            party_ui::Action::Export => self.export_vendor_register(),
        }
    }

    fn export_vendor_register(&mut self) {
        self.export_party_transactions(party_ui::Party::Vendor);
    }

    pub(super) fn vendor_information(&mut self, ui: &mut egui::Ui, height: f32) {
        let id = self.vendor_ui.browser.selected;
        if id == 0 {
            if empty_state(
                ui,
                "No vendors found",
                "Try another search or status filter, or create a vendor to get started.",
                "New Vendor",
                true,
            ) {
                self.open_vendor_editor(0);
            }
            return;
        }
        let vendor = match crate::vendor::load(&self.db, id) {
            Ok(v) => v,
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
                return;
            }
        };
        let bottom = ui.cursor().min.y + height;
        self.vendor_overview(ui, &vendor);
        ui.add_space(8.0);
        let tabs = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (index, name) in ["Transactions", "Contacts", "Notes", "Catalog"]
                .iter()
                .enumerate()
            {
                tab(ui, &mut self.vendor_ui.tab, index, name);
            }
        });
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("vendor_tabs_rect"), tabs.response.rect));
        #[cfg(not(test))]
        let _ = tabs;
        ui.add_space(6.0);
        let remaining = (bottom - ui.cursor().min.y).max(120.0);
        if self.vendor_ui.tab == 0 {
            self.vendor_expenses(ui, id, remaining);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("vendor_details", id, self.vendor_ui.tab))
            .max_height(remaining)
            .show(ui, |ui| match self.vendor_ui.tab {
                1 => self.vendor_contacts(ui, &vendor),
                2 => {
                    if vendor.profile.notes.trim().is_empty() {
                        if empty_state(
                            ui,
                            "No vendor notes yet",
                            "Keep supplier instructions and follow-up details here.",
                            "Add Notes",
                            true,
                        ) {
                            self.open_vendor_editor_tab(id, 3);
                        }
                    } else {
                        section_frame().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.strong("Vendor notes");
                            ui.add_space(8.0);
                            ui.add(egui::Label::new(&vendor.profile.notes).wrap());
                            if icons::action(ui, icons::Icon::Edit, "Edit Notes", true).clicked() {
                                self.open_vendor_editor_tab(id, 3);
                            }
                        });
                    }
                }
                _ => self.vendor_items(ui, &vendor, remaining),
            });
    }

    fn vendor_overview(&mut self, ui: &mut egui::Ui, person: &Vendor) {
        let balances = match self.party_cache.load(&self.db, party_ui::Party::Vendor) {
            Ok(data) => match data.amounts.get(&person.id) {
                Some(totals) => totals
                    .iter()
                    .map(|(currency, _)| {
                        match crate::ap::reconciliation(
                            &self.db,
                            person.id,
                            currency,
                            &crate::today().to_string(),
                        ) {
                            Ok(r) if r.ready() => {
                                format!("{currency} {} open", crate::format_money(r.open))
                            }
                            Ok(_) => format!("{currency}: Needs review"),
                            Err(_) => format!("{currency}: Unavailable"),
                        }
                    })
                    .collect::<Vec<_>>(),
                None => vec!["No bills recorded".into()],
            },
            Err(e) => vec![format!("Unavailable: {e}")],
        };
        if let Some(action) = party_ui::overview(
            ui,
            &party_ui::Overview {
                party: party_ui::Party::Vendor,
                name: &person.name,
                active: person.active,
                currency: &person.currency,
                contact: &person.profile.contact_name,
                address: &person.profile.address,
                balance_label: "SUPPLIER A/P",
                balances: &balances,
            },
        ) {
            self.vendor_action(action)
        }
    }

    pub(super) fn open_vendor_expense(&mut self, vendor: &Vendor) {
        if self.reuse_business(windows::Kind::Accounting) {
            return;
        }
        self.open_accounting_form(0);
        if let Some(accounting_ui::Form::Expense { sheet }) = &mut self.accounting.form {
            sheet.apply_vendor_defaults(vendor);
            sheet.mark_clean();
        }
    }

    fn vendor_contacts(&mut self, ui: &mut egui::Ui, vendor: &Vendor) {
        let p = &vendor.profile;
        if [
            &p.contact_name,
            &p.company,
            &p.phone,
            &p.mobile,
            &p.email,
            &p.website,
            &p.address,
            &vendor.contact,
        ]
        .iter()
        .all(|v| v.trim().is_empty())
        {
            if empty_state(
                ui,
                "No vendor contact details yet",
                "Add a contact name, phone, email or remit-to address.",
                "Add Contact Details",
                true,
            ) {
                self.open_vendor_editor(vendor.id);
            }
            return;
        }
        let contacts = |ui: &mut egui::Ui| {
            section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Contact details");
                ui.add_space(10.0);
                egui::Grid::new("vendor_contact_view")
                    .num_columns(2)
                    .spacing([16.0, 12.0])
                    .max_col_width((invoice_sheet::finite_width(ui) - 100.0).max(90.0))
                    .show(ui, |ui| {
                        for (label, value, required) in [
                            ("Contact", &p.contact_name, true),
                            ("Company", &p.company, false),
                            ("Phone", &p.phone, true),
                            ("Mobile", &p.mobile, false),
                            ("Email", &p.email, true),
                            ("Website", &p.website, false),
                            ("Other", &vendor.contact, false),
                        ] {
                            if !required && value.trim().is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(label).small().color(theme::MUTED));
                            value_label(ui, value);
                            ui.end_row();
                        }
                    });
            });
        };
        let address = |ui: &mut egui::Ui| {
            section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Mailing / Remit-to address");
                ui.add_space(10.0);
                value_label(ui, &p.address);
            });
        };
        let width = ui.available_width();
        if width >= 760.0 {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let column = (width - 16.0) / 2.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(column, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(column);
                        contacts(ui);
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(column, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(column);
                        address(ui);
                    },
                );
            });
        } else {
            contacts(ui);
            ui.add_space(12.0);
            address(ui);
        }
    }

    fn vendor_expenses(&mut self, ui: &mut egui::Ui, id: i64, height: f32) {
        self.party_transactions(ui, party_ui::Party::Vendor, id, height);
    }

    fn vendor_items(&mut self, ui: &mut egui::Ui, vendor: &Vendor, height: f32) {
        let rows = self.rows(&format!(
            "SELECT id,name,unit,currency,cost,price FROM prices WHERE vendor={} ORDER BY name,id",
            vendor.id
        ));
        if rows.is_empty() {
            if empty_state(
                ui,
                "No items or services yet",
                "Create a catalog item for work supplied by this vendor.",
                "New Item",
                vendor.active,
            ) {
                self.open_vendor_item(vendor);
            }
            return;
        }
        if ui
            .add_enabled(vendor.active, egui::Button::new("New Item"))
            .clicked()
        {
            self.open_vendor_item(vendor);
        }
        let width = invoice_sheet::finite_width(ui).max(560.0);
        invoice_sheet::register_columns(
            ui,
            "vendor_items",
            (height - 74.0).max(84.0),
            &["ITEM", "UNIT", "CURR.", "COST", "PRICE", ""],
            &[width - 390.0, 60.0, 60.0, 100.0, 100.0, 70.0],
            rows.len(),
            |ui, index, column| {
                let row = &rows[index];
                match column {
                    0 => {
                        ui.add(egui::Label::new(&row[1]).truncate())
                            .on_hover_text(&row[1]);
                    }
                    1 => {
                        ui.label(&row[2]);
                    }
                    2 => {
                        ui.label(&row[3]);
                    }
                    3 | 4 => invoice_sheet::amount(
                        ui,
                        &crate::format_money(row[column + 1].parse().unwrap_or(0)),
                    ),
                    _ => {
                        if icons::button(ui, icons::Icon::Edit, "Edit record").clicked()
                            && self.price_edit.is_none()
                        {
                            self.price_edit = Some(PriceEdit {
                                id: row[0].parse().unwrap_or(0),
                                name: row[1].clone(),
                                unit: row[2].clone(),
                                currency: row[3].clone(),
                                cost: crate::format_money(row[4].parse().unwrap_or(0)),
                                price: crate::format_money(row[5].parse().unwrap_or(0)),
                                error: String::new(),
                                ..Default::default()
                            });
                        }
                    }
                }
            },
        );
    }

    fn open_vendor_item(&mut self, vendor: &Vendor) {
        if self.new_record.is_some() {
            self.message = "Finish the current item draft before opening another.".into();
            return;
        }
        let page = self.page;
        self.page = 3;
        self.open_creation();
        self.page = page;
        if let Some(draft) = &mut self.new_record {
            draft.vendor = vendor.id;
            draft.currency = vendor.currency.clone();
            draft.unit = "item".into();
        }
    }
}

fn value_label(ui: &mut egui::Ui, value: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(if value.trim().is_empty() {
                "Not provided"
            } else {
                value
            })
            .color(if value.trim().is_empty() {
                theme::MUTED
            } else {
                theme::INK
            }),
        )
        .wrap(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(Desktop, Vendor, i64)> {
        let mut db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut vendor = Vendor {
            name: "Supplier with a long display name".into(),
            currency: "USD".into(),
            tax_code: 1,
            ..Default::default()
        };
        vendor.profile.company = "Supplier company ".repeat(12);
        vendor.profile.email = "accounts@example.com".into();
        vendor.profile.address = "A long mailing address\n".repeat(15);
        vendor.profile.notes = "Saved vendor notes\n".repeat(80);
        vendor.profile.payment_terms = "Net 30".into();
        vendor.id = crate::vendor::save(&mut db, &vendor)?;
        let document = crate::expense_doc::Document {
            id: 0,
            vendor: Some(vendor.id),
            date: crate::today().to_string(),
            currency: "CAD".into(),
            reference: "BILL-42".into(),
            due_date: None,
            memo: "Office supplies".into(),
            terms: String::new(),
            lines: vec![crate::accounting::Expense {
                id: 0,
                date: crate::today().to_string(),
                description: "Supplies".into(),
                account: crate::accounting::default_account(&db)?,
                vendor: Some(vendor.id),
                currency: "CAD".into(),
                amount: "25.00".into(),
                task: None,
                tax: Default::default(),
            }],
        };
        let id = crate::expense_doc::save(&mut db, &document)?;
        db.execute("INSERT INTO prices(vendor,name,unit,currency,cost,price) VALUES(?,'Office service','item','USD',500,1000)",[vendor.id])?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 2;
        Ok((app, vendor, id))
    }

    #[test]
    fn vendor_editor_is_singleton_retains_failures_and_fits_small_native_windows() -> Result<()> {
        let (mut app, vendor, _) = fixture()?;
        app.open_vendor_editor(vendor.id);
        app.vendor_ui.form.as_mut().unwrap().vendor.profile.notes = "Unsaved vendor notes".into();
        app.open_vendor_editor(99999);
        let mut form = app.vendor_ui.form.take().unwrap();
        assert_eq!(form.vendor.id, vendor.id);
        form.vendor.name.clear();
        assert!(!app.save_vendor_form(&mut form));
        assert_eq!(form.vendor.profile.notes, "Unsaved vendor notes");
        form.vendor.name = "Changed supplier".into();
        assert!(app.save_vendor_form(&mut form));
        assert_eq!(
            crate::vendor::load(&app.db, vendor.id)?.profile.notes,
            "Unsaved vendor notes"
        );
        form.error = "Validation message. ".repeat(30);
        app.vendor_ui.form = Some(form);
        let changes = app.db.total_changes();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        for tab in 0..5 {
            app.vendor_ui.form.as_mut().unwrap().tab = tab;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            for _ in 0..4 {
                let viewport_id = egui::ViewportId::from_hash_of("vendor_editor_layout");
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
                    |ctx| app.vendor_dialog(ctx),
                );
            }
            let save = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("vendor_save_rect")))
                .unwrap();
            assert!(
                screen.contains_rect(save),
                "Vendor save clipped on tab {tab}: {save:?}"
            );
        }
        assert_eq!(app.db.total_changes(), changes);
        app.request_business_close(windows::Kind::Vendor);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.business_windows(ctx));
        assert!(app.vendor_ui.form.is_none()); // A saved, unchanged profile closes directly.
        app.open_vendor_editor(vendor.id);
        app.vendor_ui
            .form
            .as_mut()
            .unwrap()
            .vendor
            .name
            .push_str(" unsaved");
        app.request_business_close(windows::Kind::Vendor);
        let _ = ctx.run(Default::default(), |ctx| app.business_windows(ctx));
        assert!(app.vendor_ui.form.is_some());
        app.reset_after_restore()?;
        assert!(app.vendor_ui.form.is_none());
        Ok(())
    }

    #[test]
    fn vendor_workspace_contains_tabs_and_tables_and_expense_drilldown_preserves_search(
    ) -> Result<()> {
        let (mut app, _, document) = fixture()?;
        app.vendor_ui.browser.query = "Supplier".into();
        let changes = app.db.total_changes();
        for (width, height) in [
            (800.0, 600.0),
            (900.0, 650.0),
            (1200.0, 800.0),
            (1600.0, 900.0),
            (2000.0, 1000.0),
        ] {
            for view in 0..5 {
                app.vendor_ui.browser.mode = usize::from(view == 4);
                app.vendor_ui.tab = view.min(3);
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                for _ in 0..4 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ctx| app.draw(ctx),
                    );
                }
                let rect = |name| {
                    ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(name)))
                        .unwrap()
                };
                let workspace = rect("vendor_workspace_rect");
                if view != 4 {
                    let title = rect("vendor_title_rect");
                    let actions = rect("vendor_actions_rect");
                    let tabs = rect("vendor_tabs_rect");
                    assert!(
                        screen.contains_rect(title)
                            && screen.contains_rect(actions)
                            && screen.contains_rect(tabs)
                    );
                    assert!(actions.top() >= title.bottom() && tabs.top() >= actions.bottom());
                    if let Some(sidebar) =
                        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("vendor_sidebar_rect")))
                    {
                        let details = rect("vendor_information_rect");
                        assert!((180.0..=240.1).contains(&sidebar.width()));
                        assert!(details.left() >= sidebar.right());
                        assert!(
                            screen.contains_rect(details),
                            "Vendor details overflow at {width}: {details:?}"
                        );
                    }
                }
                if view == 0 || view == 3 || view == 4 {
                    let id = if view == 3 {
                        "vendor_items"
                    } else {
                        "vendor_expenses"
                    };
                    let rows = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::new(("sheet_rows_viewport", id)))
                        })
                        .unwrap();
                    assert!(rows.bottom()<=workspace.bottom()+1.0,"Vendor rows clipped at {width}x{height}, view {view}: {rows:?}, workspace {workspace:?}");
                    let viewport = ctx
                        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("sheet_viewport", id))))
                        .unwrap();
                    assert!(
                        workspace.contains_rect(viewport),
                        "Vendor table overflows at {width}: {viewport:?}"
                    );
                    if (view != 4 && width >= 900.0) || (view == 4 && width >= 1200.0) {
                        let last_column = if view == 4 { 6_usize } else { 5_usize };
                        let cell = ctx
                            .data(|d| {
                                d.get_temp::<egui::Rect>(egui::Id::new((
                                    "sheet_cell",
                                    id,
                                    last_column,
                                )))
                            })
                            .unwrap();
                        assert!(
                            workspace.contains_rect(cell),
                            "Vendor amount or action clipped at {width}, view {view}: {cell:?}"
                        );
                    }
                }
            }
        }
        assert_eq!(app.db.total_changes(), changes);
        app.vendor_ui.browser.mode = 0;
        app.vendor_ui.tab = 0;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut Desktop, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            )
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let cell = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::new(("sheet_cell", "vendor_expenses", 1_usize)))
            })
            .unwrap();
        let pos = egui::pos2(cell.left() + 12.0, cell.center().y);
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
        assert_eq!(app.page, 2);
        assert_eq!(app.vendor_ui.browser.query, "Supplier");
        let Some(accounting_ui::Form::Expense { sheet }) = &app.accounting.form else {
            panic!("Expense number should open its document");
        };
        assert_eq!(sheet.doc.id, document);
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }

    #[test]
    fn vendor_actions_prefill_defaults_and_preserve_existing_drafts_and_workspace() -> Result<()> {
        let (mut app, vendor, _) = fixture()?;
        let changes = app.db.total_changes();
        app.open_vendor_expense(&vendor);
        let Some(accounting_ui::Form::Expense { sheet }) = &mut app.accounting.form else {
            panic!("Expected expense draft");
        };
        assert_eq!(sheet.doc.vendor, Some(vendor.id));
        assert_eq!(sheet.doc.currency, "USD");
        assert_eq!(sheet.doc.lines[0].tax.code, 1);
        assert_eq!(
            sheet.doc.due_date,
            Some((crate::today() + chrono::Days::new(30)).to_string())
        );
        sheet.doc.memo = "Keep expense draft".into();
        let other = Vendor {
            id: 9999,
            ..Default::default()
        };
        app.open_vendor_expense(&other);
        let Some(accounting_ui::Form::Expense { sheet }) = &app.accounting.form else {
            panic!("Expected retained draft");
        };
        assert_eq!(sheet.doc.vendor, Some(vendor.id));
        assert_eq!(sheet.doc.memo, "Keep expense draft");
        app.open_vendor_item(&vendor);
        let mut draft = app.new_record.take().unwrap();
        assert_eq!(draft.vendor, vendor.id);
        assert_eq!(draft.currency, "USD");
        draft.name = "Another item".into();
        draft.price = "10.00".into();
        assert!(app.save_record(&mut draft));
        assert_eq!(app.page, 2);
        assert!(app.db.total_changes() > changes);
        let changes = app.db.total_changes();
        app.open_vendor_item(&vendor);
        app.new_record.as_mut().unwrap().name = "Keep item draft".into();
        app.open_vendor_item(&other);
        assert_eq!(app.new_record.as_ref().unwrap().vendor, vendor.id);
        assert_eq!(app.new_record.as_ref().unwrap().name, "Keep item draft");
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }

    #[test]
    fn vendor_empty_sections_open_the_correct_editor_without_writes() -> Result<()> {
        for (tab, title, editor_tab) in [
            (1, "No vendor contact details yet", 0),
            (2, "No vendor notes yet", 3),
        ] {
            let db = Connection::open_in_memory()?;
            crate::init(&db)?;
            let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
            app.page = 2;
            let vendor = Vendor {
                name: "Supplier".into(),
                ..Default::default()
            };
            let id = crate::vendor::save(&mut app.db, &vendor)?;
            app.vendor_ui.tab = tab;
            let changes = app.db.total_changes();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let frame = |app: &mut Desktop, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.vendor_center(ui));
                    },
                )
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let action = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("party_empty_action", title))))
                .unwrap()
                .center();
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(action),
                        egui::Event::PointerButton {
                            pos: action,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            let form = app.vendor_ui.form.as_mut().unwrap();
            assert_eq!(form.vendor.id, id);
            assert_eq!(form.tab, editor_tab);
            form.vendor.profile.notes = "Keep draft".into();
            app.open_vendor_editor_tab(99999, 1);
            let form = app.vendor_ui.form.as_ref().unwrap();
            assert_eq!(form.vendor.id, id);
            assert_eq!(form.tab, editor_tab);
            assert_eq!(form.vendor.profile.notes, "Keep draft");
            assert_eq!(app.db.total_changes(), changes);
        }
        Ok(())
    }
}
