use super::*;
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Clone)]
struct Candidate {
    id: i64,
    number: String,
    title: String,
    date: String,
    customer: i64,
    customer_name: String,
    currency: String,
    vendors: Vec<String>,
    net: i64,
    tax: i64,
    total: i64,
    issue: String,
}
#[derive(Default)]
pub(super) struct Workbench {
    pub open: bool,
    rows: Vec<Candidate>,
    selected: BTreeSet<i64>,
    customer: i64,
    vendor: String,
    search: String,
    preview: Option<Value>,
    error: String,
    notice: String,
}
fn candidates(c: &Connection) -> Result<Vec<Candidate>> {
    let mut stmt=c.prepare("SELECT t.id,COALESCE(t.number,''),t.title,COALESCE(t.task_date,'未设置'),t.customer_id,c.name,t.currency FROM tasks t JOIN customers c ON c.id=t.customer_id WHERE t.progress=100 AND t.cancelled=0 AND NOT EXISTS(SELECT 1 FROM invoice_tasks WHERE task=t.id) ORDER BY t.id")?;
    let mut rows = stmt
        .query_map([], |r| {
            Ok(Candidate {
                id: r.get(0)?,
                number: r.get(1)?,
                title: r.get(2)?,
                date: r.get(3)?,
                customer: r.get(4)?,
                customer_name: r.get(5)?,
                currency: r.get(6)?,
                vendors: Vec::new(),
                net: 0,
                tax: 0,
                total: 0,
                issue: String::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for row in &mut rows {
        let data = match crate::snapshot(c, row.id) {
            Ok(data) => data,
            Err(e) => {
                row.issue = format!("{e:#}");
                row.vendors = vec!["未记录 Vendor".into()];
                continue;
            }
        };
        row.vendors = data["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                item["vendor_id"]
                    .as_i64()
                    .and_then(|id| {
                        c.query_row("SELECT name FROM vendors WHERE id=?", [id], |r| {
                            r.get::<_, String>(0)
                        })
                        .ok()
                    })
                    .unwrap_or_else(|| {
                        item["vendor_name"]
                            .as_str()
                            .unwrap_or("未记录 Vendor（旧业务）")
                            .to_owned()
                    })
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if row.vendors.is_empty() {
            row.vendors.push("未记录 Vendor".into());
        }
        match crate::billing::preview(c, &[row.id]) {
            Ok(data) => {
                row.net = data["net_total"].as_i64().unwrap();
                row.tax = data["tax_total"].as_i64().unwrap();
                row.total = data["total"].as_i64().unwrap();
            }
            Err(e) => row.issue = format!("{e:#}"),
        }
    }
    rows.retain(|row| row.issue.is_empty());
    rows.sort_by(|a, b| {
        a.customer_name
            .to_lowercase()
            .cmp(&b.customer_name.to_lowercase())
            .then(a.customer.cmp(&b.customer))
            .then(a.currency.cmp(&b.currency))
            .then(a.date.cmp(&b.date))
            .then(a.id.cmp(&b.id))
    });
    Ok(rows)
}
impl Workbench {
    pub(super) fn select_task(&mut self, task: i64) {
        self.selected.clear();
        if self.rows.iter().any(|r| r.id == task && r.issue.is_empty()) {
            self.selected.insert(task);
        }
    }
    fn scope(&self) -> Option<(i64, &str)> {
        self.rows
            .iter()
            .find(|row| self.selected.contains(&row.id))
            .map(|row| (row.customer, row.currency.as_str()))
    }
    fn visible(&self, row: &Candidate) -> bool {
        (self.customer == 0 || row.customer == self.customer)
            && (self.vendor.is_empty() || row.vendors.contains(&self.vendor))
            && (self.search.trim().is_empty()
                || format!(
                    "{} {} {} {}",
                    row.number,
                    row.title,
                    row.customer_name,
                    row.vendors.join(" ")
                )
                .to_lowercase()
                .contains(&self.search.trim().to_lowercase()))
    }
    fn compatible(&self, row: &Candidate) -> bool {
        self.scope()
            .is_none_or(|(customer, currency)| row.customer == customer && row.currency == currency)
    }
}
impl Desktop {
    pub(super) fn open_invoice_workbench(&mut self) {
        if self.reuse_business(windows::Kind::Invoice) {
            return;
        }
        let mut work = Workbench {
            open: true,
            ..Default::default()
        };
        match candidates(&self.db) {
            Ok(rows) => work.rows = rows,
            Err(e) => work.error = format!("{e:#}"),
        }
        let preselected: Vec<_> = work
            .rows
            .iter()
            .filter(|r| r.issue.is_empty() && self.checked_tasks.contains(&r.id))
            .collect();
        if preselected.first().is_some_and(|first| {
            preselected
                .iter()
                .any(|row| row.customer != first.customer || row.currency != first.currency)
        }) {
            work.notice =
                "原勾选跨客户或币种，请在此重新选择；一张发票只能属于一个客户和币种。".into();
        } else {
            work.selected = preselected.iter().map(|r| r.id).collect();
            if work.selected.len() < self.checked_tasks.len() {
                work.notice = "部分原勾选任务尚不符合开票条件，未预选；主列表勾选保持不变。".into();
            }
        }
        self.invoice_workbench = work;
    }
    fn refresh_workbench(&mut self, work: &mut Workbench) {
        match candidates(&self.db) {
            Ok(rows) => {
                let old = work.selected.len();
                work.selected
                    .retain(|id| rows.iter().any(|r| r.id == *id && r.issue.is_empty()));
                if old != work.selected.len() {
                    work.notice = "已移除不再符合条件的勾选，请核对后重新预览。".into();
                }
                work.rows = rows;
                work.preview = None;
                work.error.clear();
            }
            Err(e) => work.error = format!("{e:#}"),
        }
    }
    fn preview_workbench(&self, work: &mut Workbench) {
        let tasks: Vec<_> = work.selected.iter().copied().collect();
        match crate::billing::preview(&self.db, &tasks) {
            Ok(data) => {
                work.preview = Some(data);
                work.error.clear();
            }
            Err(e) => {
                work.preview = None;
                work.error = format!("无法预览：{e:#}。请刷新列表或检查税务设置、汇率。");
            }
        }
    }
    fn confirm_workbench(&mut self, work: &mut Workbench) -> bool {
        let Some(expected) = &work.preview else {
            work.error = "请先预览发票。".into();
            return false;
        };
        let tasks: Vec<_> = work.selected.iter().copied().collect();
        match crate::billing::merge_confirmed(&mut self.db, &tasks, expected) {
            Ok(id) => {
                for task in &tasks {
                    self.checked_tasks.remove(task);
                }
                work.open = false;
                work.preview = None;
                work.selected.clear();
                self.invoice_results.clear();
                self.message = format!("已将 {} 个 Task 合并开票", tasks.len());
                self.error = false;
                self.open_invoice(id);
                true
            }
            Err(e) => {
                work.error = format!("未生成发票：{e:#}。请重新预览。");
                work.preview = None;
                false
            }
        }
    }
    pub(super) fn invoice_workbench_dialog(&mut self, ctx: &egui::Context) {
        if !self.invoice_workbench.open {
            return;
        }
        let mut work = std::mem::take(&mut self.invoice_workbench);
        let response = windows::surface(ctx, egui::Id::new("invoice_workbench"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 48.0).max(300.0));
            ui.heading(if work.preview.is_some() {
                "Review Invoice · 确认开票预览"
            } else {
                "Create Invoice · 选择可开票任务"
            });
            ui.small("先选任务，再核对客户、税额和总额。一张发票仅合并同客户、同币种的任务；预览不会分配编号或修改状态。");
            ui.separator();
            egui::ScrollArea::vertical().id_salt("invoice_workbench_body").max_height((ctx.content_rect().height()-235.0).max(180.0)).show(ui,|ui| {
                if let Some(data)=&work.preview {
                    self.show_invoice_checks(ui,data);
                    invoice_sheet::band(ui, data, "Preview");
                    invoice_sheet::snapshot_lines(ui, "workbench_preview_items", data, (ctx.content_rect().height() - 480.0).clamp(180.0, 620.0));
                    invoice_sheet::totals(ui, data["currency"].as_str().unwrap_or(""), data["net_total"].as_i64().unwrap_or(0), data["tax_total"].as_i64().unwrap_or(0), data["total"].as_i64().unwrap_or(0), None);
                    ui.collapsing("税额明细 / Tax breakdown",|ui| {
                        for (index,item) in data["items"].as_array().unwrap().iter().enumerate() {
                            for part in item["tax_snapshot"]["parts"].as_array().unwrap() {
                                ui.label(format!("第 {} 行 · {} · {}% · {} {}",index+1,part["name"].as_str().unwrap_or(""),part["rate"].as_i64().unwrap_or(0) as f64/10000.0,data["currency"].as_str().unwrap_or(""),crate::format_money(part["amount"].as_i64().unwrap_or(0))));
                            }
                        }
                        ui.small("NONE 表示未指定税码；零税率与免税请核对对应税码。税额按开票日期及已配置税率计算。");
                    });
                    ui.small("确认后分配 Invoice 编号并锁定所有来源 Task，之后可在 Invoice 页面登记收款。");
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut work.search).desired_width(230.0).hint_text("查找 Task、客户或 Vendor"));
                        egui::ComboBox::from_id_salt("workbench_customer").selected_text(if work.customer==0{"全部客户".to_owned()}else{work.rows.iter().find(|r|r.customer==work.customer).map(|r|r.customer_name.clone()).unwrap_or_default()}).show_ui(ui,|ui|{ui.selectable_value(&mut work.customer,0,"全部客户");let customers:BTreeSet<_>=work.rows.iter().map(|r|(r.customer,r.customer_name.clone())).collect();for (id,name) in customers{ui.selectable_value(&mut work.customer,id,name);}});
                        egui::ComboBox::from_id_salt("workbench_vendor").selected_text(if work.vendor.is_empty(){"全部 Vendor"}else{&work.vendor}).show_ui(ui,|ui|{ui.selectable_value(&mut work.vendor,String::new(),"全部 Vendor");let vendors:BTreeSet<_>=work.rows.iter().flat_map(|r|r.vendors.clone()).collect();for name in vendors{ui.selectable_value(&mut work.vendor,name.clone(),name);}});
                        if ui.button("Reload List").clicked(){self.refresh_workbench(&mut work);}
                        if self.fx_receiver.is_some(){ui.spinner();ui.small("更新完成后刷新列表");}
                    });
                    ui.horizontal_wrapped(|ui| {
                        if let Some((customer,currency))=work.scope(){ui.strong(format!("已锁定客户：{} · {}",work.rows.iter().find(|r|r.customer==customer).map(|r|r.customer_name.as_str()).unwrap_or(""),currency));}else{ui.small("勾选第一条任务后锁定客户和币种。");}
                        if ui.button("Clear Selection").clicked(){work.selected.clear();work.preview=None;}
                        if ui.add_enabled(work.scope().is_some(),egui::Button::new("Select Visible Customer Tasks")).clicked(){let ids:Vec<_>=work.rows.iter().filter(|r|r.issue.is_empty() && work.visible(r) && work.compatible(r)).map(|r|r.id).collect();work.selected.extend(ids);}
                    });
                    let hidden=work.rows.iter().filter(|r|work.selected.contains(&r.id) && !work.visible(r)).count();
                    ui.label(format!("已选 {} 个 Task，其中 {} 个不在当前筛选内（预览包含所有已选任务）",work.selected.len(),hidden));
                    if !work.notice.is_empty(){ui.label(&work.notice);}
                    egui::ScrollArea::horizontal().id_salt("workbench_table_scroll").show(ui,|ui| {
                        egui::Grid::new("workbench_candidates").striped(true).spacing([15.0,10.0]).show(ui,|ui| {
                            for label in ["选择","Customer（排序）","Task#","Date","Vendor","Task name","币种","未税金额","税额","含税金额"]{ui.strong(label);}ui.end_row();
                            let visible:Vec<_>=work.rows.iter().filter(|r|r.issue.is_empty() && work.visible(r)).cloned().collect();
                            for row in &visible {
                                let compatible=work.compatible(row);let mut checked=work.selected.contains(&row.id);
                                if ui.push_id(row.id,|ui|ui.add_enabled(compatible,egui::Checkbox::new(&mut checked,""))).inner.on_disabled_hover_text("一张发票只能选择同一 Customer 和币种的任务，请清除选择后切换。").changed(){if checked{work.selected.insert(row.id);}else{work.selected.remove(&row.id);}}
                                ui.label(&row.customer_name);
                                ui.label(&row.number);ui.label(&row.date);
                                ui.label(row.vendors.join(" / ")).on_hover_text("同一任务可能包含多个 Vendor，始终只列一行，不会重复开票。未记录 Vendor 的历史业务不会猜测供应商。");
                                ui.add(egui::Label::new(&row.title).truncate()).on_hover_text(&row.title);ui.label(&row.currency);ui.label(crate::format_money(row.net));ui.label(crate::format_money(row.tax));ui.label(crate::format_money(row.total));ui.end_row();
                            }
                        });
                    });
                    if !work.rows.iter().any(|r|r.issue.is_empty() && work.visible(r)){ui.label("暂无符合当前条件的任务。仅 completed、未开票、未取消且明细、税务、GL 科目及汇率有效的 Task 可开票。");}
                }
            });
            if !work.error.is_empty() {
                ui.colored_label(Color32::from_rgb(190, 45, 45), &work.error);
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if work.preview.is_some() {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Confirm & Issue Invoice").color(Color32::WHITE),
                            )
                            .fill(theme::GREEN),
                        )
                        .clicked()
                    {
                        self.confirm_workbench(&mut work);
                    }
                    if ui.button("Back to Selection").clicked() {
                        work.preview = None;
                    }
                } else if ui
                    .add_enabled(
                        !work.selected.is_empty(),
                        egui::Button::new("Preview Invoice"),
                    )
                    .clicked()
                {
                    self.preview_workbench(&mut work);
                }
                if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Cancel").clicked() {
                    self.request_business_close(windows::Kind::Invoice);
                }
            });
        });
        if response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            if work.preview.is_some() {
                work.preview = None;
            } else {
                self.request_business_close(windows::Kind::Invoice);
            }
        }
        self.invoice_workbench = work;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Result<Desktop> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer',''),(2,'Other',''); INSERT INTO vendors(id,name,contact) VALUES(1,'Zulu',''),(2,'Alpha',''); INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Z service','item',0,5000,'CAD'),(2,2,'A service','item',0,5000,'CAD'),(3,2,'USD service','item',0,5000,'USD');")?;
        db.execute(
            "INSERT INTO fx_rates(source,target,rate) VALUES('USD','CAD',135000000)",
            [],
        )?;
        for (customer, currency, price) in [
            (1, "CAD", 1),
            (1, "CAD", 2),
            (2, "CAD", 2),
            (1, "CAD", 2),
            (1, "CAD", 2),
            (1, "USD", 3),
        ] {
            let id = crate::add_task(&db, "Service task", customer, currency)?;
            if id != 5 {
                crate::add_item(&db, id, price, 1, None, None)?;
            }
        }
        crate::add_item(&db, 1, 2, 1, None, None)?;
        db.execute("UPDATE tasks SET progress=100 WHERE id!=4", [])?;
        Desktop::new(db, PathBuf::from("test.db"))
    }
    #[test]
    fn customer_order_scope_and_multi_vendor_tasks() -> Result<()> {
        let mut app = app()?;
        app.open_invoice_workbench();
        let w = &mut app.invoice_workbench;
        assert!(!w.rows.iter().any(|r| r.id == 4));
        assert_eq!(w.rows.iter().filter(|r| r.id == 1).count(), 1);
        assert_eq!(
            w.rows.iter().find(|r| r.id == 1).unwrap().vendors,
            ["Alpha", "Zulu"]
        );
        assert_eq!(
            w.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [1, 2, 6, 3]
        );
        assert!(!w.rows.iter().any(|r| r.id == 5));
        w.selected.insert(2);
        for (id, allowed) in [(1, true), (3, false), (6, false)] {
            assert_eq!(
                w.compatible(w.rows.iter().find(|r| r.id == id).unwrap()),
                allowed
            );
        }
        w.vendor = "Zulu".into();
        assert!(w.visible(w.rows.iter().find(|r| r.id == 1).unwrap()));
        assert!(!w.visible(w.rows.iter().find(|r| r.id == 2).unwrap()));
        crate::billing::merge(&mut app.db, &[3])?;
        crate::set_task_status(&mut app.db, 6, true)?;
        app.invoice_workbench.open = false; // Close the existing session before requesting fresh candidates.
        app.open_invoice_workbench();
        assert_eq!(
            app.invoice_workbench
                .rows
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        Ok(())
    }
    #[test]
    fn preview_is_read_only_and_confirm_revalidates_before_allocating() -> Result<()> {
        let mut app = app()?;
        app.checked_tasks.extend([1, 2, 4]);
        app.open_invoice_workbench();
        let mut w = std::mem::take(&mut app.invoice_workbench);
        assert_eq!(w.selected, BTreeSet::from([1, 2]));
        let writes = app.db.total_changes();
        app.preview_workbench(&mut w);
        assert!(w.preview.is_some(), "{}", w.error);
        assert_eq!(app.db.total_changes(), writes);
        assert!(w.preview.as_ref().unwrap()["number"].is_null());
        assert_eq!(w.preview.as_ref().unwrap()["total"], 15000);
        app.invoice_workbench = w;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.invoice_workbench_dialog(ctx));
        let mut w = std::mem::take(&mut app.invoice_workbench);
        app.db
            .execute("UPDATE items SET price=6000 WHERE task=2", [])?;
        assert!(!app.confirm_workbench(&mut w));
        assert!(!w.error.is_empty());
        assert!(w.preview.is_none());
        assert_eq!(w.selected.len(), 2);
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            app.db.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='invoice'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        app.preview_workbench(&mut w);
        let expected = w.preview.clone().unwrap();
        assert!(app.confirm_workbench(&mut w), "{}", w.error);
        assert_eq!(app.checked_tasks, BTreeSet::from([4]));
        assert_eq!(
            app.db
                .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            crate::billing::for_task(&app.db, 1)?,
            crate::billing::for_task(&app.db, 2)?
        );
        assert!(crate::billing::merge_confirmed(&mut app.db, &[1, 2], &expected).is_err());
        assert_eq!(
            app.db.query_row(
                "SELECT next_value FROM number_sequences WHERE kind='invoice'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        Ok(())
    }
}
