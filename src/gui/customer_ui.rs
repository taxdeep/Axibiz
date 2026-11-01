use super::people_ui::{empty_state, input, section_frame, tab as customer_tab};
use super::*;
use crate::customer::Customer;

#[derive(Default)]
pub(super) struct CustomerUi {
    pub(super) form: Option<CustomerForm>,
    pub(super) browser: party_ui::Browser,
    tab: usize,
}

pub(super) struct CustomerForm {
    pub(super) customer: Customer,
    tab: usize,
    error: String,
    baseline: Customer,
}

fn contact_details(ui: &mut egui::Ui, customer: &Customer) {
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Contact details");
        ui.add_space(10.0);
        let width = (invoice_sheet::finite_width(ui) - 100.0).max(90.0);
        egui::Grid::new("customer_contact_information")
            .num_columns(2)
            .spacing([16.0, 12.0])
            .max_col_width(width)
            .show(ui, |ui| {
                for (label, value, required) in [
                    ("Contact", &customer.profile.contact_name, true),
                    ("Company", &customer.profile.company, false),
                    ("Phone", &customer.profile.phone, true),
                    ("Mobile", &customer.profile.mobile, false),
                    ("Email", &customer.profile.email, true),
                    ("CC email", &customer.profile.cc_email, false),
                    ("Website", &customer.profile.website, false),
                    ("Other", &customer.contact, false),
                ] {
                    if !required && value.trim().is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(label).small().color(theme::MUTED));
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
                    ui.end_row();
                }
            });
    });
}

fn customer_addresses(ui: &mut egui::Ui, customer: &Customer) {
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Addresses");
        ui.add_space(10.0);
        for (label, value) in [
            ("BILL TO", customer.profile.billing_address.as_str()),
            (
                "SHIP TO",
                if customer.profile.same_shipping
                    && !customer.profile.billing_address.trim().is_empty()
                {
                    "Same as billing"
                } else {
                    &customer.profile.shipping_address
                },
            ),
        ] {
            ui.label(RichText::new(label).small().color(theme::MUTED));
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
            ui.add_space(10.0);
        }
    });
}

impl CustomerForm {
    pub(super) fn dirty(&self) -> bool {
        self.customer != self.baseline
    }
}

impl Desktop {
    pub(super) fn open_customer_editor(&mut self, id: i64) {
        self.open_customer_editor_tab(id, 0);
    }

    fn open_customer_editor_tab(&mut self, id: i64, tab: usize) {
        if self.reuse_business(windows::Kind::Customer) {
            return;
        }
        let result = if id == 0 {
            Ok(Customer::default())
        } else {
            crate::customer::load(&self.db, id)
        };
        match result {
            Ok(customer) => {
                self.customer_ui.form = Some(CustomerForm {
                    baseline: customer.clone(),
                    customer,
                    tab,
                    error: String::new(),
                })
            }
            Err(error) => {
                self.message = format!("Cannot open customer: {error:#}");
                self.error = true;
            }
        }
    }
    pub(super) fn save_customer_form(&mut self, form: &mut CustomerForm) -> bool {
        match crate::customer::save(&mut self.db, &form.customer) {
            Ok(id) => {
                form.customer.id = id;
                if form.customer.profile.same_shipping {
                    form.customer.profile.shipping_address =
                        form.customer.profile.billing_address.clone();
                }
                self.customer_ui.browser.selected = id;
                if !form.customer.active || self.customer_ui.browser.status == 2 {
                    self.customer_ui.browser.status = 1;
                }
                if self.page == 1 {
                    self.customer_ui.browser.query.clear();
                }
                form.baseline = form.customer.clone();
                form.error.clear();
                self.message = "Customer saved.".into();
                self.error = false;
                true
            }
            Err(error) => {
                form.error = format!("Customer was not saved: {error:#}");
                false
            }
        }
    }
    pub(super) fn customer_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.customer_ui.form.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("customer_editor"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 48.0).max(300.0));
            ui.heading(if form.customer.id == 0 {
                "New Customer"
            } else {
                "Edit Customer"
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Customer name *");
                ui.add(egui::TextEdit::singleline(&mut form.customer.name).desired_width(270.0));
                ui.label("Currency *");
                currency_picker(ui, "customer_currency", &mut form.customer.currency);
            });
            ui.separator();
            let height = if ui.available_height().is_finite() {
                (ui.available_height() - 100.0).max(100.0)
            } else {
                (ctx.content_rect().height() - 245.0).max(100.0)
            };
            let mut selected = form.tab;
            people_ui::editor_body(
                ui,
                "customer",
                height,
                &[
                    (0, "Address Info"),
                    (1, "Payment Settings"),
                    (2, "Sales Tax Settings"),
                    (3, "Additional Info"),
                ],
                &mut selected,
                |ui, tab| {
                    form.tab = tab;
                    let profile = &mut form.customer.profile;
                    match form.tab {
                        0 => {
                            people_ui::contact_fields(
                                ui,
                                "customer",
                                &mut [
                                    ("Company name", &mut profile.company),
                                    ("Contact name", &mut profile.contact_name),
                                    ("Main phone", &mut profile.phone),
                                    ("Mobile", &mut profile.mobile),
                                    ("Main email", &mut profile.email),
                                    ("CC email", &mut profile.cc_email),
                                    ("Website", &mut profile.website),
                                    ("Other contact details", &mut form.customer.contact),
                                ],
                            );
                            people_ui::addresses(
                                ui,
                                ("Bill To", "Ship To"),
                                &mut profile.billing_address,
                                &mut profile.shipping_address,
                                &mut profile.same_shipping,
                            );
                        }
                        1 => {
                            egui::Grid::new("customer_payment_fields")
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    input(ui, "Account number", &mut profile.account_number);
                                    ui.label("Payment terms");
                                    egui::ComboBox::from_id_salt("customer_terms")
                                        .width(260.0)
                                        .selected_text(if profile.payment_terms.is_empty() {
                                            "Use invoice template"
                                        } else {
                                            &profile.payment_terms
                                        })
                                        .show_ui(ui, |ui| {
                                            for terms in [
                                                "",
                                                "Due on receipt",
                                                "Net 15",
                                                "Net 30",
                                                "Net 45",
                                                "Net 60",
                                            ] {
                                                ui.selectable_value(
                                                    &mut profile.payment_terms,
                                                    terms.into(),
                                                    if terms.is_empty() {
                                                        "Use invoice template"
                                                    } else {
                                                        terms
                                                    },
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    ui.label("Preferred payment method");
                                    egui::ComboBox::from_id_salt("customer_payment_method")
                                        .width(260.0)
                                        .selected_text(if profile.payment_method.is_empty() {
                                            "Use default"
                                        } else {
                                            &profile.payment_method
                                        })
                                        .show_ui(ui, |ui| {
                                            for method in [
                                                "",
                                                "Bank transfer",
                                                "Cash",
                                                "Cheque",
                                                "Credit card",
                                                "Other",
                                            ] {
                                                ui.selectable_value(
                                                    &mut profile.payment_method,
                                                    method.into(),
                                                    if method.is_empty() {
                                                        "Use default"
                                                    } else {
                                                        method
                                                    },
                                                );
                                            }
                                        });
                                    ui.end_row();
                                });
                            ui.label("Payment terms apply to new invoices. Existing invoice terms are preserved.");
                            ui.collapsing("Opening balances", |ui| { ui.label("Opening balances require accounting entries and supporting invoices. Customer profile edits do not create balances."); });
                        }
                        2 => {
                            egui::Grid::new("customer_tax_fields")
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Default tax code");
                                    invoice_sheet::tax_code(
                                        ui,
                                        &self.db,
                                        0,
                                        &mut form.customer.tax_code,
                                    );
                                    ui.end_row();
                                    input(ui, "Country", &mut profile.country);
                                    input(ui, "Business number", &mut profile.business_number);
                                });
                            ui.label("The default tax code is used for new sales when the item has no tax-code override.");
                        }
                        _ => {
                            egui::Grid::new("customer_additional_fields").show(ui, |ui| {
                                input(ui, "Customer type", &mut profile.customer_type);
                            });
                            ui.label("Notes");
                            ui.add(
                                egui::TextEdit::multiline(&mut profile.notes)
                                    .desired_rows(8)
                                    .desired_width(invoice_sheet::finite_width(ui)),
                            );
                        }
                    }
                },
            );
            form.tab = selected;
            if !form.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("customer_error")
                    .max_height(36.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::DARK_RED, &form.error);
                    });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut form.customer.active, "Active customer");
                let enabled = !form.customer.name.trim().is_empty();
                if ui.add_enabled(enabled, egui::Button::new("Save")).clicked() {
                    self.save_customer_form(&mut form);
                }
                let save = controls::primary(ui, "Save & Close", enabled);
                #[cfg(test)]
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("customer_save_rect"), save.rect));
                if save.clicked() && self.save_customer_form(&mut form) {
                    close = true;
                }
                if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Close").clicked() {
                    self.request_business_close(windows::Kind::Customer);
                }
            });
        });
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.request_business_close(windows::Kind::Customer);
        }
        if !close {
            self.customer_ui.form = Some(form);
        }
    }

    pub(super) fn customer_center(&mut self, ui: &mut egui::Ui) {
        self.party_center(ui, party_ui::Party::Customer);
    }
    pub(super) fn customer_action(&mut self, action: party_ui::Action) {
        let id = if self.customer_ui.browser.mode == 1 {
            0
        } else {
            self.customer_ui.browser.selected
        };
        if id > 0
            && matches!(
                action,
                party_ui::Action::Primary | party_ui::Action::Secondary
            )
        {
            match crate::customer::load(&self.db, id) {
                Ok(p) if p.active => {}
                Ok(_) => {
                    self.message = "Activate this customer before creating a transaction.".into();
                    self.error = true;
                    return;
                }
                Err(e) => {
                    self.message = format!("Cannot load customer: {e:#}");
                    self.error = true;
                    return;
                }
            }
        }
        match action {
            party_ui::Action::New => self.open_customer_editor(0),
            party_ui::Action::Primary => {
                if id > 0 {
                    self.open_customer_invoice(id)
                } else {
                    self.open_direct_invoice()
                }
            }
            party_ui::Action::Secondary => {
                if let Ok(c) = crate::customer::load(&self.db, id) {
                    self.open_customer_task(&c)
                } else {
                    self.open_new_task()
                }
            }
            party_ui::Action::Edit => self.open_customer_editor(id),
            party_ui::Action::Export => self.export_customer_register(),
        }
    }

    fn export_customer_register(&mut self) {
        self.export_party_transactions(party_ui::Party::Customer);
    }

    pub(super) fn customer_information(&mut self, ui: &mut egui::Ui, height: f32) {
        let id = self.customer_ui.browser.selected;
        if id == 0 {
            if empty_state(
                ui,
                "No customers found",
                "Try another search or status filter, or create a customer to get started.",
                "New Customer",
                true,
            ) {
                self.open_customer_editor(0);
            }
            return;
        }
        let customer = match crate::customer::load(&self.db, id) {
            Ok(customer) => customer,
            Err(error) => {
                ui.colored_label(Color32::DARK_RED, error.to_string());
                return;
            }
        };
        let bottom = ui.cursor().min.y + height;
        self.customer_overview(ui, &customer);
        ui.add_space(8.0);
        let tabs = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (tab, name) in ["Transactions", "Contacts", "Notes", "Tasks"]
                .iter()
                .enumerate()
            {
                customer_tab(ui, &mut self.customer_ui.tab, tab, name);
            }
        });
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("customer_tabs_rect"), tabs.response.rect));
        #[cfg(not(test))]
        let _ = tabs;
        ui.add_space(6.0);
        let remaining = (bottom - ui.cursor().min.y).max(120.0);
        if self.customer_ui.tab == 0 {
            self.customer_transactions(ui, id, remaining);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("customer_information_body", id, self.customer_ui.tab))
            .max_height(remaining)
            .show(ui, |ui| match self.customer_ui.tab {
                1 => self.customer_contacts(ui, &customer),
                2 => {
                    if customer.profile.notes.trim().is_empty() {
                        if empty_state(
                            ui,
                            "No notes yet",
                            "Keep useful customer information and follow-up details here.",
                            "Add Notes",
                            true,
                        ) {
                            self.open_customer_editor_tab(id, 3);
                        }
                    } else {
                        section_frame().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.strong("Customer notes");
                            ui.add_space(8.0);
                            ui.add(egui::Label::new(&customer.profile.notes).wrap());
                            ui.add_space(8.0);
                            if icons::action(ui, icons::Icon::Edit, "Edit Notes", true).clicked() {
                                self.open_customer_editor_tab(id, 3);
                            }
                        });
                    }
                }
                _ => {
                    let sql = format!(
                        "SELECT id,COALESCE(number,''),COALESCE(task_date,''),title \
                         FROM tasks WHERE customer_id={id} ORDER BY id DESC"
                    );
                    let rows = self.rows(&sql);
                    if rows.is_empty()
                        && empty_state(
                            ui,
                            "No linked tasks",
                            "Create a task to track work for this customer.",
                            "New Task",
                            customer.active,
                        )
                    {
                        self.open_customer_task(&customer);
                    }
                    for row in rows {
                        if ui
                            .button(format!("{}  {}  {}", row[1], row[2], row[3]))
                            .clicked()
                        {
                            self.open_task_editor(row[0].parse().unwrap_or(0));
                        }
                    }
                }
            });
    }

    fn open_customer_task(&mut self, customer: &Customer) {
        if !self.reuse_business(windows::Kind::Task) {
            self.open_new_task();
            self.customer = customer.id;
            self.task_currency = customer.currency.clone();
        }
    }

    fn customer_overview(&mut self, ui: &mut egui::Ui, person: &Customer) {
        let balances = match self.party_cache.load(&self.db, party_ui::Party::Customer) {
            Ok(data) => vec![party_data::summary(
                data.amounts.get(&person.id),
                &person.currency,
            )],
            Err(e) => vec![format!("Unavailable: {e}")],
        };
        if let Some(action) = party_ui::overview(
            ui,
            &party_ui::Overview {
                party: party_ui::Party::Customer,
                name: &person.name,
                active: person.active,
                currency: &person.currency,
                contact: &person.profile.contact_name,
                address: &person.profile.billing_address,
                balance_label: "OPEN BALANCE",
                balances: &balances,
            },
        ) {
            self.customer_action(action)
        }
    }

    fn customer_contacts(&mut self, ui: &mut egui::Ui, customer: &Customer) {
        let profile = &customer.profile;
        if [
            &profile.contact_name,
            &profile.company,
            &profile.phone,
            &profile.mobile,
            &profile.email,
            &profile.cc_email,
            &profile.website,
            &customer.contact,
            &profile.billing_address,
            &profile.shipping_address,
        ]
        .iter()
        .all(|value| value.trim().is_empty())
        {
            if empty_state(
                ui,
                "No contact details yet",
                "Add a contact name, phone, email or mailing address for this customer.",
                "Add Contact Details",
                true,
            ) {
                self.open_customer_editor(customer.id);
            }
            return;
        }
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
                        contact_details(ui, customer);
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(column, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(column);
                        customer_addresses(ui, customer);
                    },
                );
            });
        } else {
            contact_details(ui, customer);
            ui.add_space(12.0);
            customer_addresses(ui, customer);
        }
    }

    fn customer_transactions(&mut self, ui: &mut egui::Ui, id: i64, height: f32) {
        self.party_transactions(ui, party_ui::Party::Customer, id, height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Result<Desktop> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        Desktop::new(db, PathBuf::from("test.db"))
    }

    #[test]
    fn customer_sidebar_text_click_selects_the_customer_without_writing_data() -> Result<()> {
        let mut app = app()?;
        let first = crate::customer::save(
            &mut app.db,
            &Customer {
                name: "Alpha".into(),
                ..Default::default()
            },
        )?;
        let second = crate::customer::save(
            &mut app.db,
            &Customer {
                name: "Beta".into(),
                ..Default::default()
            },
        )?;
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
                    egui::CentralPanel::default().show(ctx, |ui| app.customer_center(ui));
                },
            )
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        assert_eq!(app.customer_ui.browser.selected, first);
        let row = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("customer_list_row", second))))
            .unwrap();
        let name = row.left_top() + egui::vec2(20.0, 14.0);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(name),
                    egui::Event::PointerButton {
                        pos: name,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.customer_ui.browser.selected, second);
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }

    #[test]
    fn empty_customer_sections_open_the_correct_editor_without_replacing_drafts() -> Result<()> {
        for (tab, title, editor_tab) in [(1, "No contact details yet", 0), (2, "No notes yet", 3)] {
            let mut app = app()?;
            let customer = Customer {
                name: "Buyer".into(),
                ..Default::default()
            };
            let id = crate::customer::save(&mut app.db, &customer)?;
            app.customer_ui.tab = tab;
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
                        egui::CentralPanel::default().show(ctx, |ui| app.customer_center(ui));
                    },
                )
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let output = frame(&mut app, vec![]);
            let text: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        Some(text.galley.job.text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(text.contains(&title));
            assert!(
                !text.contains(&"Contact") && !text.contains(&"Billing address"),
                "Empty fields should not leave orphan labels"
            );
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
            let form = app
                .customer_ui
                .form
                .as_mut()
                .expect("Empty-state action opens customer editor");
            assert_eq!(form.customer.id, id);
            assert_eq!(form.tab, editor_tab);
            form.customer.profile.notes = "Keep this draft".into();
            app.open_customer_editor_tab(id + 100, 1);
            let form = app.customer_ui.form.as_ref().unwrap();
            assert_eq!(form.customer.id, id);
            assert_eq!(form.tab, editor_tab);
            assert_eq!(form.customer.profile.notes, "Keep this draft");
            assert_eq!(app.db.total_changes(), changes);
        }
        Ok(())
    }

    #[test]
    fn customer_editor_is_singleton_and_failed_saves_preserve_profile_input() -> Result<()> {
        let mut app = app()?;
        app.open_customer_editor(0);
        app.customer_ui
            .form
            .as_mut()
            .unwrap()
            .customer
            .profile
            .notes = "Keep draft notes".into();
        app.open_customer_editor(999999);
        let mut form = app.customer_ui.form.take().unwrap();
        assert!(!app.save_customer_form(&mut form));
        assert_eq!(form.customer.profile.notes, "Keep draft notes");
        form.customer.name = "Buyer".into();
        form.customer.profile.email = "buyer@example.com".into();
        form.customer.profile.billing_address = "Main Street".into();
        assert!(app.save_customer_form(&mut form), "{}", form.error);
        let saved = crate::customer::load(&app.db, form.customer.id)?;
        assert_eq!(saved.profile.notes, "Keep draft notes");
        assert_eq!(saved.profile.billing_address, "Main Street");
        assert_eq!(saved.profile.email, "buyer@example.com");
        app.customer_ui.form = Some(form);
        app.request_business_close(windows::Kind::Customer);
        assert!(app.customer_ui.form.is_some());
        app.reset_after_restore()?;
        assert!(app.customer_ui.form.is_none());
        Ok(())
    }

    #[test]
    fn customer_tabs_fit_small_native_windows_without_writing_data() -> Result<()> {
        let mut app = app()?;
        app.open_customer_editor(0);
        app.customer_ui.form.as_mut().unwrap().error = "Validation message. ".repeat(20);
        let changes = app.db.total_changes();
        for tab in 0..4 {
            app.customer_ui.form.as_mut().unwrap().tab = tab;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
            for _ in 0..4 {
                let viewport_id = egui::ViewportId::from_hash_of("customer_layout_test");
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
                    |ctx| app.customer_dialog(ctx),
                );
            }
            let save = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("customer_save_rect")))
                .unwrap();
            assert!(
                screen.contains_rect(save),
                "Customer save control clipped on tab {tab}: {save:?}"
            );
        }
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }

    #[test]
    fn customer_workspace_keeps_details_below_header_and_inside_window() -> Result<()> {
        let mut app = app()?;
        let customer = crate::customer::Customer {
            name: "Buyer with a long customer display name".into(),
            profile: crate::customer::Profile {
                company: "Customer company information ".repeat(12),
                billing_address: "A long billing address ".repeat(12),
                notes: "Customer notes\n".repeat(100),
                ..Default::default()
            },
            ..Default::default()
        };
        let id = crate::customer::save(&mut app.db, &customer)?;
        let draft = crate::billing::DirectInvoice {
            request_key: "customer-workspace-layout".into(),
            customer: id,
            date: crate::today().to_string(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![crate::billing::DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "ea".into(),
                quantity: 1,
                unit_price: Some(10000),
                tax_code: Some(0),
            }],
        };
        let preview = crate::billing::direct_preview(&app.db, &draft)?;
        crate::billing::direct_confirm(&mut app.db, &draft, &preview)?;
        app.page = 1;
        app.customer_ui.browser.query = "Buyer".into();
        let changes = app.db.total_changes();
        for (width, height) in [
            (800.0, 600.0),
            (1200.0, 800.0),
            (1600.0, 900.0),
            (2000.0, 1000.0),
        ] {
            for view in 0..5 {
                app.customer_ui.browser.mode = usize::from(view == 4);
                app.customer_ui.tab = view.min(3);
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
                if view == 4 {
                    let viewport = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::new((
                                "sheet_viewport",
                                "customer_transactions",
                            )))
                        })
                        .unwrap();
                    assert!(
                        rect("customer_workspace_rect").contains_rect(viewport),
                        "All-customer transactions clip at {width}x{height}: {viewport:?}"
                    );
                    continue;
                }
                let title = rect("customer_title_rect");
                let actions = rect("customer_actions_rect");
                let tabs = rect("customer_tabs_rect");
                assert!(
                    screen.contains_rect(title),
                    "Title outside {width}x{height}: {title:?}"
                );
                assert!(
                    screen.contains_rect(actions),
                    "Actions outside {width}x{height}: {actions:?}"
                );
                assert!(
                    screen.contains_rect(tabs),
                    "Tabs outside {width}x{height}: {tabs:?}"
                );
                assert!(
                    actions.top() >= title.bottom(),
                    "Actions alongside title at {width}"
                );
                assert!(
                    tabs.top() >= actions.bottom(),
                    "Tabs alongside actions at {width}"
                );
                if let Some(sidebar) =
                    ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("customer_sidebar_rect")))
                {
                    let details = rect("customer_information_rect");
                    assert!(
                        (180.0..=240.1).contains(&sidebar.width()),
                        "Sidebar collapsed: {sidebar:?}"
                    );
                    assert!(
                        details.left() >= sidebar.right(),
                        "Overlapping panes at {width}"
                    );
                    assert!(
                        screen.contains_rect(details),
                        "Details overflow at {width}x{height}: {details:?}"
                    );
                }
                if view == 0 {
                    let rows_viewport = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::new((
                                "sheet_rows_viewport",
                                "customer_transactions",
                            )))
                        })
                        .unwrap();
                    let workspace = rect("customer_workspace_rect");
                    assert!(rows_viewport.bottom() <= workspace.bottom() + 1.0, "Transaction rows clipped at {width}x{height}: {rows_viewport:?}, workspace {workspace:?}");
                    let viewport = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::new((
                                "sheet_viewport",
                                "customer_transactions",
                            )))
                        })
                        .unwrap();
                    assert!(
                        rect("customer_workspace_rect").contains_rect(viewport),
                        "Transaction viewport clips at {width}x{height}: {viewport:?}, workspace {:?}", rect("customer_workspace_rect")
                    );
                    let header = ctx
                        .data(|d| {
                            d.get_temp::<egui::Rect>(egui::Id::new((
                                "sheet_header",
                                "customer_transactions",
                            )))
                        })
                        .unwrap();
                    assert!(
                        header.top() >= tabs.bottom(),
                        "Transactions alongside tabs at {width}"
                    );
                    assert!(
                        header.top() < height - 100.0,
                        "Transactions below window at {width}"
                    );
                    assert!(
                        header.left() >= rect("customer_workspace_rect").left() - 1.0
                            && header.left() < width - 100.0,
                        "Transactions outside detail column at {width}: {header:?}"
                    );
                    if width >= 1600.0 {
                        assert!(
                            screen.contains_rect(header),
                            "Table outside wide window: {header:?}"
                        );
                        let receive = ctx
                            .data(|d| {
                                d.get_temp::<egui::Rect>(egui::Id::new((
                                    "sheet_cell",
                                    "customer_transactions",
                                    6_usize,
                                )))
                            })
                            .unwrap();
                        assert!(
                            screen.contains_rect(receive),
                            "Receipt action outside window: {receive:?}"
                        );
                    }
                }
            }
        }
        assert_eq!(app.db.total_changes(), changes);
        Ok(())
    }

    #[test]
    fn customer_center_views_are_read_only_and_invoice_drilldown_preserves_workspace() -> Result<()>
    {
        let mut app = app()?;
        let customer = crate::customer::Customer {
            name: "Buyer".into(),
            ..Default::default()
        };
        let id = crate::customer::save(&mut app.db, &customer)?;
        let draft = crate::billing::DirectInvoice {
            request_key: "customer-center".into(),
            customer: id,
            date: crate::today().to_string(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![crate::billing::DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "ea".into(),
                quantity: 1,
                unit_price: Some(10000),
                tax_code: Some(0),
            }],
        };
        let preview = crate::billing::direct_preview(&app.db, &draft)?;
        let invoice = crate::billing::direct_confirm(&mut app.db, &draft, &preview)?;
        crate::billing::add_payment(
            &mut app.db,
            invoice,
            &crate::today().to_string(),
            "25.00",
            "Cash",
            "",
        )?;
        app.page = 1;
        let changes = app.db.total_changes();
        for mode in 0..2 {
            app.customer_ui.browser.mode = mode;
            for tab in 0..4 {
                app.customer_ui.tab = tab;
                let ctx = egui::Context::default();
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.customer_center(ui));
                    },
                );
                assert!(!app.error, "Customer center read failed: {}", app.message);
            }
        }
        assert_eq!(app.db.total_changes(), changes);
        app.customer_ui.browser.query = "Buyer".into();
        app.open_invoice(invoice);
        assert_eq!(app.page, 1);
        assert_eq!(app.customer_ui.browser.query, "Buyer");
        Ok(())
    }
}
