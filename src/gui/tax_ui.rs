use super::*;
use crate::tax::{self, admin};
#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn selecting_tax_record_preserves_unsaved_settings_until_discard() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.tax_ui.baselines = Some(std::array::from_fn(|i| app.tax_ui.input(i)));
        app.tax_ui.name = "Unsaved agency".into();
        assert!(app.has_business_editors());
        app.tax_ui.selection = Some(Selection::Agency(None));
        app.tax_ui.selection_guard.request();
        let ctx = egui::Context::default();
        for events in [
            vec![],
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.tax_selection(ctx),
            );
        }
        assert_eq!(app.tax_ui.name, "Unsaved agency");
        assert!(app.tax_ui.dirty());
        app.tax_ui.saved(1);
        assert!(
            app.tax_ui.dirty(),
            "Saving one tab must not clear another tab's draft"
        );
        assert_eq!(
            app.db.query_row(
                "SELECT count(*) FROM tax_agencies WHERE name='Unsaved agency'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        app.reset_after_restore()?;
        assert!(!app.tax_ui.dirty());
        Ok(())
    }
}
enum TaxReview {
    Create(tax::CashDraft, String),
    Void(i64),
}
enum Selection {
    Agency(Option<Vec<String>>),
    Code(Option<Vec<String>>),
}
pub(super) struct TaxUi {
    baselines: Option<[String; 4]>,
    selection: Option<Selection>,
    selection_guard: controls::CloseGuard,
    tab: usize,
    agency: i64,
    name: String,
    registration: String,
    registered: bool,
    payable: i64,
    recoverable: i64,
    item: i64,
    item_agency: i64,
    item_name: String,
    item_recoverable: bool,
    effective: chrono::NaiveDate,
    rate: String,
    code: i64,
    code_text: String,
    code_name: String,
    kind: String,
    active: bool,
    components: Vec<i64>,
    entity: String,
    record: i64,
    assign_code: i64,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    cash_agency: i64,
    cash_date: chrono::NaiveDate,
    cash_kind: String,
    amount: String,
    reference: String,
    reason: String,
    error: String,
    review: Option<TaxReview>,
    review_focus: bool,
}

impl Default for TaxUi {
    fn default() -> Self {
        let today = crate::today();
        Self {
            baselines: None,
            selection: None,
            selection_guard: Default::default(),
            tab: 0,
            agency: 0,
            name: String::new(),
            registration: String::new(),
            registered: false,
            payable: 0,
            recoverable: 0,
            item: 0,
            item_agency: 1,
            item_name: String::new(),
            item_recoverable: true,
            effective: today,
            rate: String::new(),
            code: -1,
            code_text: String::new(),
            code_name: String::new(),
            kind: "taxable".into(),
            active: true,
            components: Vec::new(),
            entity: "price".into(),
            record: 0,
            assign_code: 0,
            from: chrono::NaiveDate::from_ymd_opt(chrono::Datelike::year(&today), 1, 1).unwrap(),
            to: today,
            cash_agency: 1,
            cash_date: today,
            cash_kind: "payment".into(),
            amount: String::new(),
            reference: String::new(),
            reason: String::new(),
            error: String::new(),
            review: None,
            review_focus: false,
        }
    }
}
impl TaxUi {
    fn input(&self, tab: usize) -> String {
        match tab {
            0 => format!(
                "{:?}",
                (
                    self.agency,
                    &self.name,
                    &self.registration,
                    self.registered,
                    self.payable,
                    self.recoverable
                )
            ),
            1 => format!(
                "{:?}",
                (
                    self.item,
                    self.item_agency,
                    &self.item_name,
                    self.item_recoverable,
                    self.effective,
                    &self.rate
                )
            ),
            2 => format!(
                "{:?}",
                (
                    self.code,
                    &self.code_text,
                    &self.code_name,
                    &self.kind,
                    self.active,
                    &self.components
                )
            ),
            _ => format!("{:?}", (&self.entity, self.record, self.assign_code)),
        }
    }
    fn dirty_tab(&self, tab: usize) -> bool {
        self.baselines
            .as_ref()
            .is_some_and(|b| b[tab] != self.input(tab))
    }
    fn saved(&mut self, tab: usize) {
        let input = self.input(tab);
        if let Some(b) = &mut self.baselines {
            b[tab] = input;
        }
    }
    pub(super) fn dirty(&self) -> bool {
        (0..4).any(|i| self.dirty_tab(i))
            || self.review.is_some()
            || !self.amount.is_empty()
            || !self.reference.is_empty()
            || !self.reason.is_empty()
    }
}
pub(super) fn code_picker(
    c: &Connection,
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    value: &mut i64,
) -> bool {
    let rows = query(
        c,
        &format!(
            "SELECT id,code || ' · ' || name FROM tax_codes WHERE active=1 OR id={} ORDER BY id",
            *value
        ),
    )
    .unwrap_or_default();
    let old = *value;
    egui::ComboBox::from_id_salt(key)
        .width(170.0)
        .selected_text(
            rows.iter()
                .find(|r| r[0] == value.to_string())
                .map(|r| r[1].as_str())
                .unwrap_or("选择税码"),
        )
        .show_ui(ui, |ui| {
            for row in rows {
                if let Ok(id) = row[0].parse() {
                    ui.selectable_value(value, id, &row[1]);
                }
            }
        });
    old != *value
}
impl Desktop {
    fn tax_result(&mut self, result: Result<()>) {
        match result {
            Ok(()) => {
                self.tax_ui.error.clear();
                self.message = "税务数据已保存".into();
                self.error = false;
            }
            Err(e) => self.tax_ui.error = format!("{e:#}"),
        }
    }
    pub(super) fn tax_setup(&mut self, ui: &mut egui::Ui) {
        if self.tax_ui.baselines.is_none() {
            self.tax_ui.baselines = Some(std::array::from_fn(|i| self.tax_ui.input(i)));
        }
        ui.label("税率按单据日期选取，按行、按税目四舍五入到分。请根据实际供应地与业务选择税码。");
        ui.small("外币税额保存 CAD 折算快照：手工汇率优先，否则取单据日及之前 7 天内本地日汇率；没有适用报价时停止保存。");
        ui.small("启用销售计税前，请设置税务注册状态及税号。PST 默认不抵扣，GST/HST 的实际可抵扣比例在 Expense 每行填写。");
        ui.horizontal_wrapped(|ui| {
            for (i, label) in [
                "Agencies & Registration",
                "Items & Effective Rates",
                "Tax Codes",
                "Default Tax Assignments",
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut self.tax_ui.tab, i, *label);
            }
        });
        ui.separator();
        match self.tax_ui.tab {
            0 => self.tax_agencies_form(ui),
            1 => self.tax_rates_form(ui),
            2 => self.tax_codes_form(ui),
            _ => self.tax_assign_form(ui),
        }
        self.tax_selection(ui.ctx());
        if !self.tax_ui.error.is_empty() {
            ui.colored_label(Color32::from_rgb(190, 45, 45), &self.tax_ui.error);
        }
    }
    fn tax_selection(&mut self, ctx: &egui::Context) {
        let tab = match &self.tax_ui.selection {
            Some(Selection::Agency(_)) => 0,
            Some(Selection::Code(_)) => 2,
            None => return,
        };
        if !self
            .tax_ui
            .selection_guard
            .show(ctx, "tax_selection", self.tax_ui.dirty_tab(tab))
        {
            return;
        }
        match self.tax_ui.selection.take().unwrap() {
            Selection::Agency(row) => {
                let t = &mut self.tax_ui;
                t.agency = 0;
                t.name.clear();
                t.registration.clear();
                t.registered = false;
                t.payable = 0;
                t.recoverable = 0;
                if let Some(r) = row {
                    t.agency = r[0].parse().unwrap_or(0);
                    t.name = r[1].clone();
                    t.registration = r[2].clone();
                    t.registered = r[3] == "1";
                    t.payable = r[4].parse().unwrap_or(0);
                    t.recoverable = r[5].parse().unwrap_or(0);
                }
            }
            Selection::Code(row) => {
                let t = &mut self.tax_ui;
                t.code = -1;
                t.code_text.clear();
                t.code_name.clear();
                t.components.clear();
                t.kind = "taxable".into();
                t.active = true;
                if let Some(r) = row {
                    t.code = r[0].parse().unwrap_or(0);
                    t.code_text = r[1].clone();
                    t.code_name = r[2].clone();
                    t.kind = r[3].clone();
                    t.active = r[4] == "1";
                    t.components = query(
                        &self.db,
                        &format!("SELECT item FROM tax_code_items WHERE code={}", t.code),
                    )
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|r| r[0].parse().ok())
                    .collect();
                }
            }
        }
        self.tax_ui.saved(tab);
    }
    fn tax_agencies_form(&mut self, ui: &mut egui::Ui) {
        let agencies=self.rows("SELECT id,name,registration,registered,payable_account,COALESCE(recoverable_account,0) FROM tax_agencies ORDER BY id");
        ui.horizontal_wrapped(|ui| {
            if ui.button("＋ New Agency").clicked() {
                self.tax_ui.selection = Some(Selection::Agency(None));
                self.tax_ui.selection_guard.request();
            }
            for a in &agencies {
                if ui
                    .selectable_label(self.tax_ui.agency.to_string() == a[0], &a[1])
                    .clicked()
                    && self.tax_ui.agency.to_string() != a[0]
                {
                    self.tax_ui.selection = Some(Selection::Agency(Some(a.clone())));
                    self.tax_ui.selection_guard.request();
                }
            }
        });
        field(ui, "Agency Name *", &mut self.tax_ui.name);
        field(ui, "Registration Number", &mut self.tax_ui.registration);
        ui.checkbox(
            &mut self.tax_ui.registered,
            "Registered for sales tax and eligible input recovery",
        );
        let payable=self.rows("SELECT id,code || ' · ' || name FROM accounts WHERE kind='Liability' AND active=1 ORDER BY code");
        let mut recovery=self.rows("SELECT id,code || ' · ' || name FROM accounts WHERE kind='Asset' AND active=1 ORDER BY code");
        recovery.insert(0, vec!["0".into(), "No recovery account".into()]);
        ui.horizontal(|ui| {
            ui.label("Tax Payable Account *");
            picker(ui, "agency_payable", &mut self.tax_ui.payable, &payable);
        });
        ui.horizontal(|ui| {
            ui.label("Recoverable Tax Account");
            picker(
                ui,
                "agency_recoverable",
                &mut self.tax_ui.recoverable,
                &recovery,
            );
        });
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                action = Some(false);
            }
            if ui.button("Save & Close").clicked() {
                action = Some(true);
            }
        });
        if let Some(close) = action {
            let t = &self.tax_ui;
            let result = admin::agency(
                &self.db,
                t.agency,
                &t.name,
                &t.registration,
                t.registered,
                t.payable,
                (t.recoverable > 0).then_some(t.recoverable),
            );
            match result {
                Ok(id) => {
                    self.tax_ui.agency = id;
                    self.tax_ui.saved(self.tax_ui.tab);
                    self.tax_result(Ok(()));
                    if close {
                        self.page = 11;
                    }
                }
                Err(e) => self.tax_result(Err(e)),
            }
        }
    }
    fn tax_rates_form(&mut self, ui: &mut egui::Ui) {
        let mut items = self.rows("SELECT id,name FROM tax_items ORDER BY id");
        items.insert(0, vec!["0".into(), "+ New Tax Item".into()]);
        ui.horizontal(|ui| {
            ui.label("Tax Item");
            picker(ui, "rate_item", &mut self.tax_ui.item, &items);
        });
        if self.tax_ui.item == 0 {
            let agencies = self.rows("SELECT id,name FROM tax_agencies ORDER BY id");
            ui.horizontal(|ui| {
                ui.label("税务机构");
                picker(ui, "item_agency", &mut self.tax_ui.item_agency, &agencies);
            });
            field(ui, "税目名称 *", &mut self.tax_ui.item_name);
            ui.checkbox(
                &mut self.tax_ui.item_recoverable,
                "购买税额允许按资格申请抵扣",
            );
        }
        ui.horizontal(|ui| {
            ui.label("生效日期");
            ui.add(
                egui_extras::DatePickerButton::new(&mut self.tax_ui.effective)
                    .id_salt("tax_effective"),
            );
        });
        field(ui, "税率 % *", &mut self.tax_ui.rate);
        ui.small(
            "新税率追加历史，不覆盖已存在的生效日。相同机构不能在同一税码同时使用 GST 和 HST。",
        );
        let save = ui.button("Save Effective Rate").clicked();
        let close = ui.button("Save & Close").clicked();
        if save || close {
            let t = &self.tax_ui;
            let result = admin::rate(
                &mut self.db,
                t.item,
                t.item_agency,
                &if t.item > 0 {
                    "Existing".into()
                } else {
                    t.item_name.clone()
                },
                t.item_recoverable,
                &t.effective.to_string(),
                &t.rate,
            );
            match result {
                Ok(id) => {
                    self.tax_ui.item = id;
                    self.tax_ui.saved(self.tax_ui.tab);
                    self.tax_result(Ok(()));
                    if close {
                        self.page = 11;
                    }
                }
                Err(e) => self.tax_result(Err(e)),
            }
        }
        let rows=self.rows("SELECT r.id,i.name,r.effective,r.rate FROM tax_rates r JOIN tax_items i ON i.id=r.item ORDER BY i.id,r.effective DESC");
        for row in rows {
            ui.label(format!(
                "{} · {} · {}%",
                row[1],
                row[2],
                tax::format_rate(row[3].parse().unwrap_or(0))
            ));
        }
    }
    fn tax_codes_form(&mut self, ui: &mut egui::Ui) {
        let codes = self.rows("SELECT id,code,name,kind,active FROM tax_codes ORDER BY id");
        ui.horizontal_wrapped(|ui| {
            if ui.button("＋ New Code").clicked() {
                self.tax_ui.selection = Some(Selection::Code(None));
                self.tax_ui.selection_guard.request();
            }
            for code in &codes {
                if ui
                    .selectable_label(self.tax_ui.code.to_string() == code[0], &code[1])
                    .clicked()
                    && self.tax_ui.code.to_string() != code[0]
                {
                    self.tax_ui.selection = Some(Selection::Code(Some(code.clone())));
                    self.tax_ui.selection_guard.request();
                }
            }
        });
        field(ui, "Tax Code *", &mut self.tax_ui.code_text);
        field(ui, "Description *", &mut self.tax_ui.code_name);
        egui::ComboBox::from_id_salt("tax_kind")
            .selected_text(&self.tax_ui.kind)
            .show_ui(ui, |ui| {
                for kind in ["taxable", "zero", "exempt", "none"] {
                    ui.selectable_value(&mut self.tax_ui.kind, kind.into(), kind);
                }
            });
        ui.checkbox(&mut self.tax_ui.active, "启用");
        if self.tax_ui.kind == "taxable" {
            for row in self.rows("SELECT id,name FROM tax_items ORDER BY id") {
                let id = row[0].parse().unwrap_or(0);
                let mut checked = self.tax_ui.components.contains(&id);
                if ui.checkbox(&mut checked, &row[1]).changed() {
                    if checked {
                        self.tax_ui.components.push(id);
                    } else {
                        self.tax_ui.components.retain(|i| *i != id);
                    }
                }
            }
        }
        let save = ui
            .add_enabled(self.tax_ui.code != 0, egui::Button::new("Save"))
            .clicked();
        let close = ui
            .add_enabled(self.tax_ui.code != 0, egui::Button::new("Save & Close"))
            .clicked();
        if save || close {
            let t = &self.tax_ui;
            let result = admin::code(
                &mut self.db,
                t.code,
                &t.code_text,
                &t.code_name,
                &t.kind,
                t.active,
                &t.components,
            );
            match result {
                Ok(id) => {
                    self.tax_ui.code = id;
                    self.tax_ui.saved(self.tax_ui.tab);
                    self.tax_result(Ok(()));
                    if close {
                        self.page = 11;
                    }
                }
                Err(e) => self.tax_result(Err(e)),
            }
        }
    }
    fn tax_assign_form(&mut self, ui: &mut egui::Ui) {
        let old = self.tax_ui.entity.clone();
        ui.horizontal(|ui| {
            for (entity, label) in [
                ("price", "Price List"),
                ("customer", "Customer"),
                ("vendor", "Vendor"),
            ] {
                ui.selectable_value(&mut self.tax_ui.entity, entity.into(), label);
            }
        });
        if old != self.tax_ui.entity {
            self.tax_ui.record = 0;
        }
        let table = match self.tax_ui.entity.as_str() {
            "customer" => "customers",
            "vendor" => "vendors",
            _ => "prices",
        };
        let rows = self.rows(&format!("SELECT id,name FROM {table} ORDER BY id"));
        picker(ui, "tax_assignment_record", &mut self.tax_ui.record, &rows);
        code_picker(
            &self.db,
            ui,
            "assignment_code",
            &mut self.tax_ui.assign_code,
        );
        ui.small("仅影响之后的新业务行：明确行税码优先，其次价格单税码，再其次客户默认税码。已保存行和历史发票不改变。");
        if ui.button("Save Default Tax Code").clicked() {
            let t = &self.tax_ui;
            let result = admin::assign(&self.db, &t.entity, t.record, t.assign_code);
            if result.is_ok() {
                self.tax_ui.saved(3);
            }
            self.tax_result(result);
        }
        let rows=self.rows("SELECT 'Price List',p.name,c.code FROM prices p JOIN tax_codes c ON c.id=p.tax_code UNION ALL SELECT x.entity,CASE x.entity WHEN 'customer' THEN (SELECT name FROM customers WHERE id=x.record) ELSE (SELECT name FROM vendors WHERE id=x.record) END,c.code FROM tax_assignments x JOIN tax_codes c ON c.id=x.code");
        for row in rows {
            ui.label(row.join(" · "));
        }
    }
    fn prepare_tax_review(&mut self) {
        let t = &self.tax_ui;
        let draft = tax::CashDraft {
            agency: t.cash_agency,
            date: t.cash_date.to_string(),
            kind: t.cash_kind.clone(),
            amount: t.amount.clone(),
            reference: t.reference.clone(),
            reason: t.reason.clone(),
        };
        if !self.prepare_gl_fx("CAD", &draft.date) {
            self.tax_ui.error = self.message.clone();
            return;
        }
        match tax::cash_review(&self.db, &draft) {
            Ok(signature) => {
                self.tax_ui.review = Some(TaxReview::Create(draft, signature));
                self.tax_ui.review_focus = true;
                self.tax_ui.error.clear()
            }
            Err(e) => self.tax_ui.error = format!("{e:#}"),
        }
    }
    fn tax_review_dialog(&mut self, ctx: &egui::Context) {
        let Some(review) = self.tax_ui.review.take() else {
            return;
        };
        let (title, values, stale) = match &review {
            TaxReview::Create(d, signature) => {
                let stale = !tax::cash_review(&self.db, d).is_ok_and(|s| &s == signature);
                let name = self
                    .db
                    .query_row(
                        "SELECT name FROM tax_agencies WHERE id=?",
                        [d.agency],
                        |r| r.get::<_, String>(0),
                    )
                    .unwrap_or_else(|_| "Unavailable".into());
                (
                    "Review Tax Record",
                    vec![
                        ("Agency", name),
                        ("Date", d.date.clone()),
                        ("Kind", d.kind.clone()),
                        ("Amount", format!("CAD {}", d.amount)),
                        ("Reference", d.reference.clone()),
                        ("Reason", d.reason.clone()),
                    ],
                    stale,
                )
            }
            TaxReview::Void(id) => {
                let row=self.db.query_row("SELECT a.name,t.date,t.kind,t.amount,t.voided FROM tax_cash t JOIN tax_agencies a ON a.id=t.agency WHERE t.id=?",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,bool>(4)?)));
                match row {
                    Ok((agency, date, kind, amount, voided)) => (
                        "Void Tax Record",
                        vec![
                            ("Agency", agency),
                            ("Original date", date),
                            ("Kind", kind),
                            ("Amount", format!("CAD {}", crate::format_money(amount))),
                            ("Reversal date", crate::today().to_string()),
                        ],
                        voided,
                    ),
                    Err(_) => ("Void Tax Record", vec![], true),
                }
            }
        };
        let error = if stale {
            Some("Source or posting inputs changed. Keep editing and review again.")
        } else if !self.tax_ui.error.is_empty() {
            Some(self.tax_ui.error.as_str())
        } else {
            None
        };
        match controls::review(
            ctx,
            controls::Review {
                cancel: "Keep Editing",
                id: "tax_record_review",
                title,
                values: &values,
                error,
                confirm: if matches!(review, TaxReview::Create(..)) {
                    "Confirm & Record"
                } else {
                    "Confirm & Void"
                },
                notice: "Records an accounting event; it does not send money or file a tax return.",
            },
            &mut self.tax_ui.review_focus,
        ) {
            Some(controls::ReviewAction::Keep) => {}
            Some(controls::ReviewAction::Confirm) => {
                let result = match &review {
                    TaxReview::Create(d, s) => tax::cash_reviewed(&self.db, d, s),
                    TaxReview::Void(id) => tax::void_cash(&self.db, *id),
                };
                let ok = result.is_ok();
                self.tax_result(result);
                if ok {
                    if matches!(review, TaxReview::Create(..)) {
                        self.tax_ui.amount.clear();
                        self.tax_ui.reference.clear();
                        self.tax_ui.reason.clear();
                    }
                } else {
                    self.tax_ui.review = Some(review)
                }
            }
            None => self.tax_ui.review = Some(review),
        }
    }
    pub(super) fn sales_tax(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("期间");
            ui.add(egui_extras::DatePickerButton::new(&mut self.tax_ui.from).id_salt("tax_from"));
            ui.label("至");
            ui.add(egui_extras::DatePickerButton::new(&mut self.tax_ui.to).id_salt("tax_to"));
            if ui.button("Tax Setup").clicked() {
                self.page = 12;
            }
        });
        let from = self.tax_ui.from.to_string();
        let to = self.tax_ui.to.to_string();
        match admin::summary(&self.db, &from, &to) {
            Ok(rows) => {
                ui.strong("Tax Accounts · 税务账户（CAD）");
                let mut selected = 0;
                table(
                    ui,
                    "tax_summary",
                    &[
                        "ID",
                        "Agency",
                        "期间销项税",
                        "期间可抵扣",
                        "期间净税额",
                        "截止日应缴余额",
                    ],
                    &rows,
                    &mut selected,
                );
                if ui.button("Export Summary CSV").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("sales-tax-summary.csv")
                        .save_file()
                    {
                        self.tax_result(admin::export(
                            &rows,
                            &[
                                "ID",
                                "Agency",
                                "Output CAD",
                                "ITC CAD",
                                "Net CAD",
                                "Balance CAD",
                            ],
                            &path,
                        ));
                    }
                }
            }
            Err(e) => {
                ui.label(e.to_string());
            }
        }
        ui.small("余额 = 累计销项 − 合资格进项抵扣 + 调增/退款 − 调减/缴款。负数表示净抵扣或预缴余额。客户收款不改变已开票销项税。");
        ui.collapsing("登记税款缴纳、退款或调整（CAD）",|ui| {
            let agencies=self.rows("SELECT id,name FROM tax_agencies ORDER BY id");picker(ui,"tax_cash_agency",&mut self.tax_ui.cash_agency,&agencies);
            ui.horizontal(|ui|{ui.add(egui_extras::DatePickerButton::new(&mut self.tax_ui.cash_date).id_salt("tax_cash_date"));egui::ComboBox::from_id_salt("tax_cash_kind").selected_text(&self.tax_ui.cash_kind).show_ui(ui,|ui|{for kind in ["payment","refund","increase","decrease"]{ui.selectable_value(&mut self.tax_ui.cash_kind,kind.into(),kind);}});});
            field(ui,"金额 CAD *",&mut self.tax_ui.amount);field(ui,"参考号",&mut self.tax_ui.reference);field(ui,"原因 / 凭证 *",&mut self.tax_ui.reason);ui.small("此处登记已发生的税款记录，不会向银行或税局发起付款。");
            if controls::primary(ui,"Review Tax Record",true).clicked() {self.prepare_tax_review();}

            let records=self.rows("SELECT id,date,kind,printf('%d.%02d',amount/100,amount%100),reference,reason,voided FROM tax_cash ORDER BY date DESC,id DESC");
            for row in records {ui.horizontal_wrapped(|ui|{ui.label(row[1..6].join(" · "));if ui.add_enabled(row[6]=="0",egui::Button::new("Void")).clicked(){if let Ok(id)=row[0].parse::<i64>() {self.tax_ui.review=Some(TaxReview::Void(id));self.tax_ui.review_focus=true;self.tax_ui.error.clear();}}
                if row[6]=="1"{ui.small("已撤销");}});}
        });
        ui.collapsing("Detail Report · 明细", |ui| {
            match admin::detail(&self.db, &from, &to) {
                Ok(rows) => {
                    let mut selected = 0;
                    table(
                        ui,
                        "tax_detail",
                        &[
                            "日期",
                            "机构/税码",
                            "来源",
                            "单据编号",
                            "原币种",
                            "未税金额",
                            "原税额",
                            "CAD税额/缴款变动",
                            "CAD抵扣",
                            "CAD汇率",
                            "汇率日期",
                            "税目快照",
                        ],
                        &rows,
                        &mut selected,
                    );
                    if ui.button("Export Detail CSV").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("sales-tax-detail.csv")
                            .save_file()
                        {
                            self.tax_result(admin::export(
                                &rows,
                                &[
                                    "Date",
                                    "Agency/Code",
                                    "Source",
                                    "Number",
                                    "Currency",
                                    "Net",
                                    "Tax",
                                    "Tax/Cash CAD",
                                    "ITC CAD",
                                    "FX",
                                    "FX date",
                                    "Snapshot",
                                ],
                                &path,
                            ));
                        }
                    }
                }
                Err(e) => {
                    ui.label(e.to_string());
                }
            }
        });
        ui.collapsing("Exception Report · 待核对",|ui|{ui.small("这些单据未指定税码或升级前未保存税额，不代表一定应补税；历史数据不会自动追溯计税。");let rows=self.rows(&format!("SELECT 'Invoice',COALESCE(number,CAST(id AS TEXT)),COALESCE(json_extract(snapshot,'$.invoice_date'),substr(created,1,10)) FROM invoices WHERE COALESCE(json_extract(snapshot,'$.invoice_date'),substr(created,1,10)) BETWEEN '{from}' AND '{to}' AND (json_type(snapshot,'$.tax_total') IS NULL OR EXISTS(SELECT 1 FROM json_each(snapshot,'$.items') WHERE COALESCE(json_extract(value,'$.tax_snapshot.code_id'),0)=0)) UNION ALL SELECT 'Expense',COALESCE(d.number,CAST(d.id AS TEXT)),COALESCE(d.date,'') FROM expense_documents d WHERE COALESCE(d.date,'') BETWEEN '{from}' AND '{to}' AND EXISTS(SELECT 1 FROM expense_document_lines x LEFT JOIN expense_tax t ON t.expense=x.expense WHERE x.document=d.id AND COALESCE(t.code,0)=0)"));let mut selected=0;table(ui,"tax_exceptions",&["来源","编号","日期"],&rows,&mut selected);if ui.button("Export Exceptions CSV").clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("sales-tax-exceptions.csv").save_file(){self.tax_result(admin::export(&rows,&["Source","Number","Date"],&path));}}});
        self.tax_review_dialog(ui.ctx());
        if !self.tax_ui.error.is_empty() {
            ui.colored_label(Color32::from_rgb(190, 45, 45), &self.tax_ui.error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_tabs_and_tax_reports_render() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let ctx = egui::Context::default();
        app.page = 12;
        for tab in 0..4 {
            app.tax_ui.tab = tab;
            let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        }
        app.page = 11;
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        Ok(())
    }
}
