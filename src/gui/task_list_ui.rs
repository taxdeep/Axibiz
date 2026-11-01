use super::*;
impl Desktop {
    pub(super) fn invoice_checked_tasks(&mut self) {
        self.open_invoice_workbench();
    }
    pub(super) fn tasks(&mut self, ui: &mut egui::Ui) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(60));
        let rows=self.rows(&format!("SELECT t.id,t.title,{} AS status,t.number,COALESCE(t.task_date,'—') FROM tasks t LEFT JOIN invoice_tasks it ON it.task=t.id LEFT JOIN invoices i ON i.id=it.invoice ORDER BY {}",crate::TASK_STATUS_SQL,crate::TASK_ORDER_SQL));
        ui.horizontal_wrapped(|ui| {
            for (code, label) in [
                ("all", "All"),
                ("pending", "Pending"),
                ("completed", "Completed"),
                ("invoiced", "Invoiced"),
                ("cancel", "Cancel"),
            ] {
                let count = rows
                    .iter()
                    .filter(|r| code == "all" || r[2] == code)
                    .count();
                ui.selectable_value(
                    &mut self.task_filter,
                    code.to_owned(),
                    format!("{label}  {count}"),
                );
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.small(format!("已勾选 {} 个 Task", self.checked_tasks.len()));
            if !self.checked_tasks.is_empty() && ui.small_button("Clear Selection").clicked() {
                self.checked_tasks.clear();
            }
        });
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|r| {
                (self.task_filter == "all" || r[2] == self.task_filter) && self.row_matches(r)
            })
            .collect();
        if !self.invoice_results.is_empty() {
            ui.collapsing("查看批量开票结果", |ui| {
                for (task, message, success) in &self.invoice_results {
                    ui.colored_label(
                        if *success {
                            Color32::from_rgb(20, 110, 85)
                        } else {
                            Color32::from_rgb(190, 45, 45)
                        },
                        format!("Task #{task}：{message}"),
                    );
                }
            });
        }
        ui.add_space(8.0);

        let mut edit = None;
        let as_of = crate::today();
        egui::ScrollArea::horizontal()
            .id_salt("task_list_horizontal")
            .show(ui, |ui| {
                egui_extras::TableBuilder::new(ui)
                    .id_salt("task_list")
                    .striped(true)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(egui_extras::Column::exact(40.0))
                    .column(egui_extras::Column::exact(100.0))
                    .column(egui_extras::Column::exact(110.0))
                    .column(egui_extras::Column::remainder().at_least(240.0))
                    .column(egui_extras::Column::exact(60.0))
                    .column(egui_extras::Column::exact(130.0))
                    .column(egui_extras::Column::exact(76.0))
                    .header(32.0, |mut header| {
                        for title in ["选择", "Task#", "Date", "Task name", "Days", "Status", ""]
                        {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(title).size(11.0).color(theme::MUTED).strong(),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(44.0, rows.len(), |mut row| {
                            let data = &rows[row.index()];
                            let id = data[0].parse::<i64>().unwrap_or(0);
                            row.col(|ui| {
                                let mut checked = self.checked_tasks.contains(&id);
                                let checkbox = ui
                                    .push_id(("task_check", id), |ui| ui.checkbox(&mut checked, ""))
                                    .inner;
                                if checkbox.changed() {
                                    if checked {
                                        self.checked_tasks.insert(id);
                                    } else {
                                        self.checked_tasks.remove(&id);
                                    }
                                }
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new(("task_checkbox_rect", id)),
                                        checkbox.rect,
                                    )
                                });
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&data[3]).strong());
                            });
                            row.col(|ui| {
                                ui.label(&data[4]);
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&data[1]).strong()).truncate(),
                                )
                                .on_hover_ui(|ui| self.task_preview(ui, id));
                            });
                            row.col(|ui| {
                                let days = crate::task_age_days(Some(&data[4]), as_of);
                                ui.label(days.map(|d| d.to_string()).unwrap_or_else(|| "—".into()))
                                    .on_hover_text(if days.is_some() {
                                        format!(
                                            "从任务日期 {} 计算至 {}（温哥华日期），未来日期显示 0",
                                            data[4], as_of
                                        )
                                    } else {
                                        "未设置有效的任务日期，无法确定天数".into()
                                    });
                            });
                            row.col(|ui| theme::badge(ui, &data[2]));
                            row.col(|ui| {
                                let button = ui
                                    .push_id(("task_edit", id), |ui| {
                                        icons::button(ui, icons::Icon::Edit, "Edit record")
                                    })
                                    .inner;
                                if button.clicked() {
                                    edit = Some(id);
                                }
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new(("task_edit_rect", id)),
                                        button.rect,
                                    )
                                });
                            });
                        })
                    });
            });
        if rows.is_empty() {
            ui.label(
                if self.task_filter != "all" || !self.list_query.trim().is_empty() {
                    "没有匹配的 Task，请调整搜索或状态筛选。"
                } else {
                    "暂无 Task，点击右上角 New Task 添加。"
                },
            );
        }
        if let Some(id) = edit {
            self.open_task_editor(id);
        }
    }
    pub(super) fn task_preview(&self, ui: &mut egui::Ui, id: i64) {
        ui.set_width(500.0_f32.min((ui.ctx().content_rect().width() - 48.0).max(280.0)));
        let data = match crate::snapshot(&self.db, id) {
            Ok(data) => data,
            Err(e) => {
                ui.label(format!("无法加载任务内容：{e}"));
                return;
            }
        };
        let currency = data["currency"].as_str().unwrap_or("UNSET");
        ui.label(RichText::new(data["title"].as_str().unwrap_or("")).strong());
        ui.horizontal(|ui| {
            ui.label(data["number"].as_str().unwrap_or(""));
            theme::badge(ui, data["status"].as_str().unwrap_or("pending"));
        });
        ui.label(format!("客户：{}", data["customer"].as_str().unwrap_or("")));
        ui.label(format!(
            "任务日期：{}",
            data["task_date"].as_str().unwrap_or("未设置")
        ));
        ui.separator();
        let items = data["items"].as_array().unwrap();
        if items.is_empty() {
            ui.label("此任务尚未添加业务明细。");
        } else {
            egui::ScrollArea::vertical()
                .id_salt(("task_preview_items", id))
                .max_height(240.0)
                .show(ui, |ui| {
                    egui::Grid::new(("task_preview_grid", id))
                        .striped(true)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            for title in ["业务", "数量", "单价", "小计"] {
                                ui.strong(title);
                            }
                            ui.end_row();
                            for item in items {
                                ui.add_sized(
                                    [210.0, 0.0],
                                    egui::Label::new(item["description"].as_str().unwrap_or(""))
                                        .wrap(),
                                );
                                ui.label(format!(
                                    "{} {}",
                                    item["quantity"].as_i64().unwrap_or(0),
                                    item["unit"].as_str().unwrap_or("")
                                ));
                                ui.label(crate::format_money(
                                    item["unit_price"].as_i64().unwrap_or(0),
                                ));
                                ui.label(crate::format_money(
                                    item["subtotal"].as_i64().unwrap_or(0),
                                ));
                                ui.end_row();
                            }
                        });
                });
        }
        ui.separator();
        ui.label(
            RichText::new(format!(
                "合计：{} {}",
                currency,
                crate::format_money(data["total"].as_i64().unwrap_or(0))
            ))
            .strong(),
        );
        ui.small(RichText::new("点击 Edit 可查看或修改任务。").color(theme::MUTED));
    }
}
