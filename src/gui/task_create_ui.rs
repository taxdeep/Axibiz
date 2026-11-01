use super::*;
impl Desktop {
    pub(super) fn open_new_task(&mut self) {
        if self.reuse_business(windows::Kind::Task) {
            return;
        }
        self.task_form_epoch = self.task_form_epoch.wrapping_add(1);
        self.task_title.clear();
        self.customer = 0;
        self.task_date = crate::today();
        self.task_currency = Settings::load(&self.db)
            .map(|s| s.currency)
            .unwrap_or_else(|_| self.settings.currency.clone());
        self.new_task_lines = vec![TaskLine::default()];
        self.lookup_row = None;
        self.lookup_search.clear();
        self.new_item_row = None;
        self.new_task_error.clear();
        self.new_task_open = true;
    }
    pub(super) fn save_new_task(&mut self) {
        let prices: Vec<_> = self
            .new_task_lines
            .iter()
            .filter(|l| l.price_id != 0)
            .map(|l| l.price_id)
            .collect();
        if !self.prepare_price_fx(&prices, &self.task_currency.clone()) {
            self.new_task_error = self.message.clone();
            return;
        }

        let result = (|| -> Result<i64> {
            let tx = self.db.transaction()?;
            let id = crate::add_task_on(
                &tx,
                &self.task_title,
                self.customer,
                &self.task_currency,
                &self.task_date.to_string(),
            )?;
            for (index, line) in self.new_task_lines.iter().enumerate() {
                if line.price_id == 0 {
                    anyhow::ensure!(
                        !line.input_pending,
                        "第 {} 行尚未选定业务，请从候选列表选择",
                        index + 1
                    );
                    continue;
                }
                let price = line
                    .price_override
                    .as_deref()
                    .map(crate::parse_money)
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("第 {} 行单价无效：{e}", index + 1))?;
                let item = crate::add_item(&tx, id, line.price_id, line.quantity, price, None)
                    .map_err(|e| anyhow::anyhow!("第 {} 行：{e:#}", index + 1))?;
                if let Some(code) = line.tax_code {
                    tx.execute(
                        "UPDATE items SET tax_code=? WHERE id=?",
                        rusqlite::params![code, item],
                    )?;
                }
            }
            tx.commit()?;
            Ok(id)
        })();
        match result {
            Ok(id) => {
                self.task = id;
                self.item = 0;
                self.page = 0;
                self.new_task_open = false;
                self.task_editor_open = false;
                let number = self
                    .db
                    .query_row("SELECT number FROM tasks WHERE id=?", [id], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap_or_else(|_| id.to_string());
                self.message = format!("Task {number} 已创建");
                self.error = false;
                self.new_task_lines.clear();
                self.lookup_row = None;
                self.new_task_error.clear();
            }
            Err(e) => {
                self.new_task_error = format!("{e:#}");
            }
        }
    }
    pub(super) fn catalog(&self) -> Result<Vec<CatalogItem>> {
        let mut stmt=self.db.prepare("SELECT p.id,p.name,p.unit,p.currency,p.price,v.name FROM prices p JOIN vendors v ON v.id=p.vendor WHERE v.active=1 ORDER BY p.id")?;
        let rows = stmt.query_map([], |r| {
            Ok(CatalogItem {
                id: r.get(0)?,
                name: r.get(1)?,
                unit: r.get(2)?,
                currency: r.get(3)?,
                price: r.get(4)?,
                vendor: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub(super) fn assign_task_line(&mut self, row: usize, id: i64) {
        if let Some(line) = self.new_task_lines.get_mut(row) {
            line.tax_code = None;
            line.price_id = id;
            line.price_override = None;
            line.input_pending = false;
        }
    }
    pub(super) fn line_price(
        &self,
        line: &TaskLine,
        catalog: &[CatalogItem],
    ) -> Result<Option<i64>> {
        if let Some(value) = &line.price_override {
            return Ok(Some(crate::parse_money(value)?));
        }
        let item = catalog
            .iter()
            .find(|p| p.id == line.price_id)
            .ok_or_else(|| anyhow::anyhow!("业务不存在"))?;
        if item.currency == self.task_currency {
            return Ok(Some(item.price));
        }
        crate::fx::cached(&self.db, &item.currency, &self.task_currency)?
            .map(|rate| crate::convert(item.price, rate))
            .transpose()
    }
    pub(super) fn draft_total(&self, catalog: &[CatalogItem]) -> Result<(i64, usize)> {
        let mut total = 0i64;
        let mut pending = 0;
        anyhow::ensure!(
            !self.new_task_lines.iter().any(|l| l.input_pending),
            "请从候选列表选定输入中的业务"
        );
        for line in self.new_task_lines.iter().filter(|l| l.price_id != 0) {
            anyhow::ensure!(line.quantity > 0, "数量必须大于零");
            match self.line_price(line, catalog)? {
                Some(price) => {
                    total = total
                        .checked_add(
                            price
                                .checked_mul(line.quantity)
                                .ok_or_else(|| anyhow::anyhow!("金额过大"))?,
                        )
                        .ok_or_else(|| anyhow::anyhow!("合计过大"))?;
                }
                None => pending += 1,
            }
        }
        Ok((total, pending))
    }
    pub(super) fn open_item_lookup(&mut self, row: usize) {
        self.lookup_row = Some(row);
        self.lookup_search.clear();
        self.lookup_selected = self.new_task_lines.get(row).map_or(0, |l| l.price_id);
    }
    pub(super) fn create_item_for_line(&mut self, row: usize) {
        if self.new_record.is_some() {
            self.message = "已有资料录入窗口，请先完成或关闭该窗口。".into();
            return;
        }
        self.record_form_epoch = self.record_form_epoch.wrapping_add(1);
        self.new_item_row = Some(row);
        self.new_record = Some(NewRecord {
            page: 3,
            name: String::new(),
            vendor: 0,
            unit: "项".into(),
            cost: "0.00".into(),
            price: "0.00".into(),
            currency: self.task_currency.clone(),
            error: String::new(),
            ..Default::default()
        });
    }
    pub(super) fn task_line_grid(&mut self, ui: &mut egui::Ui, catalog: &[CatalogItem]) {
        let mut remove = None;
        let mut lookup = None;
        let mut create = None;
        egui::ScrollArea::horizontal()
            .id_salt("task_entry_horizontal")
            .show(ui, |ui| {
                egui::Grid::new("task_entry_grid")
                    .striped(true)
                    .min_row_height(34.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for title in [
                            "行",
                            "业务编号 / 查找",
                            "业务说明",
                            "数量",
                            "单位",
                            "单价（结算币种）",
                            "金额（未税）",
                            "税码",
                            "",
                        ] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for index in 0..self.new_task_lines.len() {
                            ui.label((index + 1).to_string());
                            let mut selected = self.new_task_lines[index].price_id;
                            ui.horizontal(|ui| {
                                let choices: Vec<_> = catalog
                                    .iter()
                                    .map(|item| crate::autocomplete::Choice {
                                        id: item.id,
                                        name: item.name.clone(),
                                        detail: format!(
                                            "#{} · {} {} · {}",
                                            item.id,
                                            item.currency,
                                            crate::format_money(item.price),
                                            item.vendor
                                        ),
                                    })
                                    .collect();
                                let outcome = crate::autocomplete::show(
                                    ui,
                                    ui.make_persistent_id((
                                        "task_line_item",
                                        self.task_form_epoch,
                                        index,
                                    )),
                                    &mut selected,
                                    &choices,
                                    200.0,
                                    true,
                                );
                                self.new_task_lines[index].input_pending = outcome.pending;
                                if let Some(name) = outcome.create {
                                    create = Some((index, name));
                                }
                                if icons::button(ui,icons::Icon::Search,"Find service").clicked() {
                                    lookup = Some(index);
                                }
                            });
                            if selected != self.new_task_lines[index].price_id {
                                let pending = self.new_task_lines[index].input_pending;
                                self.assign_task_line(index, selected);
                                self.new_task_lines[index].input_pending = pending;
                            }
                            let item = catalog.iter().find(|p| p.id == selected);
                            let description = item.map(|i| i.name.as_str()).unwrap_or("请选择业务");
                            ui.add(egui::Label::new(description).truncate())
                                .on_hover_text(description);
                            ui.add(
                                egui::DragValue::new(&mut self.new_task_lines[index].quantity)
                                    .range(1..=1_000_000)
                                    .speed(1),
                            );
                            ui.label(item.map(|i| i.unit.as_str()).unwrap_or("—"));
                            let auto_price = self
                                .line_price(&self.new_task_lines[index], catalog)
                                .ok()
                                .flatten();
                            let mut text = self.new_task_lines[index]
                                .price_override
                                .clone()
                                .unwrap_or_else(|| auto_price.map(crate::format_money).unwrap_or_default());
                            let response = ui.add_enabled(
                                selected != 0,
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(100.0)
                                    .hint_text("待换算"),
                            );
                            if response.changed() {
                                self.new_task_lines[index].price_override = Some(text);
                            }
                            let price = self.line_price(&self.new_task_lines[index], catalog);
                            let amount = match price {
                                Ok(Some(p)) => p
                                    .checked_mul(self.new_task_lines[index].quantity)
                                    .map(crate::format_money)
                                    .unwrap_or("金额过大".into()),
                                Ok(None) if selected != 0 => "待换算".into(),
                                Err(_) if selected != 0 => "单价无效".into(),
                                _ => "—".into(),
                            };
                            ui.label(amount);
                            let mut code=self.new_task_lines[index].tax_code.unwrap_or_else(||self.db.query_row("SELECT COALESCE(NULLIF((SELECT tax_code FROM prices WHERE id=?),0),(SELECT code FROM tax_assignments WHERE entity='customer' AND record=?),0)",rusqlite::params![selected,self.customer],|r|r.get(0)).unwrap_or(0));
                            if tax_ui::code_picker(&self.db,ui,("new_task_tax",self.new_task_lines[index].key),&mut code) {self.new_task_lines[index].tax_code=Some(code);}
                            if theme::trash_button(ui,"Delete line").clicked() {
                                remove = Some(index);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(index) = remove {
            self.new_task_lines.remove(index);
        } else if let Some(index) = lookup {
            self.open_item_lookup(index);
        } else if let Some((index, name)) = create {
            self.create_item_for_line(index);
            if let Some(draft) = &mut self.new_record {
                draft.name = name;
            }
        }
        if self.new_task_lines.is_empty() {
            self.new_task_lines.push(TaskLine::default());
        }
        ui.horizontal(|ui| {
            if ui.button("+ Add Line").clicked() {
                self.new_task_lines.push(TaskLine::default());
            }
            ui.small("选择业务后带入价格，可直接修改数量和单价；不影响 Price List。");
        });
    }
    pub(super) fn item_lookup_dialog(&mut self, ctx: &egui::Context) {
        let Some(row) = self.lookup_row else {
            return;
        };
        if !self.new_task_open {
            self.lookup_row = None;
            return;
        }
        let catalog = match self.catalog() {
            Ok(items) => items,
            Err(e) => {
                self.new_task_error = e.to_string();
                self.lookup_row = None;
                return;
            }
        };
        let mut cancel = false;
        let mut select = false;
        let mut create = false;
        let response = egui::Modal::new(egui::Id::new("task_item_lookup")).show(ctx, |ui| {
            ui.set_width(760.0_f32.min((ctx.content_rect().width() - 64.0).max(300.0)));
            ui.heading("Select Service");
            ui.label(format!("为第 {} 行选择 Price List 中的业务", row + 1));
            field(ui, "搜索编号 / 业务名称 / 供应商", &mut self.lookup_search);
            ui.separator();
            let visible: Vec<_> = catalog
                .iter()
                .filter(|p| p.matches(&self.lookup_search))
                .collect();
            egui::ScrollArea::both()
                .id_salt("lookup_results")
                .max_height(320.0)
                .min_scrolled_height(160.0)
                .show(ui, |ui| {
                    egui::Grid::new("lookup_items")
                        .striped(true)
                        .spacing([20.0, 10.0])
                        .show(ui, |ui| {
                            for label in ["业务编号", "业务说明", "供应商", "单位", "币种", "价格"]
                            {
                                ui.strong(label);
                            }
                            ui.end_row();
                            for item in &visible {
                                let response = ui.selectable_label(
                                    self.lookup_selected == item.id,
                                    format!("#{}", item.id),
                                );
                                if response.clicked() {
                                    self.lookup_selected = item.id;
                                }
                                if response.double_clicked() {
                                    self.lookup_selected = item.id;
                                    select = true;
                                }
                                if ui
                                    .selectable_label(self.lookup_selected == item.id, &item.name)
                                    .clicked()
                                {
                                    self.lookup_selected = item.id;
                                }
                                ui.label(&item.vendor);
                                ui.label(&item.unit);
                                ui.label(&item.currency);
                                ui.label(crate::format_money(item.price));
                                ui.end_row();
                            }
                        });
                    if visible.is_empty() {
                        ui.label("没有匹配的业务，可以修改搜索条件或新建业务。");
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("+ New Service").clicked() {
                    create = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    let valid = visible.iter().any(|p| p.id == self.lookup_selected);
                    if ui
                        .add_enabled(valid, egui::Button::new("Select Service"))
                        .clicked()
                    {
                        select = true;
                    }
                });
            });
        });
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if create {
            self.create_item_for_line(row);
        } else if select {
            self.assign_task_line(row, self.lookup_selected);
            self.lookup_row = None;
        } else if cancel || escape {
            self.lookup_row = None;
        }
    }
    pub(super) fn new_task_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_task_open {
            return;
        }
        let catalog = match self.catalog() {
            Ok(v) => v,
            Err(e) => {
                self.new_task_error = e.to_string();
                Vec::new()
            }
        };
        let mut cancel = false;
        let child_open = self.lookup_row.is_some() || self.new_record.is_some();
        let response = windows::surface(ctx, egui::Id::new("new_task_modal"), |ui| {
            ui.set_width(1060.0_f32.min((ctx.content_rect().width() - 48.0).max(300.0)));
            ui.horizontal(|ui| {
                ui.heading("New Task");
            });
            ui.label("填写客户和日期，逐行录入本次任务的业务明细。");
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("new_task_form")
                .max_height((ctx.content_rect().height() - 220.0).max(200.0))
                .show(ui, |ui| {
                    field(ui, "任务名称 *", &mut self.task_title);
                    let customers = self.rows("SELECT id,name FROM customers WHERE active=1 ORDER BY id");
                    ui.horizontal_wrapped(|ui| {
                        ui.label("客户 *");
                        picker(
                            ui,
                            &format!("new_customer_{}", self.task_form_epoch),
                            &mut self.customer,
                            &customers,
                        );
                        ui.separator();
                        ui.label("任务日期");
                        ui.add(
                            egui_extras::DatePickerButton::new(&mut self.task_date)
                                .id_salt("new_task_date")
                                .format("%Y-%m-%d"),
                        );
                        if ui.button("Today").clicked() {
                            self.task_date = crate::today();
                        }
                        ui.separator();
                        ui.label("结算币种");
                        let previous = self.task_currency.clone();
                        currency_picker(ui, "new_task_currency", &mut self.task_currency);
                        if self.task_currency != previous {
                            for line in &mut self.new_task_lines {
                                line.price_override = None;
                            }
                        }
                    });
                    if customers.is_empty() {
                        ui.colored_label(
                            Color32::from_rgb(174, 100, 20),
                            "暂无客户，请先到 Customer 页面添加客户。",
                        );
                    }
                    ui.small("Task 单价和合计均为未税金额；开票时按税码及开票日期计算税额。更改结算币种会重新换算单价。");
                    ui.add_space(14.0);
                    ui.strong("业务明细");
                    self.task_line_grid(ui, &catalog);
                    ui.add_space(16.0);
                    match self.draft_total(&catalog) {
                        Ok((total, pending)) => {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.strong(format!(
                                        "Total: {} {}",
                                        self.task_currency,
                                        crate::format_money(total)
                                    ));
                                },
                            );
                            if pending > 0 {
                                ui.colored_label(
                                    Color32::from_rgb(174, 100, 20),
                                    format!(
                                        "{} 行缺少本地汇率，未计入当前合计；保存时查询并换算。",
                                        pending
                                    ),
                                );
                            }
                        }
                        Err(e) => {
                            ui.colored_label(
                                Color32::from_rgb(190, 45, 45),
                                format!("合计无法计算：{e}"),
                            );
                        }
                    }
                });
            if !self.new_task_error.is_empty() {
                ui.colored_label(Color32::from_rgb(190, 45, 45), &self.new_task_error);
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = !self.task_title.trim().is_empty() && self.customer > 0;
                if controls::primary(ui, "保存 Task", ready).clicked() {
                    self.save_new_task();
                }
                if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        let escape = !child_open
            && !self.lookup_row.is_some()
            && self.new_record.is_none()
            && response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if cancel || escape {
            self.request_business_close(windows::Kind::Task);
        }
    }
}
