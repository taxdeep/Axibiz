mod accounting_ui;
mod ap_ui;
mod app_settings_ui;
mod auth_ui;
mod billing_ui;
mod commands;
mod company_ui;
mod compliance_ui;
mod controls;
mod customer_ui;
#[cfg(test)]
mod design_preview;
mod direct_invoice_ui;
mod expense_ui;
mod gl_ui;
mod icons;
mod invoice_sheet;
mod invoice_template_ui;
mod invoice_workbench;
mod menus;
mod navigation;
mod party_data;
mod party_transactions;
mod party_ui;
mod payment_ui;
mod people_ui;
mod reconciliation_ui;
mod task_create_ui;
mod task_editor_ui;
mod task_list_ui;
mod tax_ui;
mod theme;
mod vendor_ui;
mod windows;
use crate::{Command, Settings};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText};
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

const CURRENCIES: [&str; 8] = ["CAD", "USD", "CNY", "EUR", "GBP", "HKD", "AUD", "SGD"];
use commands::PAGES;
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let width = invoice_sheet::finite_width(ui);
    if width < 440.0 {
        ui.label(label);
        ui.add_sized(
            [width, theme::CONTROL_HEIGHT],
            egui::TextEdit::singleline(value),
        );
    } else {
        ui.horizontal(|ui| {
            ui.add_sized(
                [112.0, theme::CONTROL_HEIGHT],
                egui::Label::new(RichText::new(label).color(theme::MUTED)),
            );
            ui.add_sized(
                [
                    (ui.available_width() - 6.0).clamp(100.0, 320.0),
                    theme::CONTROL_HEIGHT,
                ],
                egui::TextEdit::singleline(value),
            );
        });
    }
}
fn currency_picker(ui: &mut egui::Ui, id: &str, value: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for code in CURRENCIES {
                ui.selectable_value(value, code.to_owned(), code);
            }
        });
}
fn edit_buttons(ui: &mut egui::Ui, enabled: bool) -> (bool, bool, bool) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let save_close = controls::primary(ui, "Save & Close", enabled).clicked();
        let save = ui.add_enabled(enabled, egui::Button::new("Save")).clicked();
        let cancel = ui.button("Cancel").clicked();
        (save, save_close, cancel)
    })
    .inner
}
fn picker(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut i64,
    rows: &[Vec<String>],
) -> crate::autocomplete::Outcome {
    picker_with_width(ui, id, value, rows, 240.0)
}
fn picker_with_width(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut i64,
    rows: &[Vec<String>],
    width: f32,
) -> crate::autocomplete::Outcome {
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(("picker_id", id)), ui.make_persistent_id(id)));
    let choices: Vec<_> = rows
        .iter()
        .filter_map(|r| {
            Some(crate::autocomplete::Choice {
                id: r.first()?.parse().ok()?,
                name: r.get(1)?.clone(),
                detail: r.get(2).cloned().unwrap_or_else(|| {
                    r.get(1)
                        .and_then(|s| s.split_once(" · ").map(|(_, name)| name.to_owned()))
                        .unwrap_or_default()
                }),
            })
        })
        .collect();
    crate::autocomplete::show(ui, ui.make_persistent_id(id), value, &choices, width, false)
}
fn query(c: &Connection, sql: &str) -> Result<Vec<Vec<String>>> {
    let mut stmt = c.prepare(sql)?;
    let cols = stmt.column_count();
    let mut rows = stmt.query([])?;
    let mut result = Vec::new();
    while let Some(r) = rows.next()? {
        result.push(
            (0..cols)
                .map(|i| {
                    Ok(match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Integer(v) => v.to_string(),
                        rusqlite::types::ValueRef::Text(v) => {
                            String::from_utf8_lossy(v).into_owned()
                        }
                        rusqlite::types::ValueRef::Real(v) => v.to_string(),
                        rusqlite::types::ValueRef::Blob(v) => format!("[{} bytes]", v.len()),
                        rusqlite::types::ValueRef::Null => String::new(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        );
    }
    Ok(result)
}
fn table(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    selected: &mut i64,
) -> bool {
    controls::register(ui, id, headers, rows, selected, false).0
}
fn creation_label(page: usize) -> Option<&'static str> {
    PAGES.get(page).and_then(|p| p.create)
}
#[derive(Default)]
struct NewRecord {
    page: usize,
    name: String,
    vendor: i64,
    unit: String,
    cost: String,
    price: String,
    currency: String,
    error: String,
    baseline: Option<String>,
    close_guard: controls::CloseGuard,
}
#[derive(Default)]
struct PriceEdit {
    id: i64,
    name: String,
    unit: String,
    cost: String,
    price: String,
    currency: String,
    error: String,
    baseline: Option<String>,
    close_guard: controls::CloseGuard,
}
impl PriceEdit {
    fn fingerprint(&self) -> String {
        format!("{:?}", (&self.price, &self.currency))
    }
}
impl NewRecord {
    fn fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                &self.name,
                self.vendor,
                &self.unit,
                &self.cost,
                &self.price,
                &self.currency
            )
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
struct TaskLine {
    tax_code: Option<i64>,
    key: u64,
    price_id: i64,
    quantity: i64,
    price_override: Option<String>,
    input_pending: bool,
}
impl Default for TaskLine {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            tax_code: None,
            key: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            price_id: 0,
            quantity: 1,
            price_override: None,
            input_pending: false,
        }
    }
}
#[derive(Clone)]
struct CatalogItem {
    id: i64,
    name: String,
    unit: String,
    currency: String,
    price: i64,
    vendor: String,
}
impl CatalogItem {
    fn matches(&self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        format!("{} {} {}", self.id, self.name, self.vendor)
            .to_lowercase()
            .contains(&search)
    }
}
pub struct Desktop {
    windows: windows::Windows,
    db: Connection,
    accounting: accounting_ui::AccountingUi,
    menus: menus::Menus,
    billing_ui: billing_ui::BillingUi,
    payment: Option<payment_ui::PaymentForm>,
    pay_bills: Option<ap_ui::PayForm>,
    customer_ui: customer_ui::CustomerUi,
    vendor_ui: vendor_ui::VendorUi,
    sidebar_collapsed: bool,
    software: app_settings_ui::SoftwareUi,
    users_ui: auth_ui::UsersUi,
    party_cache: party_data::Cache,
    direct_invoice: direct_invoice_ui::DirectUi,
    invoice_workbench: invoice_workbench::Workbench,
    gl: gl_ui::GlUi,
    number_rules: Option<Vec<crate::numbering::Rule>>,
    list_query: String,
    page_queries: [String; 18],
    page_selected: [i64; 18],
    saved_settings: Settings,
    task_filter: String,
    fx_receiver:
        Option<std::sync::mpsc::Receiver<std::result::Result<Vec<crate::fx::DailyRates>, String>>>,
    fx_status: String,
    fx_active_date: Option<String>,
    fx_pending_dates: std::collections::BTreeSet<String>,
    fx_requested_dates: std::collections::BTreeSet<String>,
    ui_context: Option<egui::Context>,
    path: PathBuf,
    page: usize,
    message: String,
    error: bool,
    settings: Settings,
    selected: i64,
    task_title: String,
    task_date: chrono::NaiveDate,
    customer: i64,
    task_currency: String,
    chosen: Vec<i64>,
    new_record: Option<NewRecord>,
    price_edit: Option<PriceEdit>,
    template_ui: invoice_template_ui::TemplateUi,
    pdf_receiver: Option<std::sync::mpsc::Receiver<std::result::Result<PathBuf, String>>>,
    print_receiver: Option<std::sync::mpsc::Receiver<std::result::Result<(), String>>>,
    new_task_open: bool,
    new_task_lines: Vec<TaskLine>,
    lookup_row: Option<usize>,
    lookup_search: String,
    lookup_selected: i64,
    new_item_row: Option<usize>,
    task_form_epoch: u64,
    record_form_epoch: u64,
    new_task_error: String,
    task: i64,
    task_editor_open: bool,
    task_review: Option<task_editor_ui::Review>,
    task_review_focus: bool,
    checked_tasks: std::collections::BTreeSet<i64>,
    invoice_results: Vec<(i64, String, bool)>,
    item: i64,
    item_price: String,
    item_cost: String,
    task_item_edits: std::collections::BTreeMap<i64, (String, String, i64)>,
    item_tax_code: i64,
    task_tax_edits: std::collections::BTreeMap<i64, i64>,
    tax_ui: tax_ui::TaxUi,
    quantity: i64,
    expense: String,
    expense_amount: String,
    expense_vendor: i64,
    task_expense_tax: crate::tax::Input,
    source: String,
    target: String,
    rate: String,
}
impl Desktop {
    fn reset_after_restore(&mut self) -> Result<()> {
        self.users_ui = Default::default();
        self.windows.reset_runtime();
        self.settings = Settings::load(&self.db)?;
        self.saved_settings = self.settings.clone();
        self.page_queries = Default::default();
        self.page_selected = [0; 18];
        self.page = 0;
        self.selected = 0;
        self.task = 0;
        self.item = 0;
        self.checked_tasks.clear();
        self.invoice_results.clear();
        self.new_task_open = false;
        self.task_editor_open = false;
        self.task_review = None;
        self.task_item_edits.clear();
        self.new_record = None;
        self.template_ui = Default::default();
        self.menus.checks = None;
        self.menus.reconciliation = None;
        self.lookup_row = None;
        self.new_item_row = None;
        self.accounting = Default::default();
        self.billing_ui = Default::default();
        self.payment = None;
        self.pay_bills = None;
        self.customer_ui = Default::default();
        self.vendor_ui = Default::default();
        self.party_cache = Default::default();
        self.direct_invoice = Default::default();
        self.invoice_workbench = Default::default();
        self.gl = Default::default();
        self.tax_ui = Default::default();
        self.task_tax_edits.clear();
        self.number_rules = None;
        self.chosen.clear();
        self.new_task_lines.clear();
        self.task_currency = self.settings.currency.clone();
        // Discard an in-flight startup FX result so it cannot overwrite restored rates.
        self.fx_receiver = None;
        self.fx_active_date = None;
        self.fx_pending_dates.clear();
        self.fx_requested_dates.clear();
        self.fx_status = "已恢复备份中的本地汇率".into();
        self.price_edit = None;
        self.pdf_receiver = None;
        self.print_receiver = None;
        self.list_query.clear();
        self.lookup_search.clear();
        self.lookup_selected = 0;
        self.new_task_error.clear();
        Ok(())
    }

    fn prepare_gl_fx(&mut self, currency: &str, date: &str) -> bool {
        match crate::gl::quote(&self.db, currency, date) {
            Ok(_) => true,
            Err(e) => {
                if let Some(ctx) = self.ui_context.clone() {
                    self.request_dated_fx(&ctx, date);
                }
                self.message = format!("{e:#}. Fetching the historical rate if available; retry after the lookup completes.");
                self.error = true;
                false
            }
        }
    }
    fn missing_dated_fx(&mut self, source: &str, target: &str, date: &str) -> bool {
        if crate::fx::dated(&self.db, source, target, date).is_ok() {
            return false;
        }
        if let Some(ctx) = self.ui_context.clone() {
            self.request_dated_fx(&ctx, date);
        }
        self.message = format!("No saved {source}/{target} rate for {date}. A historical lookup has been requested; retry when it completes.");
        self.error = true;
        true
    }
    fn new(db: Connection, path: PathBuf) -> Result<Self> {
        let settings = Settings::load(&db)?;
        let currency = settings.currency.clone();
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self {
            windows: windows::Windows::load(&path),
            db,
            accounting: Default::default(),
            menus: Default::default(),
            billing_ui: Default::default(),
            payment: None,
            pay_bills: None,
            customer_ui: Default::default(),
            vendor_ui: Default::default(),
            sidebar_collapsed: navigation::load_collapsed(&path),
            software: app_settings_ui::SoftwareUi::load(&path)?,
            users_ui: Default::default(),
            party_cache: Default::default(),
            direct_invoice: Default::default(),
            invoice_workbench: Default::default(),
            gl: Default::default(),
            number_rules: None,
            list_query: String::new(),
            page_queries: Default::default(),
            page_selected: [0; 18],
            saved_settings: settings.clone(),
            task_filter: "all".into(),
            fx_receiver: None,
            fx_status: String::new(),
            fx_active_date: None,
            fx_pending_dates: Default::default(),
            fx_requested_dates: Default::default(),
            ui_context: None,
            path,
            page: 0,
            message: String::new(),
            error: false,
            settings,
            selected: 0,
            task_title: String::new(),
            task_date: crate::today(),
            customer: 0,
            task_currency: currency,
            chosen: Vec::new(),
            new_record: None,
            price_edit: None,
            template_ui: Default::default(),
            pdf_receiver: None,
            print_receiver: None,
            new_task_open: false,
            new_task_lines: Vec::new(),
            lookup_row: None,
            lookup_search: String::new(),
            lookup_selected: 0,
            new_item_row: None,
            task_form_epoch: 0,
            record_form_epoch: 0,
            new_task_error: String::new(),
            task: 0,
            task_editor_open: false,
            task_review: None,
            task_review_focus: false,
            checked_tasks: std::collections::BTreeSet::new(),
            invoice_results: Vec::new(),
            item: 0,
            item_price: String::new(),
            item_cost: String::new(),
            task_item_edits: Default::default(),
            item_tax_code: 0,
            task_tax_edits: Default::default(),
            tax_ui: Default::default(),
            quantity: 1,
            expense: String::new(),
            expense_amount: "0.00".into(),
            expense_vendor: 0,
            task_expense_tax: Default::default(),
            source: "USD".into(),
            target: "CAD".into(),
            rate: String::new(),
        })
    }
    fn missing_fx(&mut self, source: &str, target: &str) -> bool {
        if source == target || crate::currency(source).is_err() || crate::currency(target).is_err()
        {
            return false;
        }
        if matches!(crate::fx::cached(&self.db, source, target), Ok(None)) {
            if let Some(ctx) = self.ui_context.clone() {
                self.start_fx_sync(&ctx);
                self.message =
                    format!("缺少 {source} → {target} 本地汇率，正在后台查询；完成后请再次保存。");
                self.error = false;
                return true;
            }
        }
        false
    }
    fn prepare_price_fx(&mut self, prices: &[i64], target: &str) -> bool {
        for id in prices {
            let source = self
                .db
                .query_row("SELECT currency FROM prices WHERE id=?", [id], |r| {
                    r.get::<_, String>(0)
                });
            if let Ok(source) = source {
                if self.missing_fx(&source, target) {
                    return false;
                }
            }
        }
        true
    }
    fn prepare_command_fx(&mut self, command: &Command) -> bool {
        if let Command::InvoiceCreate { task } = command {
            let currency = self
                .db
                .query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
                    r.get::<_, String>(0)
                })
                .ok();
            let taxable=self.db.query_row("SELECT EXISTS(SELECT 1 FROM items i JOIN tax_codes c ON c.id=i.tax_code WHERE i.task=? AND c.kind='taxable')",[task],|r|r.get::<_,bool>(0)).unwrap_or(false);
            let base = crate::gl::base(&self.db).unwrap_or_else(|_| "CAD".into());
            return currency.is_none_or(|currency| {
                !self.missing_fx(&currency, &base)
                    && (!taxable || !self.missing_fx(&currency, "CAD"))
            });
        }
        let (task, prices) = match command {
            Command::TaskItems { task, price_ids } => (*task, price_ids.clone()),
            Command::TaskItem { task, price_id, .. } => (*task, vec![*price_id]),
            _ => return true,
        };
        let target = self
            .db
            .query_row("SELECT currency FROM tasks WHERE id=?", [task], |r| {
                r.get::<_, String>(0)
            });
        if let Ok(target) = target {
            self.prepare_price_fx(&prices, &target)
        } else {
            true
        }
    }
    fn action(&mut self, command: Command) -> bool {
        if !self.prepare_command_fx(&command) {
            return false;
        }
        match crate::execute(&mut self.db, command) {
            Ok(()) => {
                self.message = "操作成功，已保存".into();
                self.error = false;
                true
            }
            Err(e) => {
                self.message = format!("{e:#}");
                self.error = true;
                false
            }
        }
    }
    fn row_matches(&self, row: &[String]) -> bool {
        let q = self.list_query.trim().to_lowercase();
        q.is_empty() || row.iter().any(|v| v.to_lowercase().contains(&q))
    }
    fn rows(&mut self, sql: &str) -> Vec<Vec<String>> {
        match query(&self.db, sql) {
            Ok(r) => r,
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
                Vec::new()
            }
        }
    }
    fn prices(&mut self, ui: &mut egui::Ui) {
        let rows=self.rows("SELECT p.id,p.name,v.name,p.unit,p.currency,printf('%d.%02d',p.cost/100,p.cost%100),printf('%d.%02d',p.price/100,p.price%100),p.vendor,c.code FROM prices p JOIN vendors v ON v.id=p.vendor JOIN tax_codes c ON c.id=p.tax_code ORDER BY p.id");
        let visible = rows
            .into_iter()
            .filter(|r| self.row_matches(r))
            .collect::<Vec<_>>();
        let shown = visible
            .iter()
            .map(|r| {
                [0, 1, 2, 3, 4, 6, 8]
                    .into_iter()
                    .map(|i| r[i].clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if let Some(id) = controls::register(
            ui,
            "prices",
            &[
                "ID",
                "Item",
                "Vendor",
                "Unit",
                "Curr.",
                "Rate (excl. tax)",
                "Tax",
                "Action",
            ],
            &shown,
            &mut self.selected,
            true,
        )
        .1
        {
            if let Some(row) = visible.iter().find(|r| r[0] == id.to_string()) {
                if self.price_edit.is_none() {
                    self.price_edit = Some(PriceEdit {
                        id,
                        name: row[1].clone(),
                        unit: row[3].clone(),
                        cost: row[5].clone(),
                        price: row[6].clone(),
                        currency: row[4].clone(),
                        ..Default::default()
                    });
                }
            }
        }
    }
    fn save_price_edit(&mut self, draft: &mut PriceEdit) -> bool {
        match crate::execute(
            &mut self.db,
            Command::PriceUpdate {
                id: draft.id,
                cost: draft.cost.clone(),
                price: draft.price.clone(),
                currency: Some(draft.currency.clone()),
            },
        ) {
            Ok(()) => {
                draft.error.clear();
                draft.baseline = Some(draft.fingerprint());
                self.message = "业务价格已保存".into();
                self.error = false;
                true
            }
            Err(error) => {
                draft.error = format!("{error:#}");
                false
            }
        }
    }
    fn price_edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.price_edit.take() else {
            return;
        };
        let mut close = false;
        let fingerprint = draft.fingerprint();
        if draft.baseline.is_none() {
            draft.baseline = Some(fingerprint);
        }
        let response = egui::Modal::new(egui::Id::new(("price_edit", draft.id))).show(ctx, |ui| {
            ui.set_width(500.0_f32.min((ctx.content_rect().width() - 48.0).max(280.0)));
            ui.heading("Edit Product or Service");
            ui.separator();
            ui.label(format!("业务：{} · 单位：{}", draft.name, draft.unit));
            field(ui, "销售单价", &mut draft.price);
            ui.horizontal(|ui| {
                ui.label("币种");
                currency_picker(ui, "edit_item_currency", &mut draft.currency);
            });
            ui.small("修改币种时，请填写该币种下的新售价。默认税码可在 Setup → Sales Tax → 分配默认税码设置。");
            if !draft.error.is_empty() {
                ui.colored_label(Color32::from_rgb(190, 45, 45), &draft.error);
            }
            ui.separator();
            let (save,save_close,cancel)=edit_buttons(ui,true);
            if (save || save_close) && self.save_price_edit(&mut draft) && save_close { close=true; }
            if cancel {draft.close_guard.request();}
        });
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            draft.close_guard.request();
        }
        let dirty = draft.baseline.as_ref() != Some(&draft.fingerprint());
        close |= draft.close_guard.show(ctx, "price_close", dirty);
        if !close {
            self.price_edit = Some(draft);
        }
    }
    fn start_fx_sync(&mut self, ctx: &egui::Context) {
        if self.fx_receiver.is_some() {
            return;
        }
        self.fx_requested_dates.clear();
        self.launch_fx_request(ctx, None);
    }
    fn request_dated_fx(&mut self, ctx: &egui::Context, date: &str) {
        if let Err(error) = crate::fx::history_window(date) {
            self.fx_status = error.to_string();
            return;
        }
        if self.fx_requested_dates.insert(date.to_owned()) {
            self.fx_pending_dates.insert(date.to_owned());
        }
        self.start_pending_fx(ctx);
    }
    fn start_pending_fx(&mut self, ctx: &egui::Context) {
        if self.fx_receiver.is_none() {
            if let Some(date) = self.fx_pending_dates.pop_first() {
                self.launch_fx_request(ctx, Some(date));
            }
        }
    }
    fn launch_fx_request(&mut self, ctx: &egui::Context, date: Option<String>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.fx_active_date = date.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match date {
                Some(date) => crate::fx::fetch_dated(&date),
                None => crate::fx::fetch(),
            }
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.fx_receiver = Some(receiver);
        self.fx_status = match &self.fx_active_date {
            Some(date) => format!("Fetching historical exchange rates for {date}..."),
            None => "Fetching recent exchange rates...".into(),
        };
    }
    fn poll_fx(&mut self, ctx: &egui::Context) {
        let received = self.fx_receiver.as_ref().and_then(|r| match r.try_recv() {
            Ok(value) => Some(value),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Err("汇率同步线程已结束".into()))
            }
        });
        if let Some(result) = received {
            self.fx_receiver = None;
            let requested_date = self.fx_active_date.take();
            let result = result.map_err(anyhow::Error::msg).and_then(|days| {
                if let Some(date) = &requested_date {
                    crate::fx::store_dated(&self.db, date, &days)
                } else {
                    crate::fx::store(&self.db, &days)
                }
            });
            match result {
                Ok(date) => {
                    self.fx_status = format!("Exchange rates saved: {date} - Bank of Canada");
                }
                Err(e) => {
                    self.fx_status =
                        format!("Exchange rate lookup failed; saved rates retained: {e:#}");
                }
            };
        }
        self.start_pending_fx(ctx);
        if self.fx_receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
    fn rates(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("仅启动时自动抓取一次；使用汇率时优先查询本地缓存。");
            if ui
                .add_enabled(
                    self.fx_receiver.is_none(),
                    egui::Button::new("Sync Missing Rates"),
                )
                .clicked()
            {
                self.start_fx_sync(ui.ctx());
            }
        });
        ui.label(&self.fx_status);
        let last: Option<String> = self
            .db
            .query_row("SELECT last_success FROM fx_sync WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap_or(None);
        ui.small(format!(
            "上次成功抓取（UTC）：{}",
            last.as_deref().unwrap_or("尚未成功同步")
        ));
        ui.small("参考日均汇率；非发布日使用最近可用日期。离线使用本地缓存。非 CAD 币种之间通过同日 CAD 汇率计算。");
        ui.hyperlink_to(
            "来源：Bank of Canada",
            "https://www.bankofcanada.ca/rates/exchange/daily-exchange-rates/",
        );
        let rows=self.rows("SELECT rowid,source,target,printf('%d.%08d',rate/100000000,rate%100000000),provider,COALESCE(rate_date,'未指定'),updated FROM fx_rates ORDER BY source,target");
        table(
            ui,
            "rates",
            &[
                "ID",
                "原币种",
                "目标币种",
                "汇率",
                "来源",
                "汇率日期",
                "抓取/修改时间 UTC",
            ],
            &rows,
            &mut self.selected,
        );
        ui.separator();
        ui.label("1 原币种 = 汇率 × 目标币种，最多八位小数。");
        ui.horizontal(|ui| {
            currency_picker(ui, "source", &mut self.source);
            ui.label("→");
            currency_picker(ui, "target", &mut self.target);
        });
        field(ui, "汇率", &mut self.rate);
        if ui.button("Save Rate").clicked() {
            self.action(Command::FxSet {
                source: self.source.clone(),
                target: self.target.clone(),
                rate: self.rate.clone(),
            });
        }
        ui.small("手工录入会在下次自动同步时被替换。已有任务明细和发票不受影响。");
    }
    fn open_creation(&mut self) {
        if self.page == 1 {
            self.open_customer_editor(0);
            return;
        }
        if self.page == 2 {
            self.open_vendor_editor(0);
            return;
        }
        if self.page == 0 && self.reuse_business(windows::Kind::Task) {
            return;
        }
        if self.new_record.is_some() {
            self.message = "已有资料录入窗口，请先完成或关闭该窗口。".into();
            return;
        }
        if self.page == 8 {
            self.open_direct_invoice();
            return;
        }
        if self.page == 13 {
            self.open_journal(0);
            return;
        }
        if self.page == 6 || self.page == 7 {
            self.open_accounting_form(0);
            return;
        }
        self.record_form_epoch = self.record_form_epoch.wrapping_add(1);
        self.new_item_row = None;
        if self.page == 0 {
            self.open_new_task();
            return;
        }
        if self.page != 3 {
            return;
        }
        let currency = Settings::load(&self.db)
            .map(|s| s.currency)
            .unwrap_or_else(|_| self.settings.currency.clone());
        self.new_record = Some(NewRecord {
            page: self.page,
            name: String::new(),
            vendor: 0,
            unit: "项".into(),
            cost: "0.00".into(),
            price: "0.00".into(),
            currency,
            error: String::new(),
            ..Default::default()
        });
    }
    fn save_record(&mut self, draft: &mut NewRecord) -> bool {
        let command = Command::PriceAdd {
            vendor: draft.vendor,
            name: draft.name.clone(),
            cost: draft.cost.clone(),
            price: draft.price.clone(),
            unit: draft.unit.clone(),
            currency: Some(draft.currency.clone()),
        };
        match crate::execute(&mut self.db, command) {
            Ok(()) => {
                if let Some(row) = self.new_item_row.take() {
                    self.assign_task_line(row, self.db.last_insert_rowid());
                    self.lookup_row = None;
                    self.message = "新业务已保存到价格单，并带入当前任务行".into();
                    self.error = false;
                    return true;
                }
                if !(self.page == 2 && draft.page == 3) {
                    self.page = draft.page;
                }
                self.selected = self.db.last_insert_rowid();
                self.message = "已添加新记录".into();
                self.error = false;
                true
            }
            Err(e) => {
                draft.error = format!("{e:#}");
                false
            }
        }
    }
    fn record_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.new_record.take() else {
            return;
        };
        let mut close = false;
        if draft.baseline.is_none() {
            draft.baseline = Some(draft.fingerprint());
        }
        let response = egui::Modal::new(egui::Id::new("new_record_modal")).show(ctx, |ui| {
            ui.set_width(500.0_f32.min((ctx.content_rect().width() - 48.0).max(280.0)));
            ui.horizontal(|ui| {
                ui.heading("New Product or Service");
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("new_record_form")
                .max_height((ctx.content_rect().height() - 220.0).max(180.0))
                .show(ui, |ui| {
                    field(ui, "Service Name *", &mut draft.name);
                    let vendors =
                        self.rows("SELECT id,name FROM vendors WHERE active=1 ORDER BY id");
                    ui.horizontal(|ui| {
                        ui.label("Vendor *");
                        picker(
                            ui,
                            &format!("new_item_vendor_{}", self.record_form_epoch),
                            &mut draft.vendor,
                            &vendors,
                        );
                    });
                    if vendors.is_empty() {
                        ui.colored_label(
                            Color32::from_rgb(174, 100, 20),
                            "Add a vendor before creating this service.",
                        );
                    }
                    field(ui, "Unit *", &mut draft.unit);
                    field(ui, "Sales Price", &mut draft.price);
                    ui.horizontal(|ui| {
                        ui.label("Currency");
                        currency_picker(ui, "new_item_currency", &mut draft.currency);
                    });
                });
            if !draft.error.is_empty() {
                ui.colored_label(Color32::from_rgb(190, 45, 45), &draft.error);
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = !draft.name.trim().is_empty()
                    && (draft.page != 3 || (draft.vendor > 0 && !draft.unit.trim().is_empty()));
                if ui.add_enabled(ready, egui::Button::new("Create")).clicked()
                    && self.save_record(&mut draft)
                {
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    draft.close_guard.request();
                }
            });
        });
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            draft.close_guard.request();
        }
        let dirty = draft.baseline.as_ref() != Some(&draft.fingerprint());
        close |= draft.close_guard.show(ctx, "new_record_close", dirty);
        if !close {
            self.new_record = Some(draft);
        } else {
            self.new_item_row = None;
        }
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.draw(ctx);
    }
}
impl Desktop {
    fn draw(&mut self, ctx: &egui::Context) {
        if crate::auth::require(&self.db).is_err() {
            if self.users_ui.login.show(ctx, &self.db) {
                self.users_ui = Default::default();
                self.start_fx_sync(ctx);
                ctx.request_repaint();
            }
            return;
        }
        self.ui_context = Some(ctx.clone());
        self.poll_fx(ctx);
        self.poll_pdf(ctx);
        self.menu_bar(ctx);
        self.navigation(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let message = if self.message.is_empty() {
                        "Ready"
                    } else {
                        &self.message
                    };
                    ui.add(
                        egui::Label::new(RichText::new(message).size(12.0).color(if self.error {
                            Color32::from_rgb(180, 58, 55)
                        } else {
                            theme::MUTED
                        }))
                        .wrap(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(&self.settings.currency);
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(18))
            .show(ctx, |ui| {
                if [1, 2].contains(&self.page) {
                    if self.page == 1 {
                        self.customer_center(ui)
                    } else {
                        self.vendor_center(ui)
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.heading(RichText::new(PAGES[self.page].title).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(label) = creation_label(self.page) {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(label).color(Color32::WHITE))
                                        .fill(theme::GREEN)
                                        .stroke(egui::Stroke::NONE),
                                )
                                .clicked()
                            {
                                self.open_creation();
                            }
                        }
                        if self.page == 0
                            && ui
                                .button("Create Invoice...")
                                .on_hover_text("Select completed tasks and preview the invoice")
                                .clicked()
                        {
                            self.invoice_checked_tasks();
                        }
                    });
                });
                ui.label(RichText::new(PAGES[self.page].description).color(theme::MUTED));
                ui.add_space(12.0);
                if PAGES[self.page].search {
                    controls::search(
                        ui,
                        "page_search",
                        &mut self.list_query,
                        310.0,
                        "Search name or code...",
                    );
                    ui.add_space(12.0);
                }
                if self.page == 5 {
                    self.settings_page(ui);
                } else if self.page == 16 {
                    self.software_settings_page(ui);
                } else if self.page == 17 {
                    self.users_page(ui);
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("page_scroll")
                        .show(ui, |ui| {
                            theme::card().show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                match self.page {
                                    0 => self.tasks(ui),
                                    3 => self.prices(ui),
                                    4 => self.rates(ui),
                                    6 => self.expenses_page(ui),
                                    7 => self.accounts_page(ui),
                                    8 => self.invoices_page(ui),
                                    9 => self.numbering_page(ui),
                                    10 => self.audit_page(ui),
                                    11 => self.sales_tax(ui),
                                    12 => self.tax_setup(ui),
                                    13 => self.journals_page(ui),
                                    14 => {
                                        if ui.button("Open Ledger / Trial Balance").clicked() {
                                            self.windows.ledger = true;
                                            self.focus_business(windows::Kind::Ledger);
                                        }
                                        if !self.windows.ledger {
                                            self.ledger_page(ui);
                                        }
                                    }
                                    _ => self.gl_setup_page(ui),
                                }
                            });
                        });
                }
            });
        self.business_windows(ctx);
        self.about_dialog(ctx);
    }
}
pub fn launch(db: Connection, path: PathBuf) -> Result<()> {
    let mut app = auth_ui::Shell::new(db, path)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native(
        &format!("{} v{}", crate::APP_NAME, env!("CARGO_PKG_VERSION")),
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            for file in [
                "C:/Windows/Fonts/msyh.ttc",
                "C:/Windows/Fonts/simhei.ttf",
                "C:/Windows/Fonts/simsun.ttc",
            ] {
                if let Ok(bytes) = std::fs::read(file) {
                    fonts
                        .font_data
                        .insert("chinese".into(), egui::FontData::from_owned(bytes).into());
                    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                        fonts
                            .families
                            .entry(family)
                            .or_default()
                            .insert(0, "chinese".into());
                    }
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);
            theme::configure(&cc.egui_ctx);
            app.start(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("无法启动桌面界面：{e}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_requests_queue_behind_active_sync_and_reset_with_restore() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let (_sender, receiver) = std::sync::mpsc::channel();
        app.fx_receiver = Some(receiver);
        let ctx = egui::Context::default();
        app.request_dated_fx(&ctx, "2025-12-31");
        app.request_dated_fx(&ctx, "2025-12-31");
        app.request_dated_fx(&ctx, "2025-11-30");
        assert_eq!(app.fx_pending_dates.len(), 2);
        assert_eq!(app.fx_requested_dates.len(), 2);
        assert!(app.fx_active_date.is_none());
        app.reset_after_restore()?;
        assert!(app.fx_receiver.is_none());
        assert!(app.fx_pending_dates.is_empty());
        assert!(app.fx_requested_dates.is_empty());
        Ok(())
    }
    #[test]
    fn historical_worker_result_is_saved_and_reused_locally() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let mut observation = serde_json::json!({"d":"2026-01-02"});
        for code in ["USD", "CNY", "EUR", "GBP", "HKD", "AUD", "SGD"] {
            observation[format!("FX{code}CAD")] = serde_json::json!({"v":"1.35"});
        }
        let days = crate::fx::parse(&serde_json::json!({"observations":[observation]}))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        app.fx_receiver = Some(receiver);
        app.fx_active_date = Some("2026-01-04".into());
        sender.send(Ok(days))?;
        let ctx = egui::Context::default();
        app.poll_fx(&ctx);
        assert_eq!(
            crate::fx::dated(&app.db, "USD", "CAD", "2026-01-04")?,
            135000000
        );
        assert!(app.prepare_gl_fx("USD", "2026-01-04"));
        assert!(app.fx_receiver.is_none());
        assert!(app.fx_active_date.is_none());
        assert!(app.fx_status.contains("2026-01-02"));
        Ok(())
    }
    #[test]
    fn all_pages_render_with_real_data() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Design','hour',100,200,'CAD');")?;
        crate::add_task(&db, "Project", 1, "CAD")?;
        crate::add_item(&db, 1, 1, 1, None, None)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.task = 1;
        let ctx = egui::Context::default();
        for page in 0..PAGES.len() {
            app.page = page;
            let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        }
        app.open_task_editor(1);
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        app.action(Command::TaskProgress {
            task: 1,
            progress: 100,
        });
        app.action(Command::InvoiceCreate { task: 1 });
        assert!(!app.error, "{}", app.message);
        app.page = 0;
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        Ok(())
    }
    #[test]
    fn new_task_modal_keeps_failed_draft_and_creates_atomically() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Design','hour',100,200,'USD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.chosen = vec![77];
        app.open_new_task();
        app.task_title = "New project".into();
        app.customer = 1;
        app.new_task_lines = vec![TaskLine {
            price_id: 999,
            ..Default::default()
        }];
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        app.save_new_task();
        assert!(app.new_task_open);
        assert!(!app.new_task_error.is_empty());
        assert_eq!(app.task_title, "New project");
        assert_eq!(app.new_task_lines[0].price_id, 999);
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?,
            0
        );
        app.db.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        app.new_task_lines = vec![TaskLine {
            price_id: 1,
            ..Default::default()
        }];
        app.save_new_task();
        assert!(!app.new_task_open);
        assert_eq!(app.chosen, vec![77]);
        assert_eq!(crate::snapshot(&app.db, app.task)?["title"], "New project");
        assert_eq!(crate::snapshot(&app.db, app.task)?["total"], 270);
        Ok(())
    }
    #[test]
    fn module_creation_routes_and_isolates_drafts() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let ctx = egui::Context::default();
        for (page, name) in [(1, "Customer A"), (2, "Vendor A"), (3, "Service A")] {
            app.page = page;
            app.open_creation();
            assert!(!app.new_task_open);
            let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
            if page == 1 {
                let mut form = app.customer_ui.form.take().unwrap();
                assert!(form.customer.name.is_empty());
                form.customer.name = name.into();
                form.customer.currency = "USD".into();
                assert!(app.save_customer_form(&mut form));
                continue;
            }
            if page == 2 {
                let mut form = app.vendor_ui.form.take().unwrap();
                assert!(form.vendor.name.is_empty());
                assert_eq!(form.vendor.currency, "CAD");
                form.vendor.name = name.into();
                form.vendor.currency = "USD".into();
                assert!(app.save_vendor_form(&mut form));
                continue;
            }
            let mut draft = app.new_record.take().unwrap();
            assert_eq!(draft.page, page);
            assert!(draft.name.is_empty());
            if page != 3 {
                assert_eq!(draft.currency, "CAD");
                draft.currency = "USD".into();
            }
            draft.name = name.into();
            draft.vendor = 1;
            if page == 3 {
                draft.price = "invalid".into();
                assert!(!app.save_record(&mut draft));
                assert!(!draft.error.is_empty());
                draft.price = "10.00".into();
            }
            assert!(app.save_record(&mut draft));
        }
        for table in ["customers", "vendors", "prices"] {
            assert_eq!(
                app.db
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))?,
                1
            );
        }
        assert_eq!(creation_label(0), Some("＋ New Task"));
        assert_eq!(creation_label(3), Some("＋ New Item"));
        assert!(creation_label(4).is_none());
        assert!(creation_label(5).is_none());
        for table in ["customers", "vendors"] {
            assert_eq!(
                app.db.query_row(
                    &format!("SELECT currency FROM {table} WHERE id=1"),
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "USD"
            );
        }
        app.page = 1;
        app.open_creation();
        assert!(app
            .customer_ui
            .form
            .as_ref()
            .unwrap()
            .customer
            .name
            .is_empty());
        Ok(())
    }
    #[test]
    fn task_entry_lines_price_quantity_and_lookup_create_flow() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Registry','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Annual Report','item',0,5000,'CAD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_new_task();
        app.task_title = "Annual filing".into();
        app.customer = 1;
        app.assign_task_line(0, 1);
        app.new_task_lines[0].quantity = 2;
        app.new_task_lines[0].price_override = Some("45.00".into());
        app.new_task_lines.push(TaskLine::default());
        let catalog = app.catalog()?;
        assert!(catalog[0].matches("annual"));
        assert!(catalog[0].matches("registry"));
        assert!(catalog[0].matches("1"));
        assert!(!catalog[0].matches("missing"));
        assert_eq!(app.draft_total(&catalog)?, (9000, 0));
        app.open_item_lookup(1);
        app.create_item_for_line(1);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        let mut draft = app.new_record.take().unwrap();
        draft.name = "Address update".into();
        draft.vendor = 1;
        draft.price = "25.00".into();
        assert!(app.save_record(&mut draft));
        assert!(app.new_task_open);
        assert_eq!(app.page, 0);
        assert!(app.lookup_row.is_none());
        assert_eq!(app.new_task_lines[1].price_id, 2);
        assert_eq!(
            app.new_task_lines[0].price_override.as_deref(),
            Some("45.00")
        );
        let catalog = app.catalog()?;
        assert_eq!(app.draft_total(&catalog)?, (11500, 0));
        app.new_task_lines.push(TaskLine::default()); // unused trailing row is not persisted
        app.save_new_task();
        assert!(!app.new_task_open, "{}", app.new_task_error);
        let task = crate::snapshot(&app.db, app.task)?;
        assert_eq!(task["total"], 11500);
        assert_eq!(task["items"].as_array().unwrap().len(), 2);
        assert_eq!(task["items"][0]["quantity"], 2);
        assert_eq!(task["items"][0]["unit_price"], 4500);
        assert_eq!(
            app.db
                .query_row("SELECT price FROM prices WHERE id=1", [], |r| r
                    .get::<_, i64>(0))?,
            5000
        );
        Ok(())
    }
    #[test]
    fn task_editor_save_is_atomic_and_closes_only_on_success() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        crate::add_task(&db, "Task", 1, "CAD")?;
        let first = crate::add_item(&db, 1, 1, 1, None, None)?;
        let second = crate::add_item(&db, 1, 1, 1, None, None)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_task_editor(1);
        app.task_item_edits
            .insert(first, ("40.00".into(), "0.00".into(), 2));
        app.item = second;
        app.item_price = "15.00".into();
        app.item_cost = "0.00".into();
        app.quantity = 1;
        app.chosen = vec![1];
        app.expense = "Trip".into();
        app.expense_amount = "invalid".into();
        app.expense_vendor = 1;
        assert!(!app.save_task_editor(true));
        assert!(app.task_editor_open);
        assert_eq!(crate::snapshot(&app.db, 1)?["total"], 10000);
        assert_eq!(app.chosen, vec![1]);
        assert_eq!(app.expense, "Trip");
        app.expense_amount = "7.00".into();
        assert!(app.save_task_editor(false));
        assert!(app.task_editor_open);
        let saved = crate::snapshot(&app.db, 1)?;
        assert_eq!(saved["total"], 14500);
        assert_eq!(saved["cost"], 700);
        assert_eq!(saved["items"].as_array().unwrap().len(), 3);
        assert!(app.save_task_editor(true));
        assert!(!app.task_editor_open);
        assert_eq!(
            crate::snapshot(&app.db, 1)?["expenses"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        crate::set_task_status(&mut app.db, 1, false)?;
        crate::billing::merge(&mut app.db, &[1])?;
        app.open_task_editor(1);
        assert!(!app.save_task_editor(true));
        assert!(app.task_editor_open);
        assert!(app.error);
        Ok(())
    }
    #[test]
    fn restore_reset_discards_drafts_and_stale_background_results() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.price_edit = Some(PriceEdit {
            id: 1,
            name: "Old company product".into(),
            unit: "item".into(),
            cost: "0".into(),
            price: "999".into(),
            currency: "CAD".into(),
            error: String::new(),
            ..Default::default()
        });
        app.open_invoice_template();
        app.new_task_open = true;
        app.task_editor_open = true;
        app.checked_tasks.insert(1);
        let (pdf_sender, pdf_receiver) = std::sync::mpsc::channel();
        app.pdf_receiver = Some(pdf_receiver);
        let (print_sender, print_receiver) = std::sync::mpsc::channel();
        app.print_receiver = Some(print_receiver);
        let (fx_sender, fx_receiver) = std::sync::mpsc::channel();
        app.fx_receiver = Some(fx_receiver);
        app.reset_after_restore()?;
        assert!(app.price_edit.is_none());
        assert!(!app.new_task_open && !app.task_editor_open);
        assert!(app.checked_tasks.is_empty());
        assert!(pdf_sender.send(Ok(PathBuf::from("old.pdf"))).is_err());
        assert!(print_sender.send(Ok(())).is_err());
        assert!(fx_sender.send(Ok(Vec::new())).is_err());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        assert!(!app.message.contains("old.pdf"));
        Ok(())
    }
    #[test]
    fn product_row_edit_opens_modal_and_validates_without_losing_draft() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'First','item',100,5000,'CAD'),(2,1,'Second','hour',200,7500,'USD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 3;
        let ctx = egui::Context::default();
        let frame = |app: &mut Desktop, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        assert!(app.price_edit.is_none());
        let edit = ctx
            .data_mut(|data| {
                data.get_temp::<egui::Rect>(egui::Id::new((
                    "register_action",
                    "prices",
                    "2".to_string(),
                )))
            })
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(edit),
                    egui::Event::PointerButton {
                        pos: edit,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let mut draft = app.price_edit.take().expect("Edit opens the row's modal");
        assert_eq!(draft.id, 2);
        assert_eq!(draft.name, "Second");
        assert_eq!(draft.price, "75.00");
        draft.price = "invalid".into();
        assert!(!app.save_price_edit(&mut draft));
        assert_eq!(draft.price, "invalid");
        assert!(!draft.error.is_empty());
        assert_eq!(
            app.db
                .query_row("SELECT price FROM prices WHERE id=2", [], |r| r
                    .get::<_, i64>(0))?,
            7500
        );
        draft.price = "88.50".into();
        draft.currency = "CAD".into();
        assert!(app.save_price_edit(&mut draft));
        assert!(draft.error.is_empty());
        assert_eq!(
            app.db.query_row(
                "SELECT price,cost,currency FROM prices WHERE id=2",
                [],
                |r| Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?
                ))
            )?,
            (8850, 200, "CAD".into())
        );
        assert_eq!(
            app.db
                .query_row("SELECT price FROM prices WHERE id=1", [], |r| r
                    .get::<_, i64>(0))?,
            5000
        );
        draft.price = "999.00".into();
        app.price_edit = Some(draft);
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.price_edit.as_ref().unwrap().price, "999.00");
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let discard = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("price_close", "review_confirm"))))
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(discard),
                    egui::Event::PointerButton {
                        pos: discard,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.price_edit.is_none());
        assert_eq!(
            app.db
                .query_row("SELECT price FROM prices WHERE id=2", [], |r| r
                    .get::<_, i64>(0))?,
            8850
        );
        Ok(())
    }
    #[test]
    fn task_list_checkboxes_and_edit_are_independent() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute(
            "INSERT INTO customers(id,name,contact) VALUES(1,'Customer','')",
            [],
        )?;
        crate::add_task(&db, "List task", 1, "CAD")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let ctx = egui::Context::default();
        let frame = |app: &mut Desktop, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 850.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let click = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let check = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new(("task_checkbox_rect", 1i64))))
            .unwrap()
            .center();
        frame(&mut app, click(check, true));
        frame(&mut app, click(check, false));
        assert!(app.checked_tasks.contains(&1));
        assert!(!app.task_editor_open);
        let edit = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new(("task_edit_rect", 1i64))))
            .unwrap()
            .center();
        frame(&mut app, click(edit, true));
        frame(&mut app, click(edit, false));
        assert!(app.task_editor_open);
        assert_eq!(app.task, 1);
        assert!(app.checked_tasks.contains(&1));
        Ok(())
    }
    #[test]
    fn opening_workbench_keeps_checks_without_creating_invoice() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        for title in ["One", "Two", "Pending"] {
            let task = crate::add_task(&app.db, title, 1, "CAD")?;
            crate::add_item(&app.db, task, 1, 1, None, None)?;
        }
        app.db
            .execute("UPDATE tasks SET progress=100 WHERE id IN (1,2)", [])?;
        app.checked_tasks.extend([1, 2, 3]);
        app.invoice_checked_tasks();
        assert!(app.invoice_workbench.open);
        assert_eq!(app.checked_tasks.len(), 3);
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        assert_eq!(app.checked_tasks.len(), 3);
        Ok(())
    }
    #[test]
    fn missing_rate_keeps_gui_draft_and_uses_background_fetch_before_transaction() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,1000,'USD');")?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_new_task();
        app.task_title = "Task".into();
        app.customer = 1;
        app.assign_task_line(0, 1);
        let ctx = egui::Context::default();
        app.ui_context = Some(ctx.clone());
        let (sender, receiver) = std::sync::mpsc::channel();
        app.fx_receiver = Some(receiver);
        app.save_new_task();
        assert!(app.new_task_open);
        assert!(!app.new_task_error.is_empty());
        assert!(app.db.is_autocommit());
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?,
            0
        );
        sender.send(Err("offline".into()))?;
        app.poll_fx(&ctx);
        assert!(app.fx_receiver.is_none());
        app.db.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        app.save_new_task();
        assert!(!app.new_task_open);
        assert_eq!(crate::snapshot(&app.db, app.task)?["total"], 1350);
        Ok(())
    }
}
