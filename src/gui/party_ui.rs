//! Shared Customer/Vendor workspace contract. Domain actions stay in their modules.
use super::*;

#[derive(Clone, Default)]
pub(super) struct Browser {
    pub selected: i64,
    pub status: usize,
    pub mode: usize,
    pub query: String,
    pub transaction_query: String,
    pub master_width: f32,
    pub type_filter: usize,
    pub date_filter: usize,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Party {
    Customer,
    Vendor,
}
impl Party {
    pub fn scope(self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Vendor => "vendor",
        }
    }
    pub fn plural(self) -> &'static str {
        match self {
            Self::Customer => "Customers",
            Self::Vendor => "Vendors",
        }
    }
    pub fn new_label(self) -> &'static str {
        match self {
            Self::Customer => "New Customer",
            Self::Vendor => "New Vendor",
        }
    }
    pub fn primary(self) -> &'static str {
        match self {
            Self::Customer => "New Invoice",
            Self::Vendor => "Enter Bill",
        }
    }
    pub fn secondary(self) -> &'static str {
        match self {
            Self::Customer => "New Task",
            Self::Vendor => "Pay Bills",
        }
    }
    pub fn edit(self) -> &'static str {
        match self {
            Self::Customer => "Edit Customer",
            Self::Vendor => "Edit Vendor",
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    New,
    Primary,
    Secondary,
    Edit,
    Export,
}
pub(super) struct Row {
    pub id: i64,
    pub name: String,
    pub amount: String,
}
pub(super) fn matches(values: &[String], query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || values.iter().any(|v| v.to_lowercase().contains(&query))
}

pub(super) fn workspace(
    ui: &mut egui::Ui,
    party: Party,
    state: &mut Browser,
    rows: &[Row],
    details: impl FnOnce(&mut egui::Ui, i64, f32, bool),
) -> Option<Action> {
    invoice_sheet::compact(ui);
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.mode, 0, party.plural());
        ui.selectable_value(&mut state.mode, 1, "Transactions");
    });
    ui.horizontal_wrapped(|ui| {
        if icons::action(ui, icons::Icon::Add, party.new_label(), true).clicked() {
            action = Some(Action::New);
        }
        icons::menu(ui, icons::Icon::Add, "New Transaction", |ui| {
            if ui.button(party.primary()).clicked() {
                action = Some(Action::Primary);
                ui.close();
            }
            if ui.button(party.secondary()).clicked() {
                action = Some(Action::Secondary);
                ui.close();
            }
        });
        icons::menu(ui, icons::Icon::More, "More", |ui| {
            if icons::action(ui, icons::Icon::Export, "Export CSV", true).clicked() {
                action = Some(Action::Export);
                ui.close();
            }
        });
    });
    ui.separator();
    let bottom = ui
        .max_rect()
        .bottom()
        .min(ui.clip_rect().bottom())
        .min(ui.ctx().content_rect().bottom() - 16.0)
        - 8.0;
    let height = (bottom - ui.cursor().min.y).max(120.0);
    if state.mode == 1 {
        details(ui, 0, height, true);
        return action;
    }
    if !rows.iter().any(|r| r.id == state.selected) {
        state.selected = rows.first().map(|r| r.id).unwrap_or(0)
    }
    let width = invoice_sheet::finite_width(ui);
    let filters = |ui: &mut egui::Ui, state: &mut Browser| {
        egui::ComboBox::from_id_salt((party.scope(), "status"))
            .selected_text(["Active", "All", "Inactive"][state.status])
            .show_ui(ui, |ui| {
                for (i, label) in ["Active", "All", "Inactive"].iter().enumerate() {
                    ui.selectable_value(&mut state.status, i, *label);
                }
            });
        controls::search(
            ui,
            (party.scope(), "search"),
            &mut state.query,
            300.0,
            &format!("Search {}...", party.plural().to_lowercase()),
        );
    };
    if width >= 1050.0 {
        let sidebar = if state.master_width > 0.0 {
            state.master_width
        } else {
            220.0_f32
        }
        .clamp(190.0, 280.0)
        .min(width * 0.34);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let master = ui.allocate_ui_with_layout(
                egui::vec2(sidebar, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(sidebar);
                    ui.set_min_height(height);
                    filters(ui, state);
                    ui.horizontal(|ui| {
                        ui.small("NAME");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.small(if party == Party::Customer {
                                "OPEN"
                            } else {
                                "RECORDED"
                            });
                        });
                    });
                    egui::ScrollArea::vertical()
                        .id_salt((party.scope(), "list"))
                        .max_height((height - 106.0).max(50.0))
                        .show_rows(ui, 29.0, rows.len(), |ui, range| {
                            for row in &rows[range] {
                                if people_ui::list_item(
                                    ui,
                                    party.scope(),
                                    row.id,
                                    &row.name,
                                    &row.amount,
                                    row.id == state.selected,
                                )
                                .clicked()
                                {
                                    state.selected = row.id
                                }
                            }
                        });
                    ui.small(format!(
                        "{} {}",
                        rows.len(),
                        if rows.len() == 1 {
                            party.scope()
                        } else {
                            party.plural()
                        }
                    ));
                },
            );
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(5.0, height), egui::Sense::drag());
            ui.painter().vline(
                rect.center().x,
                rect.y_range(),
                egui::Stroke::new(1.0_f32, theme::BORDER),
            );
            if response.dragged() {
                state.master_width = (sidebar + response.drag_delta().x).clamp(190.0, 280.0)
            }
            let detail = ui.allocate_ui_with_layout(
                egui::vec2(width - sidebar - 17.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width - sidebar - 17.0);
                    ui.set_min_height(height);
                    details(ui, state.selected, height, false);
                },
            );
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new(format!("{}_sidebar_rect", party.scope())),
                    master.response.rect,
                );
                d.insert_temp(
                    egui::Id::new(format!("{}_information_rect", party.scope())),
                    detail.response.rect,
                );
            });
            #[cfg(not(test))]
            let _ = (master, detail);
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt((party.scope(), "narrow_selection"))
                .width(220.0)
                .truncate()
                .selected_text(
                    rows.iter()
                        .find(|r| r.id == state.selected)
                        .map(|r| r.name.as_str())
                        .unwrap_or("Select a record"),
                )
                .show_ui(ui, |ui| {
                    for row in rows {
                        ui.selectable_value(&mut state.selected, row.id, &row.name);
                    }
                });
            filters(ui, state);
        });
        details(
            ui,
            state.selected,
            (bottom - ui.cursor().min.y).max(100.0),
            false,
        );
    }
    action
}

pub(super) struct Overview<'a> {
    pub party: Party,
    pub name: &'a str,
    pub active: bool,
    pub currency: &'a str,
    pub contact: &'a str,
    pub address: &'a str,
    pub balance_label: &'a str,
    pub balances: &'a [String],
}
pub(super) fn overview(ui: &mut egui::Ui, model: &Overview<'_>) -> Option<Action> {
    let mut action = None;
    let width = invoice_sheet::finite_width(ui);
    let summary_width = if width >= 550.0 { 185.0 } else { 145.0 };
    ui.horizontal_top(|ui| {
        let title_width = (width - summary_width - 6.0).max(120.0);
        let title = ui.allocate_ui_with_layout(
            egui::vec2(title_width, 44.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(title_width);
                let name = ui
                    .add(egui::Label::new(RichText::new(model.name).size(20.0)).truncate())
                    .on_hover_text(model.name);
                ui.small(format!(
                    "{} | {}",
                    if model.active { "Active" } else { "Inactive" },
                    model.currency
                ));
                name
            },
        );
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new(format!("{}_title_rect", model.party.scope())),
                title.inner.rect,
            )
        });
        #[cfg(not(test))]
        let _ = title;
        let summary = ui.allocate_ui_with_layout(
            egui::vec2(summary_width, 44.0),
            egui::Layout::top_down(egui::Align::RIGHT),
            |ui| {
                ui.set_width(summary_width);
                ui.small(model.balance_label);
                let text = model.balances.join(" / ");
                ui.add(egui::Label::new(text.clone()).truncate())
                    .on_hover_text(format!("{}\n{}", model.balance_label, text))
            },
        );
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new(format!("{}_balance_rect", model.party.scope())),
                summary.inner.rect,
            )
        });
        #[cfg(not(test))]
        let _ = summary;
    });
    let actions = ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                model.active,
                egui::Button::new(RichText::new(model.party.primary()).color(Color32::WHITE))
                    .fill(theme::GREEN),
            )
            .clicked()
        {
            action = Some(Action::Primary)
        }
        if ui
            .add_enabled(
                model.active || model.party == Party::Vendor,
                egui::Button::new(model.party.secondary()),
            )
            .clicked()
        {
            action = Some(Action::Secondary)
        }
        if icons::action(ui, icons::Icon::Edit, model.party.edit(), true).clicked() {
            action = Some(Action::Edit)
        }
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new(format!("{}_actions_rect", model.party.scope())),
            actions.response.rect,
        )
    });
    #[cfg(not(test))]
    let _ = actions;
    for (label, value) in [
        ("Contact", model.contact),
        (
            if model.party == Party::Customer {
                "Bill To"
            } else {
                "Billed From"
            },
            model.address,
        ),
    ] {
        if value.trim().is_empty() {
            continue;
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [74.0, 18.0],
                egui::Label::new(RichText::new(label).small().color(theme::MUTED)),
            );
            ui.add(egui::Label::new(value.replace('\n', ", ")).truncate())
                .on_hover_text(value);
        });
    }
    ui.separator();
    action
}

impl Desktop {
    fn party_browser(&mut self, party: Party) -> &mut Browser {
        match party {
            Party::Customer => &mut self.customer_ui.browser,
            Party::Vendor => &mut self.vendor_ui.browser,
        }
    }
    pub(super) fn party_center(&mut self, ui: &mut egui::Ui, party: Party) {
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new(format!("{}_workspace_rect", party.scope())),
                ui.max_rect(),
            )
        });
        let data = match self.party_cache.load(&self.db, party) {
            Ok(data) => data,
            Err(e) => {
                ui.colored_label(
                    Color32::DARK_RED,
                    format!("Cannot load {} profiles/balances: {e:#}", party.scope()),
                );
                return;
            }
        };
        let mut browser = std::mem::take(self.party_browser(party));
        let rows = data
            .records
            .iter()
            .filter(|r| {
                (browser.status == 1 || r.active == (browser.status == 0))
                    && matches(&r.search, &browser.query)
            })
            .map(|r| Row {
                id: r.id,
                name: r.name.clone(),
                amount: party_data::summary(data.amounts.get(&r.id), &r.currency),
            })
            .collect::<Vec<_>>();
        *self.party_browser(party) = browser.clone();
        let action = workspace(ui, party, &mut browser, &rows, |ui, id, height, global| {
            self.party_browser(party).selected = id;
            self.party_browser(party).mode = usize::from(global);
            if global {
                self.party_transactions(ui, party, 0, height)
            } else {
                match party {
                    Party::Customer => self.customer_information(ui, height),
                    Party::Vendor => self.vendor_information(ui, height),
                }
            }
        });
        let detail = self.party_browser(party);
        browser.type_filter = detail.type_filter;
        browser.date_filter = detail.date_filter;
        browser.transaction_query = std::mem::take(&mut detail.transaction_query);
        *detail = browser;
        if let Some(action) = action {
            match party {
                Party::Customer => self.customer_action(action),
                Party::Vendor => self.vendor_action(action),
            }
        }
    }
    pub(super) fn export_party_csv(
        &mut self,
        name: &str,
        headers: &[&str],
        rows: Vec<Vec<crate::csv::Cell>>,
    ) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(path, crate::csv::encode(headers, &rows)) {
            Ok(()) => {
                self.message = "Report exported.".into();
                self.error = false;
            }
            Err(e) => {
                self.message = format!("Cannot export report: {e}");
                self.error = true;
            }
        }
    }
}
