use super::*;
pub(super) struct ReconciliationUi {
    date: chrono::NaiveDate,
    report: Option<crate::reconciliation::Report>,
    error: String,
}
impl Desktop {
    pub(super) fn open_reconciliation(&mut self) {
        if self.reuse_business(windows::Kind::Reconciliation) {
            return;
        }
        let date = crate::today();
        let (report, error) = match crate::reconciliation::build(&self.db, &date.to_string()) {
            Ok(report) => (Some(report), String::new()),
            Err(error) => (None, format!("{error:#}")),
        };
        self.menus.reconciliation = Some(ReconciliationUi {
            date,
            report,
            error,
        });
    }
    pub(super) fn reconciliation_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.menus.reconciliation.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("reconciliation"), |ui| {
            ui.set_width(1150.0_f32.min((ctx.content_rect().width() - 64.0).max(300.0)));
            ui.set_height((ctx.content_rect().height() - 100.0).clamp(300.0, 860.0));
            ui.heading("Reconciliation");
            ui.horizontal(|ui| {
                ui.label("截止日期");
                ui.add(
                    egui_extras::DatePickerButton::new(&mut state.date)
                        .id_salt("reconciliation_date"),
                );
                if ui.button("Run Checks").clicked() {
                    match crate::reconciliation::build(&self.db, &state.date.to_string()) {
                        Ok(report) => {
                            state.report = Some(report);
                            state.error.clear();
                        }
                        Err(error) => {
                            state.report = None;
                            state.error = format!("{error:#}");
                        }
                    }
                }
            });
            egui::ScrollArea::both().id_salt("reconciliation_body").auto_shrink([false,false]).max_height((ctx.content_rect().height()-270.0).clamp(140.0,650.0)).show(ui,|ui|{
                if let Some(report)=&state.report {
                    ui.label(format!("报告截止日：{} · GL 本币：{}",report.date,report.base_currency));
                    if report.date!=state.date.to_string(){ui.colored_label(Color32::DARK_RED,"日期已改变，请重新检查后再导出。");}
                    ui.strong("原币应收（不同币种不相加）");
                    for balance in &report.currencies {ui.label(format!("{}  {}",balance.currency,balance.remaining.map(crate::format_money).unwrap_or_else(||"日期不完整，无法确定".into())));}
                    ui.separator();
                    ui.strong("控制科目：差额 = GL − 来源账面值 / 税务子账");
                    egui::Grid::new("reconciliation_controls").striped(true).spacing([14.0,10.0]).show(ui,|ui|{
                        for title in ["范围","来源币种","来源金额","GL 币种","GL 金额","差额","结果"] {ui.strong(title);}ui.end_row();
                        for control in &report.controls {
                            ui.add(egui::Label::new(&control.label).wrap());ui.label(&control.source_currency);ui.label(crate::format_money(control.source));ui.label(&report.base_currency);ui.label(crate::format_money(control.ledger));
                            ui.label(control.difference.map(crate::format_money).unwrap_or_else(||"—".into()));
                            ui.label(match control.difference {Some(0)=>"已记录范围相符",Some(_)=>"差异待核对",None=>"不完整 / 不可比"});ui.end_row();
                        }
                    });
                    ui.separator();
                    for note in &report.notes {ui.label(note);}
                    ui.small("相符不代表期初、分类、凭证或税务资格已获确认；本功能不会自动调整或锁定账目。");
                }
            });
            if !state.error.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(&state.error).color(Color32::DARK_RED))
                        .truncate(),
                )
                .on_hover_text(&state.error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                let ready = state
                    .report
                    .as_ref()
                    .is_some_and(|report| report.date == state.date.to_string());
                if ui
                    .add_enabled(ready, egui::Button::new("Export CSV"))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("CSV", &["csv"])
                        .set_file_name(format!("reconciliation-{}.csv", state.date))
                        .save_file()
                    {
                        if let Some(report) = &state.report {
                            match crate::tax::admin::export(
                                &report.csv_rows(),
                                &crate::reconciliation::CSV_HEADERS,
                                &path,
                            ) {
                                Ok(()) => {
                                    self.message = format!("对账 CSV 已保存：{}", path.display());
                                    self.error = false;
                                }
                                Err(error) => state.error = format!("{error:#}"),
                            }
                        }
                    }
                }
                if ctx.viewport_id() == egui::ViewportId::ROOT {
                    let button = ui.button("Close");
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("reconciliation_close_rect"), button.rect)
                    });
                    if button.clicked() {
                        close = true;
                    }
                }
            });
        });
        if !close
            && !(response.is_top_modal
                && !response.any_popup_open
                && ctx.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            self.menus.reconciliation = Some(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconciliation_dialog_keeps_actions_visible_and_does_not_write_business_data() -> Result<()>
    {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        let changes = app.db.total_changes();
        app.open_reconciliation();
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
            let close = ctx
                .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("reconciliation_close_rect")))
                .unwrap();
            assert!(
                screen.contains(close.min) && screen.contains(close.max),
                "Close action outside viewport: {close:?}"
            );
        }
        assert_eq!(app.db.total_changes(), changes);
        app.reset_after_restore()?;
        assert!(app.menus.reconciliation.is_none());
        Ok(())
    }
}
