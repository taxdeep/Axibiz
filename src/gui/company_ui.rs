//! Company preferences share the regular form, section and footer primitives.
use super::*;
use crate::settings::CompanyProfile;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    people_ui::section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        ui.add_space(6.0);
        content(ui);
    });
    ui.add_space(8.0);
}
fn address(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_rows(3)
            .desired_width(ui.available_width()),
    );
}
fn legal(ui: &mut egui::Ui, p: &mut CompanyProfile, contact_address: &str, country: &str) {
    section(ui, "Legal Information", |ui| {
        field(ui, "Legal name", &mut p.legal_name);
        ui.checkbox(
            &mut p.legal_same_as_contact,
            "Legal address is the contact address",
        );
        if p.legal_same_as_contact {
            ui.label(if contact_address.is_empty() {
                "No contact address provided"
            } else {
                contact_address
            });
            if !country.is_empty() {
                ui.label(country);
            }
            ui.small("Your separate legal address is retained when this option is cleared.");
        } else {
            address(ui, "Legal address", &mut p.legal_address);
            field(ui, "City", &mut p.legal_city);
            field(ui, "Province / State", &mut p.legal_province);
            field(ui, "Postal / ZIP code", &mut p.legal_postal_code);
            field(ui, "Legal country", &mut p.legal_country);
        }
    });
}
fn identification(ui: &mut egui::Ui, p: &mut CompanyProfile, legacy: &mut String) {
    section(ui, "Company Identification", |ui| {
        field(ui, "Business number", &mut p.business_number);
        field(ui, "GST/HST account", &mut p.gst_hst_number);
        ui.small(
            "Optional Canadian identifiers: 9-digit BN; GST/HST account such as 123456789RT0001.",
        );
        field(ui, "QST registration", &mut p.qst_number);
        ui.small("Enter the QST registration reference as issued.");
        field(ui, "Tax reference", legacy);
        ui.small("These are company identifiers. Tax collection and account mappings remain in Sales Tax Setup.");
    });
}
fn month(ui: &mut egui::Ui, label: &str, value: &mut Option<u8>) {
    ui.horizontal(|ui| {
        ui.add_sized([170.0, theme::CONTROL_HEIGHT], egui::Label::new(label));
        egui::ComboBox::from_id_salt(label)
            .width(150.0)
            .selected_text(
                value
                    .and_then(|m| MONTHS.get(usize::from(m).wrapping_sub(1)))
                    .copied()
                    .unwrap_or("Not specified"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(value, None, "Not specified");
                for (index, name) in MONTHS.iter().enumerate() {
                    ui.selectable_value(value, Some(index as u8 + 1), *name);
                }
            });
    });
}

impl Desktop {
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        let dirty = self.settings != self.saved_settings;
        let height = (ui.available_height() - 88.0).max(80.0);
        egui::ScrollArea::vertical().id_salt("company_settings_body").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            section(ui, "Contact Information", |ui| {
                field(ui, "Company name *", &mut self.settings.company_name);
                let count = if ui.available_width() >= 800.0 { 2 } else { 1 };
                ui.columns(count, |columns| {
                    address(&mut columns[0], "Contact address", &mut self.settings.address);
                    field(&mut columns[0], "Country", &mut self.settings.profile.country);
                    let col = &mut columns[count - 1];
                    field(col, "Phone", &mut self.settings.profile.phone);
                    field(col, "Fax", &mut self.settings.profile.fax);
                    field(col, "Email", &mut self.settings.profile.email);
                    field(col, "Website", &mut self.settings.profile.website);
                    field(col, "Other contact", &mut self.settings.contact);
                });
            });
            let country = self.settings.profile.country.clone();
            if ui.available_width() >= 900.0 {
                ui.columns(2, |columns| {
                    legal(&mut columns[0], &mut self.settings.profile, &self.settings.address, &country);
                    identification(&mut columns[1], &mut self.settings.profile, &mut self.settings.tax_number);
                });
            } else {
                legal(ui, &mut self.settings.profile, &self.settings.address, &country);
                identification(ui, &mut self.settings.profile, &mut self.settings.tax_number);
            }
            section(ui, "Reporting Preferences", |ui| {
                month(ui, "Fiscal year starts in", &mut self.settings.profile.fiscal_year_start_month);
                month(ui, "Tax year starts in", &mut self.settings.profile.tax_year_start_month);
                ui.horizontal(|ui| {
                    ui.add_sized([170.0, theme::CONTROL_HEIGHT], egui::Label::new("Income tax form"));
                    egui::ComboBox::from_id_salt("company_tax_form").width(150.0).selected_text(if self.settings.profile.income_tax_form.is_empty() { "Not specified" } else { &self.settings.profile.income_tax_form }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.profile.income_tax_form, String::new(), "Not specified");
                        for form in ["T2", "T1", "T5013", "Other"] {
                            ui.selectable_value(&mut self.settings.profile.income_tax_form, form.into(), form);
                        }
                    });
                });
                ui.small("Reference preferences only. They do not change posted dates, closing dates or sales tax periods, and do not file a tax return.");
            });
            section(ui, "Company Preferences", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Default currency");
                    currency_picker(ui, "company_currency", &mut self.settings.currency);
                });
                ui.small("Applies to new tasks and prices. Existing amounts and currencies are retained.");
                ui.collapsing("Storage and Maintenance", |ui| {
                    if let Ok(Some(id)) = crate::storage::company_id(&self.db) { ui.small(format!("Company ID: {id}")); }
                    ui.collapsing("Legacy Currency Labels", |ui| {
                        ui.label("Apply only if all unspecified legacy records originally used the same currency. Amounts are not converted.");
                        currency_picker(ui, "legacy_currency", &mut self.target);
                        if ui.button("Review Legacy Currency Labels").clicked() { self.review_legacy_currency(); }
                    });
                });
            });
        });
        controls::footer(
            ui,
            Some(if dirty {
                "Unsaved changes. Issued invoices retain their original company information."
            } else {
                "Company information is used for new invoices. Optional fields may be left blank."
            }),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let save = controls::primary(ui, "Save", dirty);
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("company_save_rect"), save.rect));
                    if save.clicked() {
                        self.save_company_settings(false);
                    }
                    if ui.button("Save & Close").clicked() {
                        self.save_company_settings(true);
                    }
                    ui.add_enabled_ui(dirty, |ui| {
                        ui.menu_button("Revert Changes", |ui| {
                            ui.label("Discard unsaved company changes?");
                            if ui.button("Discard Changes").clicked() {
                                self.settings = self.saved_settings.clone();
                                ui.close();
                            }
                        });
                    });
                });
            },
        );
    }
    fn save_company_settings(&mut self, close: bool) {
        match self
            .settings
            .save(&self.db)
            .and_then(|()| Settings::load(&self.db))
        {
            Ok(saved) => {
                self.settings = saved.clone();
                self.saved_settings = saved;
                if !self.new_task_open {
                    self.task_currency = self.settings.currency.clone();
                }
                self.error = false;
                self.message = "Company settings saved.".into();
                if close {
                    self.page = 0;
                    self.list_query.clear();
                }
            }
            Err(error) => {
                self.error = true;
                self.message = format!("Unable to save company settings: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_draft_survives_navigation_and_failed_save_then_saves_normalized() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.navigate(5);
        app.settings.company_name = "Example Ltd".into();
        app.settings.profile.email = "invalid".into();
        app.settings.profile.legal_name = "Example Legal Ltd".into();
        app.navigate(1);
        assert!(app.has_business_editors());
        app.navigate(5);
        app.save_company_settings(true);
        assert!(app.error);
        assert_eq!(app.page, 5);
        assert_eq!(app.settings.profile.legal_name, "Example Legal Ltd");
        assert_eq!(Settings::load(&app.db)?, app.saved_settings);
        app.settings.profile.email = " billing@example.com ".into();
        app.save_company_settings(true);
        assert!(!app.error);
        assert_eq!(app.page, 0);
        assert_eq!(app.settings.profile.email, "billing@example.com");
        assert_eq!(Settings::load(&app.db)?, app.settings);
        assert_eq!(app.saved_settings, app.settings);
        assert!(!app.has_business_editors());
        Ok(())
    }
}
