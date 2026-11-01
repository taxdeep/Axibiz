use super::*;
pub(super) enum Review {
    Cancel(i64),
    Save {
        close: bool,
        complete: bool,
        input: String,
        signature: String,
        amount: i64,
        currency: String,
    },
}
impl Desktop {
    fn task_input(&self) -> String {
        format!(
            "{:?}",
            (
                self.task,
                self.item,
                &self.item_price,
                &self.item_cost,
                self.quantity,
                &self.task_item_edits,
                &self.task_tax_edits,
                &self.chosen,
                &self.expense,
                &self.expense_amount,
                self.expense_vendor,
                &self.task_expense_tax
            )
        )
    }
    fn task_expense_document(&self) -> Result<crate::expense_doc::Document> {
        let currency =
            self.db
                .query_row("SELECT currency FROM tasks WHERE id=?", [self.task], |r| {
                    r.get::<_, String>(0)
                })?;
        let date = crate::today().to_string();
        Ok(crate::expense_doc::Document {
            id: 0,
            vendor: Some(self.expense_vendor),
            date: date.clone(),
            currency: currency.clone(),
            reference: String::new(),
            due_date: None,
            memo: String::new(),
            terms: String::new(),
            lines: vec![crate::accounting::Expense {
                id: 0,
                date,
                description: self.expense.clone(),
                account: crate::accounting::default_account(&self.db)?,
                vendor: Some(self.expense_vendor),
                currency,
                amount: self.expense_amount.clone(),
                task: Some(self.task),
                tax: self.task_expense_tax.clone(),
            }],
        })
    }
    fn task_has_expense(&self) -> bool {
        !self.expense.trim().is_empty()
            || self.expense_vendor > 0
            || self.task_expense_tax != Default::default()
            || !crate::parse_money(&self.expense_amount).is_ok_and(|v| v == 0)
    }
    fn request_task_save(&mut self, close: bool, complete: bool) {
        if !self.task_has_expense() {
            if self.save_task_editor(close) && complete {
                self.action(Command::TaskComplete { task: self.task });
            }
            return;
        }
        let result = (|| -> Result<Review> {
            let doc = self.task_expense_document()?;
            anyhow::ensure!(
                self.prepare_gl_fx(&doc.currency, &doc.date),
                "{}",
                self.message
            );
            let review = crate::expense_doc::review(&self.db, &doc)?;
            Ok(Review::Save {
                close,
                complete,
                input: self.task_input(),
                signature: review.signature,
                amount: review.gross,
                currency: doc.currency,
            })
        })();
        match result {
            Ok(r) => {
                self.task_review = Some(r);
                self.task_review_focus = true;
                self.error = false;
                self.message.clear()
            }
            Err(e) => {
                self.message = format!("{e:#}");
                self.error = true;
            }
        }
    }

    pub(super) fn open_task_editor(&mut self, id: i64) {
        if self.reuse_business(windows::Kind::Task) {
            return;
        }
        match self
            .db
            .query_row("SELECT id FROM tasks WHERE id=?", [id], |r| {
                r.get::<_, i64>(0)
            }) {
            Ok(_) => {
                self.task = id;
                self.item = 0;
                self.item_price.clear();
                self.item_cost.clear();
                self.task_item_edits.clear();
                self.task_tax_edits.clear();
                self.item_tax_code = 0;
                self.quantity = 1;
                self.chosen.clear();
                self.expense.clear();
                self.expense_amount = "0.00".into();
                self.expense_vendor = 0;
                self.task_expense_tax = Default::default();
                self.task_editor_open = true;
            }
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
            }
        }
    }
    pub(super) fn task_editor_dialog(&mut self, ctx: &egui::Context) {
        if !self.task_editor_open {
            return;
        }
        let response = windows::surface(ctx, egui::Id::new("task_editor_modal"), |ui| {
            ui.set_width(1060.0_f32.min((ctx.content_rect().width() - 48.0).max(300.0)));
            ui.horizontal(|ui| {
                let number = self
                    .db
                    .query_row("SELECT number FROM tasks WHERE id=?", [self.task], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap_or_else(|_| self.task.to_string());
                ui.heading(format!("Edit Task {number}"));
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("task_editor_body")
                .max_height((ctx.content_rect().height() - 190.0).max(180.0))
                .show(ui, |ui| self.task_details(ui));
            if !self.message.is_empty() {
                ui.colored_label(
                    if self.error {
                        Color32::from_rgb(190, 45, 45)
                    } else {
                        Color32::from_rgb(20, 110, 85)
                    },
                    &self.message,
                );
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if controls::primary(ui, "Save & Close", true).clicked() {
                    self.request_task_save(true, false);
                }
                if ui.button("Save").clicked() {
                    self.request_task_save(false, false);
                }
                ui.small("统一保存明细修改、勾选业务和新费用。");
            });
        });
        if let Some(review) = self.task_review.take() {
            let (title, values, error) = match &review {
                Review::Cancel(id) => ("Cancel Task", vec![("Task", id.to_string())], None),
                Review::Save {
                    input,
                    amount,
                    currency,
                    ..
                } => (
                    "Review Task Expense",
                    vec![
                        ("Task", self.task.to_string()),
                        ("Expense", self.expense.clone()),
                        (
                            "Amount",
                            format!("{} {}", currency, crate::format_money(*amount)),
                        ),
                    ],
                    if input != &self.task_input() {
                        Some("Task input changed. Review it again.")
                    } else {
                        None
                    },
                ),
            };
            match controls::review(
                ctx,
                controls::Review {
                    cancel: if matches!(review, Review::Cancel(_)) {
                        "Keep Task"
                    } else {
                        "Keep Editing"
                    },
                    id: "task_review",
                    title,
                    values: &values,
                    error,
                    confirm: if matches!(review, Review::Cancel(_)) {
                        "Confirm Cancellation"
                    } else {
                        "Confirm & Record"
                    },
                    notice: if matches!(review, Review::Cancel(_)) {
                        "Cancels the saved task and discards pending input. Existing recorded expenses remain."
                    } else {
                        "Saves task changes and records this expense with its accounting entries."
                    },
                },
                &mut self.task_review_focus,
            ) {
                Some(controls::ReviewAction::Keep) => {}
                Some(controls::ReviewAction::Confirm) => match &review {
                    Review::Cancel(id) => {
                        if self.action(Command::TaskCancel { task: *id }) {
                            self.task_editor_open = false;
                        } else {
                            self.task_review = Some(review)
                        }
                    }
                    Review::Save {
                        close,
                        complete,
                        signature,
                        ..
                    } => {
                        if self.save_task_editor_checked(*close, Some(signature)) {
                            if *complete {
                                self.action(Command::TaskComplete { task: self.task });
                            }
                        } else {
                            self.task_review = Some(review)
                        }
                    }
                },
                None => self.task_review = Some(review),
            }
        }
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            self.request_business_close(windows::Kind::Task);
        }
    }
    pub(super) fn save_task_editor(&mut self, close_after: bool) -> bool {
        self.save_task_editor_checked(close_after, None)
    }
    fn save_task_editor_checked(&mut self, close_after: bool, expected: Option<&str>) -> bool {
        let result = (|| -> Result<()> {
            let locked:bool=self.db.query_row("SELECT cancelled=1 OR EXISTS(SELECT 1 FROM invoice_tasks WHERE task=tasks.id) FROM tasks WHERE id=?",[self.task],|r|r.get(0))?;
            anyhow::ensure!(
                !locked,
                "任务已开票或取消，无法保存修改。当前输入已保留，请核对关联单据。"
            );
            if self.item > 0 {
                self.task_tax_edits.insert(self.item, self.item_tax_code);
                self.task_item_edits.insert(
                    self.item,
                    (
                        self.item_price.clone(),
                        self.item_cost.clone(),
                        self.quantity,
                    ),
                );
            } else {
                anyhow::ensure!(
                    self.item_price.trim().is_empty()
                        && self.item_cost.trim().is_empty()
                        && self.quantity == 1,
                    "请先选择需要修改的业务明细"
                );
            }
            let currency: String =
                self.db
                    .query_row("SELECT currency FROM tasks WHERE id=?", [self.task], |r| {
                        r.get(0)
                    })?;
            for id in self.chosen.clone() {
                let source: String =
                    self.db
                        .query_row("SELECT currency FROM prices WHERE id=?", [id], |r| r.get(0))?;
                if self.missing_fx(&source, &currency) {
                    anyhow::bail!(self.message.clone());
                }
            }
            let has_expense = !self.expense.trim().is_empty()
                || self.expense_vendor > 0
                || self.task_expense_tax != Default::default()
                || !crate::parse_money(&self.expense_amount).is_ok_and(|value| value == 0);
            if has_expense && self.task_expense_tax.code > 0 && self.missing_fx(&currency, "CAD") {
                anyhow::bail!(self.message.clone());
            }
            let candidate = if expected.is_some() {
                Some(self.task_expense_document()?)
            } else {
                None
            };
            let tx = self
                .db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let (Some(signature), Some(doc)) = (expected, candidate.as_ref()) {
                anyhow::ensure!(
                    crate::expense_doc::review(&tx, doc)?.signature == signature,
                    "Expense inputs changed. Review the task expense again."
                );
            }
            crate::editable(&tx, self.task)?;
            for (id, (price, cost, quantity)) in &self.task_item_edits {
                crate::update_item(
                    &tx,
                    self.task,
                    *id,
                    Some(crate::parse_money(price)?),
                    Some(crate::parse_money(cost)?),
                    Some(*quantity),
                )?;
            }
            for (id, code) in &self.task_tax_edits {
                tx.execute(
                    "UPDATE items SET tax_code=? WHERE id=? AND task=?",
                    rusqlite::params![code, id, self.task],
                )?;
            }
            for id in &self.chosen {
                crate::add_item(&tx, self.task, *id, 1, None, None)?;
            }
            if has_expense {
                let draft = crate::accounting::Expense {
                    tax: self.task_expense_tax.clone(),
                    id: 0,
                    date: crate::today().to_string(),
                    description: self.expense.clone(),
                    account: crate::accounting::default_account(&tx)?,
                    vendor: Some(self.expense_vendor),
                    currency,
                    amount: self.expense_amount.clone(),
                    task: Some(self.task),
                };
                let id = crate::accounting::save_expense_on(&tx, &draft)?;
                crate::expense_doc::register_single(&tx, id)?;
            }
            tx.commit()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.task_item_edits.clear();
                self.task_tax_edits.clear();
                self.item_tax_code = 0;
                self.item = 0;
                self.item_price.clear();
                self.item_cost.clear();
                self.quantity = 1;
                self.chosen.clear();
                self.expense.clear();
                self.expense_amount = "0.00".into();
                self.expense_vendor = 0;
                self.task_expense_tax = Default::default();
                self.message = "Task 已保存".into();
                self.error = false;
                if close_after {
                    self.task_editor_open = false;
                }
                true
            }
            Err(error) => {
                self.message = format!("保存失败：{error:#}");
                self.error = true;
                false
            }
        }
    }
    pub(super) fn task_details(&mut self, ui: &mut egui::Ui) {
        let data = match crate::snapshot(&self.db, self.task) {
            Ok(v) => v,
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
                return;
            }
        };
        ui.label(format!(
            "任务日期：{}",
            data["task_date"].as_str().unwrap_or("未设置")
        ));
        let code = data["currency"].as_str().unwrap_or("UNSET");
        ui.add_space(12.0);
        ui.heading(format!(
            "任务 {} · {}",
            data["number"].as_str().unwrap_or(""),
            data["title"].as_str().unwrap_or("")
        ));
        ui.horizontal(|ui| {
            for (key, label) in [("total", "销售额"), ("cost", "总成本"), ("profit", "利润")]
            {
                ui.group(|ui| {
                    ui.label(label);
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            code,
                            crate::format_money(data[key].as_i64().unwrap_or(0))
                        ))
                        .strong(),
                    );
                });
            }
        });
        let invoice: Option<i64> = match self
            .db
            .query_row(
                "SELECT invoice FROM invoice_tasks WHERE task=?",
                [self.task],
                |r| r.get(0),
            )
            .optional()
        {
            Ok(id) => id,
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
                return;
            }
        };
        let cancelled = data["status"] == "cancel";
        let locked = invoice.is_some() || cancelled;
        if !locked && self.item > 0 {
            self.task_tax_edits.insert(self.item, self.item_tax_code);
            self.task_item_edits.insert(
                self.item,
                (
                    self.item_price.clone(),
                    self.item_cost.clone(),
                    self.quantity,
                ),
            );
        }
        let items: Vec<Vec<String>> = data["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                vec![
                    i["id"].to_string(),
                    i["description"].as_str().unwrap_or("").into(),
                    i["quantity"].to_string(),
                    crate::format_money(i["unit_cost"].as_i64().unwrap_or(0)),
                    crate::format_money(i["unit_price"].as_i64().unwrap_or(0)),
                    crate::format_money(i["subtotal"].as_i64().unwrap_or(0)),
                    i["source_currency"].as_str().unwrap_or("").into(),
                    i["fx_rate"].as_str().unwrap_or("").into(),
                ]
            })
            .collect();
        if table(
            ui,
            "items",
            &[
                "明细 ID",
                "业务",
                "数量",
                "单位成本",
                "单价",
                "小计",
                "原币种",
                "汇率",
            ],
            &items,
            &mut self.item,
        ) {
            if let Some(r) = items.iter().find(|r| r[0] == self.item.to_string()) {
                self.item_tax_code =
                    self.task_tax_edits
                        .get(&self.item)
                        .copied()
                        .unwrap_or_else(|| {
                            self.db
                                .query_row(
                                    "SELECT tax_code FROM items WHERE id=?",
                                    [self.item],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0)
                        });
                if let Some((price, cost, quantity)) = self.task_item_edits.get(&self.item) {
                    self.item_price = price.clone();
                    self.item_cost = cost.clone();
                    self.quantity = *quantity;
                } else {
                    self.quantity = r[2].parse().unwrap_or(1);
                    self.item_cost = r[3].clone();
                    self.item_price = r[4].clone();
                }
            }
        }
        if locked {
            ui.colored_label(
                Color32::from_rgb(174, 100, 20),
                if cancelled {
                    "任务已 cancel，业务与价格已锁定。"
                } else {
                    "已开票，业务与价格已锁定。"
                },
            );
        }
        ui.add_enabled_ui(!locked, |ui| {
            ui.horizontal(|ui| {
                ui.label("数量");
                ui.add(egui::DragValue::new(&mut self.quantity).range(1..=1_000_000));
            });
            field(ui, "本任务单价", &mut self.item_price);
            field(ui, "本任务单位成本", &mut self.item_cost);
            ui.horizontal(|ui| {
                ui.label("本业务税码");
                tax_ui::code_picker(
                    &self.db,
                    ui,
                    ("edit_task_tax", self.task, self.item),
                    &mut self.item_tax_code,
                );
            });
            ui.small("选择明细后修改，点击底部 Save 保存；可切换明细继续修改。");
            ui.collapsing("从 Price List 添加更多业务", |ui| {
                self.business_choices(ui);
                ui.small("勾选后点击底部 Save 添加到任务。");
            });
            ui.collapsing("额外费用", |ui| {
                let previous_vendor = self.expense_vendor;
                field(ui, "说明", &mut self.expense);
                field(ui, "金额（任务币种）", &mut self.expense_amount);
                let vendors = self.rows("SELECT id,name FROM vendors ORDER BY id");
                ui.horizontal(|ui| {
                    ui.label("Vendor * 供应商");
                    picker(
                        ui,
                        &format!("task_expense_vendor_{}", self.task),
                        &mut self.expense_vendor,
                        &vendors,
                    );
                });
                if previous_vendor != self.expense_vendor && self.task_expense_tax.code == 0 {
                    self.task_expense_tax.code = self
                        .db
                        .query_row(
                            "SELECT code FROM tax_assignments WHERE entity='vendor' AND record=?",
                            [self.expense_vendor],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("税码");
                    tax_ui::code_picker(
                        &self.db,
                        ui,
                        ("task_expense_tax", self.task),
                        &mut self.task_expense_tax.code,
                    );
                    ui.checkbox(&mut self.task_expense_tax.inclusive, "含税录入");
                    ui.label("可抵扣 %");
                    ui.add(
                        egui::DragValue::new(&mut self.task_expense_tax.recovery).range(0..=100),
                    );
                });
                ui.small("填写后点击底部 Save 记录费用。");
            });
        });
        if let Some(expenses) = data["expenses"].as_array() {
            for e in expenses {
                ui.label(format!(
                    "费用 · {}：{} {}",
                    e["description"].as_str().unwrap_or(""),
                    code,
                    crate::format_money(e["amount"].as_i64().unwrap_or(0))
                ));
            }
        }
        ui.separator();
        ui.label(format!(
            "状态：{}",
            data["status"].as_str().unwrap_or("pending")
        ));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !locked && data["status"] != "completed",
                    egui::Button::new("Mark Complete"),
                )
                .clicked()
            {
                self.request_task_save(false, true);
            }
            if ui
                .add_enabled(!locked, egui::Button::new("Cancel Task"))
                .clicked()
            {
                self.task_review = Some(Review::Cancel(self.task));
                self.task_review_focus = true;
            }
        });
        if let Some(id) = invoice {
            ui.separator();
            let number: String = self
                .db
                .query_row("SELECT number FROM invoices WHERE id=?", [id], |r| r.get(0))
                .unwrap_or_else(|_| id.to_string());
            ui.heading(format!("Invoice {number}"));
            if let Ok(balance) = crate::billing::balance(&self.db, id) {
                theme::badge(ui, balance.status());
                ui.label(format!(
                    "未付余额：{} {}",
                    balance.currency,
                    crate::format_money(balance.remaining)
                ));
            }
            if ui.button("Open Invoice / Receipts").clicked() {
                self.task_editor_open = false;
                self.open_invoice(id);
            }
            if ui
                .add_enabled(
                    self.print_receiver.is_none(),
                    egui::Button::new("Print Invoice"),
                )
                .clicked()
            {
                self.print_invoice(ui.ctx(), id);
            }
        } else {
            let completed = self
                .db
                .query_row(
                    "SELECT progress=100 AND cancelled=0 FROM tasks WHERE id=?",
                    [self.task],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false);
            if ui
                .add_enabled(completed, egui::Button::new("Create Invoice from Task"))
                .on_disabled_hover_text("仅 completed 状态可开票，请先点击 Completed 标记完成")
                .clicked()
                && self.save_task_editor(false)
            {
                self.task_editor_open = false;
                self.open_invoice_workbench();
                self.invoice_workbench.select_task(self.task);
            }
        }
    }
    pub(super) fn business_choices(&mut self, ui: &mut egui::Ui) {
        let rows=self.rows("SELECT p.id,p.name,p.currency,printf('%d.%02d',p.price/100,p.price%100),v.name FROM prices p JOIN vendors v ON v.id=p.vendor ORDER BY p.id");
        ui.label("勾选业务（每项数量默认为 1，可在任务明细中调整）");
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .id_salt(ui.id().with("choices"))
            .show(ui, |ui| {
                for r in rows {
                    let id = r[0].parse().unwrap_or(0);
                    let choices = &mut self.chosen;
                    let mut checked = choices.contains(&id);
                    if ui
                        .checkbox(
                            &mut checked,
                            format!("#{} {} · {} {} · {}", r[0], r[1], r[2], r[3], r[4]),
                        )
                        .changed()
                    {
                        if checked {
                            choices.push(id)
                        } else {
                            choices.retain(|v| *v != id)
                        }
                    }
                }
            });
    }
}
