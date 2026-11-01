use super::*;
use crate::gl::{self, Journal, Line};

struct Columns {
    number: f32,
    account: f32,
    money: f32,
    description: f32,
    party: f32,
    tax: f32,
    actions: f32,
    base: f32,
}
impl Columns {
    fn new(available: f32, foreign: bool) -> Self {
        let extra = (available - 1328.0 - if foreign { 240.0 } else { 0.0 }).max(0.0);
        Self {
            number: 28.0,
            account: 240.0 + extra * 0.15,
            money: 150.0 + extra * 0.05,
            description: 220.0 + extra * 0.5,
            party: 210.0 + extra * 0.25,
            tax: 100.0,
            actions: 160.0,
            base: 110.0,
        }
    }
}

#[derive(Default)]
pub(super) struct GlUi {
    pub(super) form: Option<JournalForm>,
    tab: usize,
    account: i64,
    from: String,
    to: String,
    currency: String,
    locked: String,
    roles: Vec<(String, i64)>,
    baseline: Option<String>,
    error: String,
}
impl GlUi {
    fn setup_input(&self) -> String {
        format!("{:?}", (&self.currency, &self.locked, &self.roles))
    }
    pub(super) fn setup_dirty(&self) -> bool {
        self.baseline
            .as_ref()
            .is_some_and(|b| *b != self.setup_input())
    }
}

#[derive(Clone, Default, Debug)]
struct Row {
    account_pending: bool,
    party_pending: bool,
    account: i64,
    debit: String,
    credit: String,
    description: String,
    party: i64,
    tax: String,
    key: u64,
}
enum JournalReview {
    Post(Journal),
    Reverse {
        entry: i64,
        date: String,
        reason: String,
    },
}
pub(super) struct JournalForm {
    journal: Journal,
    date: chrono::NaiveDate,
    rate: String,
    rows: Vec<Row>,
    files: Vec<(String, Vec<u8>)>,
    error: String,
    reverse_date: String,
    reverse_reason: String,
    dirty: bool,
    clear_guard: controls::CloseGuard,
    review: Option<JournalReview>,
    review_focus: bool,
    baseline: String,
}
impl Row {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            key: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ..Default::default()
        }
    }
    fn line(&self) -> Result<Line> {
        Ok(Line {
            account: self.account,
            debit: if self.debit.trim().is_empty() {
                0
            } else {
                crate::parse_money(&self.debit)?
            },
            credit: if self.credit.trim().is_empty() {
                0
            } else {
                crate::parse_money(&self.credit)?
            },
            description: self.description.clone(),
            customer: (self.party > 0).then_some(self.party),
            vendor: (self.party < 0).then_some(-self.party),
            tax_reference: self.tax.clone(),
            ..Default::default()
        })
    }
}
impl JournalForm {
    fn fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                &self.journal,
                self.date,
                &self.rate,
                &self.rows,
                self.files
                    .iter()
                    .map(|(name, data)| (name, data.len()))
                    .collect::<Vec<_>>(),
                &self.reverse_date,
                &self.reverse_reason
            )
        )
    }
    pub(super) fn has_changes(&self) -> bool {
        self.fingerprint() != self.baseline
    }
    pub(super) fn saveable_draft(&self) -> bool {
        self.journal.status == "draft" && self.journal.origin == "manual"
    }
    fn new(journal: Journal) -> Self {
        let mut rows: Vec<Row> = journal
            .lines
            .iter()
            .map(|l| Row {
                account: l.account,
                debit: if l.debit == 0 {
                    String::new()
                } else {
                    crate::format_money(l.debit)
                },
                credit: if l.credit == 0 {
                    String::new()
                } else {
                    crate::format_money(l.credit)
                },
                description: l.description.clone(),
                party: l.customer.unwrap_or(0) - l.vendor.unwrap_or(0),
                tax: l.tax_reference.clone(),
                ..Row::new()
            })
            .collect();
        if journal.origin == "manual" && journal.status == "draft" {
            while rows.len() < 8 {
                rows.push(Row::new());
            }
        }
        let mut form = Self {
            date: chrono::NaiveDate::parse_from_str(&journal.date, "%Y-%m-%d")
                .unwrap_or_else(|_| crate::today()),
            rate: format!(
                "{}.{:08}",
                journal.rate / crate::FX_SCALE,
                journal.rate % crate::FX_SCALE
            ),
            journal,
            rows,
            files: Vec::new(),
            error: String::new(),
            reverse_date: crate::today().to_string(),
            reverse_reason: String::new(),
            dirty: false,
            clear_guard: Default::default(),
            review: None,
            review_focus: false,
            baseline: String::new(),
        };
        form.baseline = form.fingerprint();
        form
    }
    fn draft(&self) -> Result<Journal> {
        anyhow::ensure!(
            !self
                .rows
                .iter()
                .any(|r| r.account_pending || r.party_pending),
            "请从候选列表选定输入中的科目、客户或供应商，或清空该输入"
        );
        let mut j = self.journal.clone();
        j.date = self.date.to_string();
        j.rate = crate::rate(&self.rate)?;
        j.lines = self.rows.iter().map(Row::line).collect::<Result<_>>()?;
        Ok(j)
    }
}
impl Desktop {
    pub(super) fn show_gl_source(&mut self, ui: &mut egui::Ui, kind: &str, id: i64) {
        if id == 0 {
            return;
        }
        let result=self.db.query_row("SELECT s.entry,COALESCE(e.number,'') FROM gl_sources s LEFT JOIN gl_entries e ON e.id=s.entry WHERE s.kind=? AND s.record=?",rusqlite::params![kind,id],|r|Ok((r.get::<_,Option<i64>>(0)?,r.get::<_,String>(1)?))).optional();
        match result {
            Ok(Some((Some(entry), number))) => {
                if ui.button(format!("GL - {number}")).clicked() {
                    self.open_journal(entry);
                }
            }
            Ok(Some((None, _))) => {
                ui.small("No posting is needed for a zero-value document.");
            }
            Ok(None) => {
                ui.small(
                    "Historical document has no GL source. Review opening balances before posting.",
                );
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
            }
        }
    }
    pub(super) fn open_journal(&mut self, id: i64) {
        if self.reuse_business(windows::Kind::Journal) {
            return;
        }
        let result = if id == 0 {
            gl::blank(&self.db)
        } else {
            gl::load(&self.db, id)
        };
        match result {
            Ok(j) => self.gl.form = Some(JournalForm::new(j)),
            Err(e) => self.gl.error = format!("{e:#}"),
        }
    }
    pub(super) fn journals_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.small(format!(
                "本币：{} · 草稿不计入总账；已过账分录只能冲销",
                gl::base(&self.db).unwrap_or_default()
            ));
            if ui.button("General Ledger / Trial Balance").clicked() {
                self.page = 14;
            }
        });
        let rows=self.rows("SELECT e.id,COALESCE(e.number,''),e.date,e.currency,e.origin,CASE WHEN EXISTS(SELECT 1 FROM gl_reversals r WHERE r.entry=e.id) THEN 'reversed' ELSE e.status END,e.memo,printf('%d.%02d',COALESCE(sum(l.debit),0)/100,COALESCE(sum(l.debit),0)%100),printf('%d.%02d',COALESCE(sum(l.base_debit),0)/100,COALESCE(sum(l.base_debit),0)%100) FROM gl_entries e LEFT JOIN gl_lines l ON l.entry=e.id GROUP BY e.id ORDER BY e.date DESC,e.id DESC");
        let rows: Vec<_> = rows.into_iter().filter(|r| self.row_matches(r)).collect();
        let mut selected = 0;
        if table(
            ui,
            "journals",
            &[
                "查看 ID",
                "Journal#",
                "Date",
                "Currency",
                "来源",
                "状态",
                "Memo",
                "借方合计",
                "本币借方",
            ],
            &rows,
            &mut selected,
        ) {
            self.open_journal(selected);
        }
        if !self.gl.error.is_empty() {
            ui.colored_label(Color32::DARK_RED, &self.gl.error);
        }
        ui.small(
            "点击分录查看详情；新单据自动入账。升级前的单据不自动追溯，请按启用范围录入期初分录。",
        );
    }
    pub(super) fn ledger_page(&mut self, ui: &mut egui::Ui) {
        if self.gl.to.is_empty() {
            self.gl.to = crate::today().to_string();
            self.gl.from = format!("{}-01-01", chrono::Datelike::year(&crate::today()));
        }
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.gl.tab, 0, "General Ledger · 总账明细");
            ui.selectable_value(&mut self.gl.tab, 1, "Trial Balance · 试算平衡");
        });
        let base = gl::base(&self.db).unwrap_or_default();
        ui.label(format!(
            "报表币种：{base} · 仅统计已过账分录，包括原分录与冲销；不包含历史未入账单据。"
        ));
        ui.horizontal_wrapped(|ui| {
            if self.gl.tab == 0 {
                ui.label("From");
                ui.add(egui::TextEdit::singleline(&mut self.gl.from).desired_width(110.0));
            }
            ui.label("To / As of");
            ui.add(egui::TextEdit::singleline(&mut self.gl.to).desired_width(110.0));
            if self.gl.tab == 0 {
                let mut accounts =
                    self.rows("SELECT id,code || ' · ' || name FROM accounts ORDER BY code");
                accounts.insert(0, vec!["0".into(), "全部科目".into()]);
                picker(ui, "ledger_account", &mut self.gl.account, &accounts);
            }
        });
        let mut balances = None;
        let result = (|| -> Result<Vec<Vec<String>>> {
            if self.gl.tab == 1 {
                return gl::trial_balance(&self.db, &self.gl.to);
            }
            let (opening, rows, closing) =
                gl::ledger(&self.db, &self.gl.from, &self.gl.to, self.gl.account)?;
            balances = Some((opening, closing));
            Ok(rows)
        })();
        match result {
            Ok(rows) => {
                if self.gl.account > 0 {
                    if let Some((opening, closing)) = balances {
                        ui.strong(format!(
                            "期初 {} · 期末 {} {base}（借方为正、贷方为负）",
                            crate::format_money(opening),
                            crate::format_money(closing)
                        ));
                    }
                }
                let headers = if self.gl.tab == 0 {
                    vec![
                        "查看 ID",
                        "Journal#",
                        "Date",
                        "来源",
                        "Account",
                        "Description",
                        "Currency",
                        "Debit",
                        "Credit",
                        "本币 Debit",
                        "本币 Credit",
                        "本币余额（借+贷-）",
                    ]
                } else {
                    vec!["Account#", "Account name", "Type", "Debit", "Credit"]
                };
                if self.gl.tab == 1 {
                    let totals = rows
                        .iter()
                        .try_fold((0i64, 0i64), |(d, c), r| -> Result<_> {
                            Ok((
                                d.checked_add(crate::parse_money(&r[3])?)
                                    .ok_or_else(|| anyhow::anyhow!("合计过大"))?,
                                c.checked_add(crate::parse_money(&r[4])?)
                                    .ok_or_else(|| anyhow::anyhow!("合计过大"))?,
                            ))
                        });
                    match totals {
                        Ok((d, c)) => {
                            ui.strong(format!(
                                "Debit {} · Credit {} · Difference {} {base}",
                                crate::format_money(d),
                                crate::format_money(c),
                                crate::format_money(d - c)
                            ));
                        }
                        Err(e) => {
                            ui.colored_label(Color32::DARK_RED, e.to_string());
                        }
                    }
                }
                if ui.button("Export CSV").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("Axibiz-GL.csv")
                        .save_file()
                    {
                        match crate::tax::admin::export(&rows, &headers, &path) {
                            Ok(()) => {
                                self.message = "GL 报表已导出".into();
                                self.error = false;
                            }
                            Err(e) => {
                                self.gl.error = e.to_string();
                            }
                        }
                    }
                }
                let mut selected = 0;
                if table(ui, "ledger_report", &headers, &rows, &mut selected) && self.gl.tab == 0 {
                    self.open_journal(selected);
                }
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, format!("{e:#}"));
            }
        }
        if !self.gl.error.is_empty() {
            ui.colored_label(Color32::DARK_RED, &self.gl.error);
        }
    }
    pub(super) fn gl_setup_page(&mut self, ui: &mut egui::Ui) {
        if self.gl.roles.is_empty() {
            self.gl.currency = gl::base(&self.db).unwrap_or_else(|_| "CAD".into());
            self.gl.locked = self
                .db
                .query_row(
                    "SELECT locked_through FROM gl_settings WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            for (role, _, _, _) in gl::ROLES {
                if let Ok(id) = self.db.query_row(
                    "SELECT account FROM gl_accounts WHERE role=?",
                    [role],
                    |r| r.get(0),
                ) {
                    self.gl.roles.push((role.into(), id));
                }
            }
        }
        if self.gl.baseline.is_none() {
            self.gl.baseline = Some(self.gl.setup_input());
        }
        let posted = self
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM gl_entries WHERE status='posted')",
                [],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(true);
        ui.horizontal(|ui| {
            ui.label("GL Base Currency");
            ui.add_enabled_ui(!posted, |ui| {
                currency_picker(ui, "gl_base", &mut self.gl.currency)
            });
        });
        ui.small("本币在首次过账后固定；公司默认结算币种与 GL 本币分别管理。下列关联用于之后的新单据，历史分录保留原科目。");
        field(ui, "Locked Through", &mut self.gl.locked);
        ui.small("格式 YYYY-MM-DD，留空不锁定；锁定期间拒绝过账或更改已入账费用，可用之后日期的冲销或调整分录更正。");
        let mut roles = std::mem::take(&mut self.gl.roles);
        for (role, id) in &mut roles {
            let spec = gl::ROLES.iter().find(|r| r.0 == role).unwrap();
            let mut accounts=self.rows(&format!("SELECT id,code || ' · ' || name FROM accounts WHERE active=1 AND kind='{}' ORDER BY code",spec.3));
            if !accounts.iter().any(|r| r[0] == id.to_string()) {
                accounts.insert(0, vec![id.to_string(), "Select an active account".into()]);
            }
            ui.horizontal(|ui| {
                ui.add_sized(
                    [220.0, 32.0],
                    egui::Label::new(format!("{} / {}", spec.0, spec.2)),
                );
                picker(ui, role, id, &accounts);
            });
        }
        self.gl.roles = roles;
        if ui.button("Save GL Setup").clicked() {
            match gl::configure(
                &mut self.db,
                &self.gl.currency,
                &self.gl.roles,
                &self.gl.locked,
            ) {
                Ok(()) => {
                    self.gl.error.clear();
                    self.message = "GL 设置已保存".into();
                    self.error = false;
                    self.gl.baseline = Some(self.gl.setup_input());
                }
                Err(e) => self.gl.error = format!("{e:#}"),
            }
        }
        if !self.gl.error.is_empty() {
            ui.colored_label(Color32::DARK_RED, &self.gl.error);
        }
        ui.separator();
        ui.label("自动分录：Invoice 借应收、贷收入及销项税；收款借银行、贷应收；Expense 借费用及可抵扣税、贷应付；税款按缴款或调整生成分录。多币种收款自动记录已实现汇兑差额。");
        ui.small("Use Pay Bills to allocate supplier payments against reviewed bills. Manual control-account entries need reconciliation; manual tax lines do not update the sales tax subledger.");
    }
    pub(super) fn save_journal_form(&mut self, form: &mut JournalForm, post: bool) -> bool {
        let result = (|| -> Result<Journal> {
            let draft = form.draft()?;
            let id = if form.files.is_empty() {
                gl::save(&mut self.db, &draft, post)?
            } else {
                gl::save_with_files(&mut self.db, &draft, post, &form.files)?
            };
            gl::load(&self.db, id)
        })();
        match result {
            Ok(j) => {
                *form = JournalForm::new(j);
                self.message = if post {
                    "分录已过账"
                } else {
                    "分录草稿已保存"
                }
                .into();
                self.error = false;
                true
            }
            Err(e) => {
                form.error = format!("{e:#}");
                false
            }
        }
    }
    pub(super) fn journal_editor(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.gl.form.take() else {
            return;
        };
        let mut close = false;
        let accounts=self.rows("SELECT id,code || ' · ' || name || CASE active WHEN 0 THEN '（停用）' ELSE '' END,name || ' · ' || kind FROM accounts ORDER BY code");
        let mut parties =
            self.rows("SELECT id,name,'Customer · ' || currency FROM customers ORDER BY name");
        parties.extend(
            self.rows("SELECT -id,name,'Vendor · ' || currency FROM vendors ORDER BY name"),
        );
        parties.insert(0, vec!["0".into(), "无关联".into()]);
        let base = gl::base(&self.db).unwrap_or_default();
        let locked = form.journal.status == "posted";
        if !locked && form.rate.trim().is_empty() {
            match gl::quote(&self.db, &form.journal.currency, &form.date.to_string()) {
                Ok(rate) => {
                    form.rate = format!("{}.{:08}", rate / crate::FX_SCALE, rate % crate::FX_SCALE);
                    form.dirty = true;
                    form.error.clear();
                }
                Err(_) => self.request_dated_fx(ctx, &form.date.to_string()),
            }
        }
        let historical = if locked {
            self.rows(&format!("SELECT account,account_code || ' · ' || account_name FROM gl_lines WHERE entry={} ORDER BY position",form.journal.id))
        } else {
            Vec::new()
        };
        let response = windows::surface(ctx, egui::Id::new("journal_editor"), |ui| {
            ui.set_width((ctx.content_rect().width() - 48.0).clamp(320.0, 1500.0));
            ui.horizontal_wrapped(|ui| {
                ui.heading(format!(
                    "Journal Entry {}",
                    if form.journal.number.is_empty() {
                        "· New"
                    } else {
                        &form.journal.number
                    }
                ));
                theme::badge(ui, &form.journal.status);
                ui.small(format!("Source: {}", form.journal.origin));
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("journal_body").max_height((ctx.content_rect().height()-320.0).max(120.0)).show(ui,|ui| {
                ui.add_enabled_ui(!locked,|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Currency");
                        let old=form.journal.currency.clone();
                        currency_picker(ui,"journal_currency",&mut form.journal.currency);
                        if old!=form.journal.currency {
                            form.dirty=true;
                            match gl::quote(&self.db,&form.journal.currency,&form.date.to_string()) {
                                Ok(r)=>form.rate=format!("{}.{:08}",r/crate::FX_SCALE,r%crate::FX_SCALE),
                                Err(e)=>{form.rate=String::new();form.error=e.to_string();}
                            }
                        }
                        ui.label("Journal date");
                        form.dirty|=ui.add(egui_extras::DatePickerButton::new(&mut form.date)).changed();
                        ui.label(format!("1 {} =",form.journal.currency));
                        form.dirty|=ui.add(egui::TextEdit::singleline(&mut form.rate).desired_width(110.0)).changed();
                        ui.label(&base);
                        if ui.button("Use saved rate").clicked() {
                            match gl::quote(&self.db,&form.journal.currency,&form.date.to_string()) {
                                Ok(r)=>{form.rate=format!("{}.{:08}",r/crate::FX_SCALE,r%crate::FX_SCALE);form.dirty=true;form.error.clear();}
                                Err(e)=>{form.error=e.to_string();self.request_dated_fx(ctx,&form.date.to_string());}
                            }
                        }
                    });
                });
                ui.small("Journal# 在首次保存时分配，可在 Setup → Numbering 自定义前缀。借贷按原币及 GL 本币同时校验。");
                ui.add_space(12.0);
                let mut remove=None;
                let mut copy=None;
                let converted=if locked {form.journal.lines.clone()}else{form.draft().and_then(|j|gl::preview_lines(&self.db,&j)).unwrap_or_default()};
                let keys:Vec<_>=form.rows.iter().filter(|r|r.account!=0 || !r.debit.trim().is_empty() || !r.credit.trim().is_empty() || !r.description.trim().is_empty() || r.party!=0 || !r.tax.is_empty()).map(|r|r.key).collect();
                let amounts:std::collections::BTreeMap<_,_>=keys.into_iter().zip(converted.iter().map(|l|(l.base_debit,l.base_credit))).collect();
                let foreign=form.journal.currency!=base;
                let columns=Columns::new(ui.available_width(),foreign);
                let picker_gap=crate::autocomplete::ARROW_WIDTH+ui.spacing().item_spacing.x;
                #[cfg(test)]
                ctx.data_mut(|d|d.insert_temp(egui::Id::new("gl_table_available"),ui.available_width()));
                egui::ScrollArea::horizontal().id_salt("journal_lines_scroll").show(ui,|ui| {
                    let grid=egui::Grid::new("journal_lines").striped(true).min_col_width(0.0).spacing([10.0,8.0]).show(ui,|ui| {
                        let mut headers=vec![("#".into(),columns.number),("Account *".into(),columns.account),("Debits".into(),columns.money),("Credits".into(),columns.money)];
                        if foreign {headers.extend([(format!("Debit ({base})"),columns.base),(format!("Credit ({base})"),columns.base)]);}
                        headers.extend([("Description".into(),columns.description),("Name（可选）".into(),columns.party),("Tax reference".into(),columns.tax),("操作".into(),columns.actions)]);
                        for (label,width) in headers {ui.add_sized([width,28.0],egui::Label::new(RichText::new(&label).strong()).truncate()).on_hover_text(label);}
                        ui.end_row();
                        for (i,row) in form.rows.iter_mut().enumerate() {
                            ui.add_sized([columns.number,32.0],egui::Label::new((i+1).to_string()));
                            if locked {
                                let label=historical.get(i).map(|r|r[1].as_str()).unwrap_or("");
                                ui.add_sized([columns.account,32.0],egui::Label::new(label).truncate()).on_hover_text(label);
                            }else {
                                let old=row.account;
                                let outcome=picker_with_width(ui,&format!("journal_account_{}",row.key),&mut row.account,&accounts,columns.account-picker_gap);
                                form.dirty|=old!=row.account || outcome.pending!=row.account_pending;
                                row.account_pending=outcome.pending;
                            }
                            for (side,value) in [("debit",&mut row.debit),("credit",&mut row.credit)] {
                                let response=ui.add_sized([columns.money,32.0],egui::TextEdit::singleline(value).id(egui::Id::new((side,row.key))).horizontal_align(egui::Align::RIGHT).interactive(!locked));
                                #[cfg(test)]
                                ctx.data_mut(|d|d.insert_temp(egui::Id::new(("gl_cell",side,row.key)),response.rect));
                                form.dirty|=response.changed();
                            }
                            if foreign {
                                let (d,c)=amounts.get(&row.key).copied().unwrap_or_default();
                                ui.add_sized([columns.base,32.0],egui::Label::new(crate::format_money(d)));ui.add_sized([columns.base,32.0],egui::Label::new(crate::format_money(c)));
                            }
                            form.dirty|=ui.add_sized([columns.description,32.0],egui::TextEdit::singleline(&mut row.description).id(egui::Id::new(("description",row.key))).interactive(!locked)).changed();
                            if locked {let label=parties.iter().find(|p|p[0]==row.party.to_string()).map(|p|p[1].as_str()).unwrap_or("");ui.add_sized([columns.party,32.0],egui::Label::new(label).truncate()).on_hover_text(label);}
                            else {let old=row.party;let outcome=picker_with_width(ui,&format!("journal_party_{}",row.key),&mut row.party,&parties,columns.party-picker_gap);form.dirty|=old!=row.party || outcome.pending!=row.party_pending;row.party_pending=outcome.pending;}
                            form.dirty|=ui.add_sized([columns.tax,32.0],egui::TextEdit::singleline(&mut row.tax).id(egui::Id::new(("tax",row.key))).interactive(!locked)).on_hover_text("仅为分录备注；税款调整请使用 Sales Tax。").changed();
                            ui.allocate_ui_with_layout(egui::vec2(columns.actions,32.0),egui::Layout::left_to_right(egui::Align::Center),|ui| {
                                if ui.add_enabled_ui(!locked,|ui|icons::button(ui,icons::Icon::Copy,"Duplicate line")).inner.clicked(){copy=Some(i);}
                                if ui.add_enabled_ui(!locked,|ui| theme::trash_button(ui,"Delete line")).inner.clicked(){remove=Some(i);}
                            });
                            ui.end_row();
                        }
                    });
                    #[cfg(test)]
                    ctx.data_mut(|d|d.insert_temp(egui::Id::new("gl_table_rect"),grid.response.rect));
                    #[cfg(not(test))]
                    let _=grid;
                });
                if let Some(i)=copy {
                    if let Some(row)=form.rows.get(i){let mut row=row.clone();row.key=Row::new().key;form.rows.insert(i+1,row);form.dirty=true;}
                }
                if let Some(i)=remove {form.rows.remove(i);form.dirty=true;}
                ui.horizontal(|ui| {
                    if ui.add_enabled(!locked,egui::Button::new("+ Add lines")).clicked(){form.rows.push(Row::new());form.dirty=true;}
                    if ui.add_enabled(!locked,egui::Button::new("Clear all lines")).clicked(){form.clear_guard.request();}
                });
                ui.separator();
                ui.label("Memo");
                form.dirty|=ui.add(egui::TextEdit::multiline(&mut form.journal.memo).desired_rows(3).desired_width(ui.available_width()).interactive(!locked)).changed();
                if ui.add_enabled(!locked,egui::Button::new("Add attachment · 最大 20 MB")).clicked() {
                    if let Some(path)=rfd::FileDialog::new().pick_file() {
                        let result=(||->Result<_>{anyhow::ensure!(std::fs::metadata(&path)?.len()<=20*1024*1024,"附件超过 20 MB");Ok((path.file_name().unwrap_or_default().to_string_lossy().into_owned(),std::fs::read(path)?))})();
                        match result {Ok(file)=>{form.files.push(file);form.dirty=true;},Err(e)=>form.error=e.to_string()}
                    }
                }
                let mut remove=None;
                for (i,(name,data)) in form.files.iter().enumerate() {
                    ui.horizontal(|ui|{ui.label(format!("待保存：{name} · {} KB",data.len()/1024));if ui.small_button("Remove").clicked(){remove=Some(i);}});
                }
                if let Some(i)=remove{form.files.remove(i);}
                if form.journal.id>0 {
                    let files=self.rows(&format!("SELECT id,name,length(data) FROM gl_attachments WHERE entry={} ORDER BY id",form.journal.id));
                    for file in files {
                        ui.horizontal(|ui| {
                            ui.label(format!("附件：{} · {} bytes",file[1],file[2]));
                            if ui.button("Save As").clicked() {
                                if let Some(path)=rfd::FileDialog::new().set_file_name(&file[1]).save_file() {
                                    let result=(||->Result<()>{use std::io::Write;let data:Vec<u8>=self.db.query_row("SELECT data FROM gl_attachments WHERE id=?",[file[0].parse::<i64>()?],|r|r.get(0))?;let mut output=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;output.write_all(&data)?;Ok(())})();
                                    if let Err(e)=result{form.error=e.to_string();}
                                }
                            }
                        });
                    }
                }
                if locked && form.journal.origin=="manual" {
                    let reversed=self.db.query_row("SELECT EXISTS(SELECT 1 FROM gl_reversals WHERE entry=?)",[form.journal.id],|r|r.get::<_,bool>(0)).unwrap_or(true);
                    if reversed {ui.label("该分录已经冲销。");}
                    else {
                        ui.collapsing("Reverse Entry",|ui| {
                            field(ui,"Reversal date",&mut form.reverse_date);
                            field(ui,"Reason *",&mut form.reverse_reason);
                            if ui.button("Review Reversal").clicked() {
                                form.review=Some(JournalReview::Reverse{entry:form.journal.id,date:form.reverse_date.clone(),reason:form.reverse_reason.clone()});form.review_focus=true;form.error.clear();
                            }
                        });
                    }
                }
            });
            ui.separator();
            if locked {
                let (d, c) = gl::totals(&form.journal.lines).unwrap_or_default();
                let bd = form
                    .journal
                    .lines
                    .iter()
                    .map(|l| i128::from(l.base_debit))
                    .sum::<i128>();
                let bc = form
                    .journal
                    .lines
                    .iter()
                    .map(|l| i128::from(l.base_credit))
                    .sum::<i128>();
                let bd = i64::try_from(bd)
                    .map(crate::format_money)
                    .unwrap_or_else(|_| "过大".into());
                let bc = i64::try_from(bc)
                    .map(crate::format_money)
                    .unwrap_or_else(|_| "过大".into());
                ui.strong(format!(
                    "Total {} · Debit {} / Credit {} · 本币 {base} {bd} / {bc}",
                    form.journal.currency,
                    crate::format_money(d),
                    crate::format_money(c)
                ));
            } else {
                match form
                    .draft()
                    .and_then(|j| gl::totals(&j.lines).map(|totals| (j, totals)))
                {
                    Ok((j, (d, c))) => {
                        ui.colored_label(
                            if d == c && d > 0 {
                                theme::GREEN
                            } else {
                                Color32::DARK_RED
                            },
                            format!(
                                "Total {} · Debit {} / Credit {} · Difference {}",
                                j.currency,
                                crate::format_money(d),
                                crate::format_money(c),
                                crate::format_money(d - c)
                            ),
                        );
                        if let (Ok(d), Ok(c)) =
                            (crate::convert(d, j.rate), crate::convert(c, j.rate))
                        {
                            ui.small(format!(
                                "本币 {base} · Debit {} / Credit {}",
                                crate::format_money(d),
                                crate::format_money(c)
                            ));
                        }
                    }
                    Err(e) => {
                        ui.colored_label(Color32::DARK_RED, e.to_string());
                    }
                }
            }
            if !form.error.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("journal_error")
                    .max_height(48.0)
                    .show(ui, |ui| {
                        ui.colored_label(Color32::DARK_RED, &form.error);
                    });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if locked {
                    if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Close").clicked() {
                        close = true;
                    }
                } else {
                    let post = controls::primary(ui,"Review Posting",true);
                    #[cfg(test)]
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("gl_post_rect"), post.rect));
                    if post.clicked() {
                        match form.draft().and_then(|j| {gl::preview_posting(&self.db,&j)?;Ok(j)}) {
                            Ok(j)=>{form.review=Some(JournalReview::Post(j));form.review_focus=true;form.error.clear();},
                            Err(e)=>form.error=format!("{e:#}"),
                        }
                    }
                    if ui.button("Save Draft & Close").clicked()
                        && self.save_journal_form(&mut form, false)
                    {
                        close = true;
                    }
                    if ui.button("Save Draft").clicked() {
                        self.save_journal_form(&mut form, false);
                    }
                    if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Cancel").clicked()
                    {
                        self.request_business_close(windows::Kind::Journal);
                    }
                    ui.small("Save Draft keeps an unposted draft. Posting requires review and confirmation.");
                }
            });
        });
        if let Some(review) = form.review.take() {
            let (title, values) = match &review {
                JournalReview::Post(j) => {
                    let (debit, credit) = gl::totals(&j.lines).unwrap_or_default();
                    (
                        "Review Journal Posting",
                        vec![
                            ("Date", j.date.clone()),
                            ("Currency", j.currency.clone()),
                            ("Debit", crate::format_money(debit)),
                            ("Credit", crate::format_money(credit)),
                            ("Memo", j.memo.clone()),
                        ],
                    )
                }
                JournalReview::Reverse { date, reason, .. } => (
                    "Review Journal Reversal",
                    vec![
                        ("Journal", form.journal.number.clone()),
                        ("Date", date.clone()),
                        ("Reason", reason.clone()),
                    ],
                ),
            };
            match controls::review(ctx,controls::Review{ cancel: "Keep Editing",id:"journal_review",title,values:&values,error:(!form.error.is_empty()).then_some(form.error.as_str()),confirm:if matches!(review,JournalReview::Post(_)){"Confirm & Post"}else{"Confirm & Reverse"},notice:"Creates posted accounting entries. Closing this window does not post or reverse an entry."},&mut form.review_focus) {
                Some(controls::ReviewAction::Keep)=>{},
                Some(controls::ReviewAction::Confirm)=>{
                    let ok=match &review {
                        JournalReview::Post(j)=>{
                            if form.draft().is_ok_and(|current|&current==j) {self.save_journal_form(&mut form,true)} else {form.error="Journal input changed. Review it again.".into();false}
                        },
                        JournalReview::Reverse{entry,date,reason}=>match gl::reverse(&mut self.db,*entry,date,reason).and_then(|id|gl::load(&self.db,id)) {Ok(j)=>{form=JournalForm::new(j);true},Err(e)=>{form.error=format!("{e:#}");false}},
                    };
                    if !ok {form.review=Some(review)}
                },
                None=>form.review=Some(review),
            }
        }
        let dirty = form.has_changes();
        if form.clear_guard.show(ctx, "journal_clear", dirty) {
            form.rows = vec![Row::new(), Row::new()];
            form.dirty = true;
        }
        if controls::escape(ctx, response.is_top_modal, response.any_popup_open) {
            self.request_business_close(windows::Kind::Journal);
        }
        if !close {
            self.gl.form = Some(form);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posting_review_does_not_write_and_final_post_rechecks_period() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_journal(0);
        let mut form = app.gl.form.take().unwrap();
        form.rows[0].account = gl::role(&app.db, "bank")?;
        form.rows[0].debit = "10".into();
        form.rows[1].account = gl::role(&app.db, "revenue")?;
        form.rows[1].credit = "10".into();
        let draft = form.draft()?;
        let changes = app.db.total_changes();
        let rows = gl::preview_posting(&app.db, &draft)?;
        assert_eq!(gl::totals(&rows)?, (1000, 1000));
        assert_eq!(app.db.total_changes(), changes);
        app.db.execute(
            "UPDATE gl_settings SET locked_through=? WHERE id=1",
            [&draft.date],
        )?;
        assert!(!app.save_journal_form(&mut form, true));
        assert_eq!(form.rows[0].debit, "10");
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM gl_entries", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        app.db
            .execute("UPDATE gl_settings SET locked_through='' WHERE id=1", [])?;
        assert!(app.save_journal_form(&mut form, true), "{}", form.error);
        crate::storage::validate_projections(&app.db)?;
        Ok(())
    }
    #[test]
    fn journal_close_detection_includes_reversal_input_and_draft_save_does_not_post() -> Result<()>
    {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_journal(0);
        let mut form = app.gl.form.take().unwrap();
        assert!(!form.has_changes());
        form.journal.memo = "Draft for review".into();
        form.rows[0].account = crate::gl::role(&app.db, "bank")?;
        form.rows[0].debit = "10".into();
        form.rows[1].account = crate::gl::role(&app.db, "revenue")?;
        form.rows[1].credit = "10".into();
        assert!(form.has_changes());
        assert!(form.saveable_draft());
        assert!(app.save_journal_form(&mut form, false), "{}", form.error);
        assert_eq!(form.journal.status, "draft");
        assert!(!form.has_changes());
        assert!(app.save_journal_form(&mut form, true), "{}", form.error);
        assert!(!form.saveable_draft());
        form.reverse_reason = "Keep reversal explanation".into();
        assert!(
            form.has_changes(),
            "A posted journal's unsaved reversal input still needs close protection"
        );
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM gl_reversals", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn journal_singleton_preserves_dirty_input_when_other_entry_is_requested() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_journal(0);
        let form = app.gl.form.as_mut().unwrap();
        form.journal.memo = "Unsaved journal".into();
        form.rows[0].description = "Keep this line".into();
        form.dirty = true;
        app.open_journal(0);
        app.open_journal(999999);
        let form = app.gl.form.as_ref().unwrap();
        assert_eq!(form.journal.memo, "Unsaved journal");
        assert_eq!(form.rows[0].description, "Keep this line");
        assert!(form.dirty);
        assert_eq!(
            app.db
                .query_row("SELECT count(*) FROM gl_entries", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn journal_columns_fill_wide_windows_and_keep_amount_fields_wide() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 13;
        app.open_journal(0);
        let key = app.gl.form.as_ref().unwrap().rows[0].key;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for (width, currency) in [
            (1400.0, "CAD"),
            (1920.0, "CAD"),
            (1000.0, "CAD"),
            (1400.0, "USD"),
        ] {
            let form = app.gl.form.as_mut().unwrap();
            form.journal.currency = currency.into();
            form.rate = if currency == "CAD" { "1" } else { "1.35" }.into();
            for _ in 0..4 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.draw(ctx),
                );
            }
            for side in ["debit", "credit"] {
                let rect = ctx
                    .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new(("gl_cell", side, key))))
                    .unwrap();
                assert!(
                    rect.width() >= 149.0,
                    "{side} too narrow at {width}: {rect:?}"
                );
            }
            let description = ctx
                .read_response(egui::Id::new(("description", key)))
                .unwrap()
                .rect;
            assert!(
                description.width() >= 219.0,
                "description should not collapse to header width"
            );
            let id = ctx
                .data_mut(|d| {
                    d.get_temp::<egui::Id>(egui::Id::new((
                        "picker_id",
                        format!("journal_account_{key}"),
                    )))
                })
                .unwrap();
            let (input, arrow) = ctx.data_mut(|d| {
                (
                    d.get_temp::<egui::Rect>(id.with("input_rect")).unwrap(),
                    d.get_temp::<egui::Rect>(id.with("arrow_rect")).unwrap(),
                )
            });
            assert!(
                (input.top() - arrow.top()).abs() < 1.0
                    && (input.height() - arrow.height()).abs() < 1.0,
                "picker input and arrow should align"
            );
            if currency == "CAD" && width >= 1400.0 {
                let available = ctx
                    .data_mut(|d| d.get_temp::<f32>(egui::Id::new("gl_table_available")))
                    .unwrap();
                let table = ctx
                    .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("gl_table_rect")))
                    .unwrap();
                assert!(
                    (available - table.width()).abs() < 8.0,
                    "table should fill viewport: available={available}, actual={}",
                    table.width()
                );
            }
        }
        Ok(())
    }
    #[test]
    fn journal_account_arrow_opens_and_selects_in_scrolled_grid() -> Result<()> {
        for size in [
            egui::vec2(1400.0, 900.0),
            egui::vec2(1000.0, 650.0),
            egui::vec2(900.0, 600.0),
        ] {
            let db = Connection::open_in_memory()?;
            crate::init(&db)?;
            let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
            app.page = 13;
            app.open_journal(0);
            let key = app.gl.form.as_ref().unwrap().rows[0].key;
            let bank = gl::role(&app.db, "bank")?;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let frame = |app: &mut Desktop, events| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.draw(ctx),
                );
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let id = ctx
                .data_mut(|d| {
                    d.get_temp::<egui::Id>(egui::Id::new((
                        "picker_id",
                        format!("journal_account_{key}"),
                    )))
                })
                .unwrap();
            let arrow = ctx
                .data_mut(|d| d.get_temp::<egui::Rect>(id.with("arrow_rect")))
                .unwrap()
                .center();
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
            frame(&mut app, click(arrow, true));
            frame(&mut app, click(arrow, false));
            frame(&mut app, vec![]);
            assert!(
                egui::Popup::is_id_open(&ctx, id.with("popup")),
                "JE arrow should keep popup open after click"
            );
            let candidate = ctx
                .data_mut(|d| d.get_temp::<egui::Rect>(id.with(("choice_rect", bank))))
                .expect("bank candidate must render")
                .center();
            frame(&mut app, click(candidate, true));
            frame(&mut app, click(candidate, false));
            assert_eq!(
                app.gl.form.as_ref().unwrap().rows[0].account,
                bank,
                "JE account candidate should be selectable"
            );
        }
        Ok(())
    }
    #[test]
    fn journal_grid_layout_footer_and_failed_post_preserve_draft() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.page = 13;
        app.open_journal(0);
        let mut form = app.gl.form.take().unwrap();
        form.rows[0].account = gl::role(&app.db, "bank")?;
        form.rows[0].debit = "100".into();
        form.rows[1].account = gl::role(&app.db, "revenue")?;
        form.rows[1].credit = "90".into();
        form.dirty = true;
        let keys = [form.rows[0].key, form.rows[1].key];
        app.gl.form = Some(form);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        }
        let rect = |side: &str, key: u64| {
            ctx.data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new(("gl_cell", side, key))))
                .unwrap()
        };
        assert!(rect("debit", keys[0]).left() < rect("credit", keys[0]).left());
        assert!((rect("debit", keys[0]).top() - rect("credit", keys[0]).top()).abs() < 2.0);
        assert!(rect("debit", keys[1]).top() > rect("debit", keys[0]).top());
        let post = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("gl_post_rect")))
            .unwrap();
        assert!(
            screen.contains_rect(post),
            "post footer outside viewport: {post:?}"
        );
        app.gl.form.as_mut().unwrap().error = "Long validation error · ".repeat(100);
        for _ in 0..20 {
            app.gl.form.as_mut().unwrap().rows.push(Row::new());
        }
        let small = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 650.0));
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(small),
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        }
        let post = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("gl_post_rect")))
            .unwrap();
        assert!(
            small.contains_rect(post),
            "footer outside small viewport with long error: {post:?}"
        );
        let mut form = app.gl.form.take().unwrap();
        assert!(!app.save_journal_form(&mut form, true));
        assert_eq!(form.rows[1].credit, "90");
        assert!(!form.error.is_empty());
        assert_eq!(form.journal.id, 0);
        form.rows[1].credit = "100".into();
        assert!(app.save_journal_form(&mut form, false), "{}", form.error);
        let id = form.journal.id;
        assert!(id > 0);
        assert_eq!(form.journal.status, "draft");
        assert!(app.save_journal_form(&mut form, false));
        assert_eq!(form.journal.id, id);
        assert!(app.save_journal_form(&mut form, true), "{}", form.error);
        assert_eq!(form.journal.status, "posted");
        app.gl.form = Some(form);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| app.draw(ctx),
        );
        app.gl.form = None;
        for page in [13, 14, 15] {
            app.page = page;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| app.draw(ctx),
            );
        }
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM gl_entries", [], |r| r
                    .get::<_, i64>(0))?,
            1
        );
        Ok(())
    }
}
