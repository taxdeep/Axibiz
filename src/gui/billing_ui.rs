use super::*;
use serde_json::Value;
#[derive(Default)]
pub(super) struct BillingUi {
    selected: i64,
    pub(super) open: bool,
    error: String,
    pdf_status: String,
    list_page: i64,
    list_search: String,
    void: Option<i64>,
    void_reason: String,
    void_focus: bool,
}
impl Desktop {
    pub(super) fn poll_pdf(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = self.print_receiver.as_ref() {
            match receiver.try_recv() {
                Ok(result) => {
                    self.print_receiver = None;
                    match result {
                        Ok(()) => self.billing_ui.pdf_status = "打印窗口已关闭".into(),
                        Err(error) => {
                            self.template_print_error(&error);
                            self.billing_ui.error = error.clone();
                            self.message = error;
                            self.error = true;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(250))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.print_receiver = None;
                    self.billing_ui.error = "打印预览任务意外结束，请重试".into();
                }
            }
        }
        let Some(receiver) = self.pdf_receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.pdf_receiver = None;
                match result {
                    Ok(path) => {
                        self.message = format!("PDF 已保存：{}", path.display());
                        self.billing_ui.pdf_status = self.message.clone();
                        self.error = false;
                    }
                    Err(error) => {
                        self.message = error.clone();
                        self.billing_ui.error = error;
                        self.error = true;
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pdf_receiver = None;
                self.billing_ui.error = "PDF 生成任务意外结束，请重试".into();
            }
        }
    }
    fn save_invoice_pdf(&mut self, ctx: &egui::Context, id: i64, number: &str) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save Invoice PDF")
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{number}.pdf"))
            .save_file()
        {
            match crate::invoice::document(&self.db, id) {
                Ok(html) => {
                    let (sender, receiver) = std::sync::mpsc::channel();
                    self.pdf_receiver = Some(receiver);
                    self.billing_ui.error.clear();
                    self.billing_ui.pdf_status.clear();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let result = crate::pdf::save(&html, &path)
                            .map(|_| path)
                            .map_err(|e| format!("{e:#}"));
                        let _ = sender.send(result);
                        ctx.request_repaint();
                    });
                }
                Err(error) => self.billing_ui.error = format!("{error:#}"),
            }
        }
    }
    pub(super) fn print_invoice(&mut self, ctx: &egui::Context, id: i64) {
        match crate::invoice::document(&self.db, id) {
            Ok(html) => self.start_print_preview(ctx, html),
            Err(error) => self.billing_ui.error = format!("{error:#}"),
        }
    }
    pub(super) fn start_print_preview(&mut self, ctx: &egui::Context, html: String) {
        if self.print_receiver.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.print_receiver = Some(receiver);
        self.billing_ui.error.clear();
        self.billing_ui.pdf_status =
            "正在打开打印预览，请选择打印机；完成或取消后关闭预览窗口。".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::pdf::print(&html).map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn audit_page(&mut self, ui: &mut egui::Ui) {
        ui.small("Latest 200 recorded events. Timestamps use your saved software display preferences; the UTC offset is shown. Expand an entry to view changes. Original timestamps remain stored unchanged.");
        let rows=self.rows("SELECT id,occurred_at,entity,record_id,action,COALESCE(before_json,''),COALESCE(after_json,''),actor FROM audit_log ORDER BY id DESC LIMIT 200");
        for row in rows {
            if !self.row_matches(&row) {
                continue;
            }
            egui::CollapsingHeader::new(format!(
                "{} · {} #{} · {} · {}",
                self.software.saved.display_time(&row[1]),
                row[2],
                row[3],
                row[4],
                row[7]
            ))
            .id_salt(("audit_entry", &row[0]))
            .show(ui, |ui| {
                for (label, index) in [("修改前", 5), ("修改后", 6)] {
                    ui.strong(label);
                    let pretty = serde_json::from_str::<Value>(&row[index])
                        .and_then(|v| serde_json::to_string_pretty(&v))
                        .unwrap_or_else(|_| "—".into());
                    ui.add(egui::Label::new(RichText::new(pretty).monospace()).wrap());
                }
            });
        }
    }
    pub(super) fn open_invoice(&mut self, id: i64) {
        if self.reuse_business(windows::Kind::Invoice) {
            return;
        }
        self.billing_ui = BillingUi {
            selected: id,
            open: true,
            ..Default::default()
        };
        if self.page != 1 {
            self.navigate(8);
        }
    }
    pub(super) fn invoices_page(&mut self, ui: &mut egui::Ui) {
        if self.billing_ui.list_search != self.list_query {
            self.billing_ui.list_search = self.list_query.clone();
            self.billing_ui.list_page = 0;
        }
        let mut rows = match crate::billing::list_invoices(
            &self.db,
            &self.list_query,
            101,
            self.billing_ui.list_page * 100,
        ) {
            Ok(rows) => rows,
            Err(error) => {
                ui.colored_label(Color32::DARK_RED, format!("发票列表无法读取：{error:#}"));
                return;
            }
        };
        let has_next = rows.len() > 100;
        rows.truncate(100);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.billing_ui.list_page > 0, egui::Button::new("Previous"))
                .clicked()
            {
                self.billing_ui.list_page -= 1;
            }
            ui.label(format!(
                "Page {} · 每页最多 100 条",
                self.billing_ui.list_page + 1
            ));
            if ui
                .add_enabled(has_next, egui::Button::new("Next"))
                .clicked()
            {
                self.billing_ui.list_page += 1;
            }
        });
        let mut open = None;
        egui::ScrollArea::horizontal()
            .id_salt("invoice_list_scroll")
            .show(ui, |ui| {
                egui::Grid::new("invoice_list")
                    .striped(true)
                    .spacing([20.0, 14.0])
                    .show(ui, |ui| {
                        for title in [
                            "Invoice #",
                            "Customer",
                            "日期",
                            "币种",
                            "总额",
                            "已收",
                            "未付",
                            "状态",
                            "",
                        ] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for row in rows {
                            ui.label(&row.number);
                            ui.label(&row.customer);
                            ui.label(&row.date);
                            ui.label(&row.balance.currency);
                            ui.label(crate::format_money(row.balance.total));
                            ui.label(crate::format_money(row.balance.received));
                            ui.label(crate::format_money(row.balance.remaining));
                            theme::badge(ui, row.balance.status());
                            if ui
                                .push_id(row.id, |ui| ui.button("View / Receive Payment"))
                                .inner
                                .clicked()
                            {
                                open = Some(row.id);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(id) = open {
            self.open_invoice(id);
        }
        ui.small("点击 New Invoice 直接开票；也可在 Task 页面选择同客户、同币种的 completed Task 合并开票。");
    }
    pub(super) fn invoice_dialog(&mut self, ctx: &egui::Context) {
        if !self.billing_ui.open {
            return;
        }
        let id = self.billing_ui.selected;
        let details = (|| -> Result<(String, Value, crate::billing::Balance)> {
            let (number, raw): (String, String) = self.db.query_row(
                "SELECT number,snapshot FROM invoices WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok((
                number,
                serde_json::from_str(&raw)?,
                crate::billing::balance(&self.db, id)?,
            ))
        })();
        let (number, data, balance) = match details {
            Ok(v) => v,
            Err(e) => {
                self.message = e.to_string();
                self.error = true;
                self.billing_ui.open = false;
                return;
            }
        };
        let mut close = false;
        let mut void = None;
        let response = windows::surface(ctx, egui::Id::new("invoice_details"), |ui| {
            invoice_sheet::compact(ui);
            ui.set_width((ctx.content_rect().width() - 64.0).max(300.0));
            ui.set_height((ctx.content_rect().height() - 80.0).clamp(300.0, 920.0));
            let toolbar_width = (invoice_sheet::finite_width(ui) - 12.0).max(180.0);
            egui::Frame::new()
                .fill(theme::CANVAS)
                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                .inner_margin(6)
                .show(ui, |ui| {
                    ui.set_min_width(toolbar_width);
                    ui.horizontal(|ui| {
                        let pdf = theme::document_action(
                            ui,
                            "Save PDF",
                            theme::DocumentIcon::Pdf,
                            self.pdf_receiver.is_none(),
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::new("invoice_pdf_rect"), pdf.rect)
                        });
                        if pdf.clicked() {
                            self.save_invoice_pdf(ctx, id, &number);
                        }
                        let print = theme::document_action(
                            ui,
                            "Print",
                            theme::DocumentIcon::Print,
                            self.print_receiver.is_none(),
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::new("invoice_print_rect"), print.rect)
                        });
                        if print.clicked() {
                            self.print_invoice(ctx, id);
                        }
                        let receive = theme::document_action(
                            ui,
                            "Receive Payment",
                            theme::DocumentIcon::Payment,
                            balance.remaining > 0,
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(egui::Id::new("invoice_receive_rect"), receive.rect)
                        });
                        if receive.clicked() {
                            self.open_payment(id);
                        }
                        if self.pdf_receiver.is_some() {
                            ui.spinner();
                            ui.label("Saving PDF...");
                        }
                        if self.print_receiver.is_some() {
                            ui.spinner();
                            ui.label("Opening print preview...");
                        }
                    });
                });
            ui.horizontal(|ui| {
                ui.heading(format!("Invoice {number}"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
            invoice_sheet::band(ui, &data, &number);
            ui.separator();
            let body_height = (ctx.content_rect().height() - 280.0).clamp(180.0, 760.0);
            let body = egui::ScrollArea::vertical().id_salt(("invoice_details_scroll",id)).auto_shrink([false,false]).max_height(body_height).show(ui,|ui| {
                theme::badge(ui,balance.status());
                let tasks=self.rows(&format!("SELECT t.id,COALESCE(t.number,''),t.title FROM invoice_tasks it JOIN tasks t ON t.id=it.task WHERE it.invoice={id} ORDER BY t.id"));
                ui.label(format!("未税小计：{} · 税额：{}",crate::format_money(data["net_total"].as_i64().unwrap_or(balance.total)),crate::format_money(data["tax_total"].as_i64().unwrap_or(0))));
                self.show_gl_source(ui,"invoice",id);
                ui.separator();if tasks.is_empty(){ui.label("Direct Invoice · 直接开票");}else{ui.strong("关联 Task");for task in tasks {ui.label(format!("{} · {}",task[1],task[2]));}}
                ui.separator();ui.strong("业务明细");
                invoice_sheet::snapshot_lines(ui, "invoice_items", &data, (ctx.content_rect().height() - 480.0).clamp(180.0, 620.0));
                invoice_sheet::totals(ui, &balance.currency, data["net_total"].as_i64().unwrap_or(balance.total), data["tax_total"].as_i64().unwrap_or(0), balance.total, Some(balance.received));
                ui.separator();ui.strong("付款记录");
                let payments=self.rows(&format!("SELECT id,COALESCE(number,''),COALESCE(date,'未知（旧版）'),printf('%d.%02d',amount/100,amount%100),method,reference,voided,void_reason FROM payments WHERE invoice={id} ORDER BY id"));
                egui::ScrollArea::horizontal().id_salt("invoice_payment_scroll").show(ui,|ui| {
                    egui::Grid::new("invoice_payments").striped(true).spacing([16.0,10.0]).show(ui,|ui| {
                        for title in ["Payment #","日期","金额","方式","参考号","状态",""] {ui.strong(title);}ui.end_row();
                        for payment in payments {
                            for value in &payment[1..6] {ui.label(value);}
                            ui.label(if payment[6]=="1" {"voided"} else {"received"}).on_hover_text(&payment[7]);
                            if ui.add_enabled(payment[6]=="0",egui::Button::new("Void")).clicked() {void=payment[0].parse().ok();}ui.end_row();
                        }
                    });
                });
            });
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("invoice_body_rect"), body.inner_rect)
            });
            #[cfg(not(test))]
            let _ = body;
            ui.separator();
            if !self.billing_ui.error.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(&self.billing_ui.error).color(Color32::from_rgb(190, 45, 45)),
                    )
                    .truncate(),
                )
                .on_hover_text(&self.billing_ui.error);
            }
            if !self.billing_ui.pdf_status.is_empty() {
                ui.add(egui::Label::new(&self.billing_ui.pdf_status).truncate())
                    .on_hover_text(&self.billing_ui.pdf_status);
            }
        });
        if let Some(payment) = void {
            self.billing_ui.void = Some(payment);
            self.billing_ui.void_reason.clear();
            self.billing_ui.void_focus = true;
            self.billing_ui.error.clear();
        }
        if let Some(payment) = self.billing_ui.void {
            let values = self.db.query_row(
                "SELECT COALESCE(number,''),date,amount,voided FROM payments WHERE id=?",
                [payment],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, bool>(3)?,
                    ))
                },
            );
            let (values, stale) = match values {
                Ok((number, date, amount, voided)) => (
                    vec![
                        ("Receipt", number),
                        ("Original date", date.unwrap_or_else(|| "Unknown".into())),
                        (
                            "Amount",
                            format!("{} {}", balance.currency, crate::format_money(amount)),
                        ),
                        ("Reversal date", crate::today().to_string()),
                    ],
                    voided,
                ),
                Err(_) => (vec![], true),
            };
            let error = if stale {
                Some("The receipt is unavailable or already voided.")
            } else if self.billing_ui.void_reason.trim().is_empty() {
                Some("A reason is required.")
            } else if !self.billing_ui.error.is_empty() {
                Some(self.billing_ui.error.as_str())
            } else {
                None
            };
            let before = self.billing_ui.void_reason.clone();
            let action=controls::review_content(ctx,controls::Review{ cancel: "Keep Editing",id:"receipt_void",title:"Void Receipt",values:&values,error,confirm:"Confirm & Void",notice:"The original receipt remains in history. A dated reversal reopens the receivable."},&mut self.billing_ui.void_focus,|ui| {ui.label("Reason *");ui.add(egui::TextEdit::multiline(&mut self.billing_ui.void_reason).desired_rows(2).desired_width(f32::INFINITY));});
            if before != self.billing_ui.void_reason {
                self.billing_ui.error.clear();
            }
            match action {
                Some(controls::ReviewAction::Keep) => self.billing_ui.void = None,
                Some(controls::ReviewAction::Confirm) => match crate::billing::void_payment(
                    &mut self.db,
                    payment,
                    &self.billing_ui.void_reason,
                ) {
                    Ok(()) => {
                        self.billing_ui.void = None;
                        self.billing_ui.error.clear()
                    }
                    Err(e) => self.billing_ui.error = format!("{e:#}"),
                },
                None => {}
            }
        }
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if close || escape {
            self.request_business_close(windows::Kind::Invoice);
        }
    }
    pub(super) fn numbering_page(&mut self, ui: &mut egui::Ui) {
        if self.number_rules.is_none() {
            match crate::numbering::rules(&self.db) {
                Ok(rules) => self.number_rules = Some(rules),
                Err(e) => {
                    ui.label(e.to_string());
                    return;
                }
            }
        }
        ui.small("Set prefixes, minimum digits and the next number. Used numbers are skipped; existing documents remain unchanged.");
        let rules = self.number_rules.as_mut().unwrap();
        egui::Grid::new("numbering_settings")
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                for label in ["Type", "Prefix", "Digits", "Next Number", "Example"] {
                    ui.strong(label);
                }
                ui.end_row();
                for rule in rules.iter_mut() {
                    ui.label(&rule.kind);
                    ui.add(egui::TextEdit::singleline(&mut rule.prefix).desired_width(110.0));
                    ui.add(egui::DragValue::new(&mut rule.width).range(1..=12));
                    ui.add(egui::DragValue::new(&mut rule.next).range(1..=i64::MAX - 1));
                    ui.label(format!(
                        "{}{:0width$}",
                        rule.prefix,
                        rule.next,
                        width = rule.width
                    ));
                    ui.end_row();
                }
            });
        let mut save = None;
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                save = Some(false);
            }
            if ui.button("Save & Close").clicked() {
                save = Some(true);
            }
        });
        if let Some(close) = save {
            match crate::numbering::configure(&mut self.db, rules) {
                Ok(()) => {
                    self.message = "编号规则已保存，之后自动分配".into();
                    self.error = false;
                    self.number_rules = None;
                    if close {
                        self.page = 0;
                        self.list_query.clear();
                    }
                }
                Err(e) => {
                    self.message = e.to_string();
                    self.error = true;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invoice_list_and_receipts_render() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        db.execute_batch("INSERT INTO customers(id,name,contact) VALUES(1,'Customer','');INSERT INTO vendors(id,name,contact) VALUES(1,'Vendor','');INSERT INTO prices(id,vendor,name,unit,cost,price,currency) VALUES(1,1,'Service','item',0,5000,'CAD');")?;
        crate::add_task(&db, "Task", 1, "CAD")?;
        crate::add_item(&db, 1, 1, 1, None, None)?;
        for _ in 0..24 {
            crate::add_item(&db, 1, 1, 1, None, None)?;
        }
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        crate::set_task_status(&mut app.db, 1, false)?;
        let id = crate::billing::merge(&mut app.db, &[1])?;
        app.open_invoice(id);
        let ctx = egui::Context::default();
        for size in [egui::vec2(1600.0, 1000.0), egui::vec2(900.0, 650.0)] {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| app.draw(ctx),
                );
            }
            let body = ctx
                .data_mut(|data| data.get_temp::<egui::Rect>(egui::Id::new("invoice_body_rect")))
                .unwrap();
            let pdf = ctx
                .data_mut(|data| data.get_temp::<egui::Rect>(egui::Id::new("invoice_pdf_rect")))
                .unwrap();
            let print = ctx
                .data_mut(|data| data.get_temp::<egui::Rect>(egui::Id::new("invoice_print_rect")))
                .unwrap();
            let receive = ctx
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("invoice_receive_rect")))
                .unwrap();
            assert!(
                screen.contains_rect(receive) && receive.bottom() <= body.top(),
                "Receive Payment must remain visible in the top action strip"
            );
            assert!(screen.contains(print.min) && screen.contains(print.max));
            assert!(body.height() >= size.y * 0.55, "body too small: {body:?}");
            assert!(
                screen.contains(pdf.min) && screen.contains(pdf.max),
                "PDF action offscreen: {pdf:?}"
            );
            assert!(
                pdf.bottom() <= body.top() && print.bottom() <= body.top(),
                "Document actions must stay above the scrolling body"
            );
        }
        Ok(())
    }
}
