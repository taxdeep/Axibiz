//! One report/filter/export contract for customer and supplier events.
use super::{
    party_ui::{self, Party},
    *,
};

fn prefix(index: usize) -> String {
    match index {
        1 => crate::today().format("%Y").to_string(),
        2 => crate::today().format("%Y-%m").to_string(),
        _ => String::new(),
    }
}
impl Desktop {
    pub(super) fn party_transactions(
        &mut self,
        ui: &mut egui::Ui,
        party: Party,
        id: i64,
        height: f32,
    ) {
        let top = ui.cursor().min.y;
        let browser = match party {
            Party::Customer => &mut self.customer_ui.browser,
            Party::Vendor => &mut self.vendor_ui.browser,
        };
        ui.horizontal_wrapped(|ui| {
            let count = usize::from(browser.type_filter != 0)
                + usize::from(browser.date_filter != 0)
                + usize::from(!browser.transaction_query.trim().is_empty());
            icons::label(
                ui,
                icons::Icon::Filter,
                &if count > 0 {
                    format!("Filters ({count})")
                } else {
                    "Filters".into()
                },
            );
            egui::ComboBox::from_id_salt((party.scope(), "transaction_type"))
                .width(130.0)
                .selected_text(
                    [
                        "All transactions",
                        if party == Party::Customer {
                            "Invoices"
                        } else {
                            "Bills"
                        },
                        "Payments",
                        "Open documents",
                    ][browser.type_filter],
                )
                .show_ui(ui, |ui| {
                    for (i, name) in [
                        "All transactions",
                        if party == Party::Customer {
                            "Invoices"
                        } else {
                            "Bills"
                        },
                        "Payments",
                        "Open documents",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut browser.type_filter, i, *name);
                    }
                });
            egui::ComboBox::from_id_salt((party.scope(), "transaction_date"))
                .width(112.0)
                .selected_text(["All dates", "This year", "This month"][browser.date_filter])
                .show_ui(ui, |ui| {
                    for (i, name) in ["All dates", "This year", "This month"].iter().enumerate() {
                        ui.selectable_value(&mut browser.date_filter, i, *name);
                    }
                });
            controls::search(
                ui,
                (party.scope(), "transaction_search"),
                &mut browser.transaction_query,
                300.0,
                "Search number, reference or memo...",
            );
        });
        let filter = browser.type_filter;
        let date = prefix(browser.date_filter);
        let query = browser.transaction_query.clone();
        let rows = match self.party_cache.events(&self.db, party, id) {
            Ok(rows) => rows
                .into_iter()
                .filter(|r| r.matches(filter, &date, &query))
                .collect::<Vec<_>>(),
            Err(e) => {
                ui.colored_label(
                    Color32::DARK_RED,
                    format!("Cannot load transactions: {e:#}"),
                );
                return;
            }
        };
        let remaining = (height - (ui.cursor().min.y - top) - 42.0).max(84.0);
        if rows.is_empty() {
            egui::Frame::new()
                .fill(Color32::WHITE)
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 12.0).max(80.0));
                    ui.set_min_height((remaining - 32.0).max(50.0));
                    let filtered = filter != 0 || !date.is_empty() || !query.trim().is_empty();
                    ui.strong(if filtered {
                        "No matching transactions"
                    } else {
                        "No transactions yet"
                    });
                    ui.label(if filtered {
                        "Change or clear the transaction filters. Your profile search is separate."
                    } else {
                        "New documents and recorded payments will appear here."
                    });
                    if filtered && ui.button("Clear Filters").clicked() {
                        let browser = match party {
                            Party::Customer => &mut self.customer_ui.browser,
                            Party::Vendor => &mut self.vendor_ui.browser,
                        };
                        browser.type_filter = 0;
                        browser.date_filter = 0;
                        browser.transaction_query.clear();
                    }
                    if !filtered && ui.button(party.primary()).clicked() {
                        match party {
                            Party::Customer => self.customer_action(party_ui::Action::Primary),
                            Party::Vendor => self.vendor_action(party_ui::Action::Primary),
                        }
                    }
                });
            return;
        }
        let width = invoice_sheet::finite_width(ui).max(620.0);
        let sheet = if party == Party::Customer {
            "customer_transactions"
        } else {
            "vendor_expenses"
        };
        invoice_sheet::register_columns(
            ui,
            sheet,
            remaining,
            &[
                "TYPE",
                "NUMBER",
                "DATE",
                "REFERENCE / MEMO",
                "CURR.",
                "AMOUNT",
                "OPEN",
                "ACTION",
            ],
            &[70.0, 85.0, 88.0, width - 548.0, 55.0, 90.0, 90.0, 70.0],
            rows.len(),
            |ui, index, column| {
                let row = &rows[index];
                match column {
                    0 => {
                        ui.add(egui::Label::new(&row.kind).truncate())
                            .on_hover_text(&row.kind);
                    }
                    1 => {
                        if ui
                            .button(&row.number)
                            .on_hover_text(if row.is_document() {
                                "Open document"
                            } else {
                                "View payment journal"
                            })
                            .clicked()
                        {
                            if row.is_document() {
                                match party {
                                    Party::Customer => self.open_invoice(row.document),
                                    Party::Vendor => self.open_accounting_form(row.document),
                                }
                            } else {
                                let source = if party == Party::Customer {
                                    "payment"
                                } else {
                                    "vendor_payment"
                                };
                                match self.db.query_row(
                                    "SELECT entry FROM gl_sources WHERE kind=? AND record=?",
                                    rusqlite::params![source, row.record],
                                    |r| r.get::<_, i64>(0),
                                ) {
                                    Ok(entry) => self.open_journal(entry),
                                    Err(e) => {
                                        self.message = format!(
                                            "This historical payment has no journal source: {e}"
                                        );
                                        self.error = true;
                                    }
                                }
                            }
                        }
                    }
                    2 => {
                        ui.label(&row.date);
                    }
                    3 => {
                        let memo = row.memo();
                        ui.add(egui::Label::new(&memo).truncate())
                            .on_hover_text(memo);
                    }
                    4 => {
                        ui.label(&row.currency);
                    }
                    5 => invoice_sheet::amount(ui, &crate::format_money(row.amount)),
                    6 => {
                        if row.is_document() {
                            invoice_sheet::amount(
                                ui,
                                &row.remaining
                                    .map(crate::format_money)
                                    .unwrap_or_else(|| "Review".into()),
                            );
                        }
                    }
                    _ => {
                        if row.is_document()
                            && row.remaining.is_some_and(|v| v > 0)
                            && ui
                                .button(if party == Party::Customer {
                                    "Receive"
                                } else {
                                    "Pay"
                                })
                                .clicked()
                        {
                            match party {
                                Party::Customer => self.open_payment(row.document),
                                Party::Vendor => self.open_pay_bills(row.owner, Some(row.document)),
                            }
                        }
                    }
                }
            },
        );
        ui.small(format!(
            "{} transactions | Amounts remain in their source currency",
            rows.len()
        ));
    }
    pub(super) fn export_party_transactions(&mut self, party: Party) {
        let browser = match party {
            Party::Customer => &self.customer_ui.browser,
            Party::Vendor => &self.vendor_ui.browser,
        };
        let owner = if browser.mode == 1 {
            0
        } else {
            browser.selected
        };
        let filter = browser.type_filter;
        let date = prefix(browser.date_filter);
        let query = browser.transaction_query.clone();
        let rows = match self.party_cache.events(&self.db, party, owner) {
            Ok(rows) => rows,
            Err(e) => {
                self.message = format!("Cannot export transactions: {e:#}");
                self.error = true;
                return;
            }
        };
        let rows = rows
            .into_iter()
            .filter(|r| r.matches(filter, &date, &query))
            .map(|r| {
                use crate::csv::Cell::{Amount, Text};
                vec![
                    Text(r.kind),
                    Text(r.number),
                    Text(r.date),
                    Text(r.party),
                    Text(r.reference),
                    Text(r.memo),
                    Text(r.currency),
                    Amount(r.amount),
                    r.remaining
                        .map(Amount)
                        .unwrap_or_else(|| Text(String::new())),
                ]
            })
            .collect();
        self.export_party_csv(
            if party == Party::Customer {
                "customer-transactions.csv"
            } else {
                "vendor-transactions.csv"
            },
            &[
                "Type",
                "Number",
                "Date",
                "Party",
                "Reference",
                "Memo",
                "Currency",
                "Amount",
                "Open",
            ],
            rows,
        );
    }
}
