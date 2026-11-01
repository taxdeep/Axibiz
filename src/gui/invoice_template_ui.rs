use super::*;

#[derive(Default)]
pub(super) struct TemplateUi {
    draft: Option<crate::invoice_template::Template>,
    error: String,
    status: String,
    baseline: Option<crate::invoice_template::Template>,
    close_guard: controls::CloseGuard,
    reset_guard: controls::CloseGuard,
}
impl TemplateUi {
    pub(super) fn is_open(&self) -> bool {
        self.draft.is_some()
    }
}
impl Desktop {
    pub(super) fn template_print_error(&mut self, error: &str) {
        if self.template_ui.draft.is_some() {
            self.template_ui.error = error.into();
            self.template_ui.status.clear();
        }
    }
    pub(super) fn open_invoice_template(&mut self) {
        if self.template_ui.draft.is_some() {
            return;
        }
        match crate::invoice_template::Template::load(&self.db) {
            Ok(draft) => {
                self.template_ui = TemplateUi {
                    baseline: Some(draft.clone()),
                    draft: Some(draft),
                    ..Default::default()
                }
            }
            Err(error) => {
                self.message = error.to_string();
                self.error = true;
            }
        }
    }
    pub(super) fn invoice_template_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.template_ui.draft.take() else {
            return;
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("invoice_template_settings")).show(ctx, |ui| {
            ui.set_width(640.0_f32.min((ctx.content_rect().width()-64.0).max(300.0)));
            ui.heading("Invoice Template");
            ui.separator();
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-220.0).max(200.0)).show(ui, |ui| {
                ui.label("English invoice · Letter paper · No issue timestamp");
                ui.small("适用于 HTML 和 PDF。发票日期保留，不显示开票时间；公司抬头和业务金额沿用原发票快照。模板修改会应用于之后的导出和重新导出。");
                field(ui,"Accent color",&mut draft.accent);
                ui.label("Payment terms (e.g. Net 30, as agreed with the customer)");
                ui.add(egui::TextEdit::multiline(&mut draft.payment_terms).desired_rows(2).desired_width(f32::INFINITY));
                ui.small("付款约定与付款说明只应用于新发票；已开票的约定保持原快照。旧发票缺失约定需核对原合同，不自动补填。");
                ui.label("Payment instructions");
                ui.add(egui::TextEdit::multiline(&mut draft.payment_instructions).desired_rows(4).desired_width(f32::INFINITY));
                ui.label("Footer");
                ui.add(egui::TextEdit::multiline(&mut draft.footer).desired_rows(2).desired_width(f32::INFINITY));
                ui.small("请输入英文付款说明和页脚。客户名称及业务内容按原始录入显示，不会自动翻译。");
            });
            if !self.template_ui.error.is_empty() {ui.colored_label(Color32::from_rgb(190,45,45),&self.template_ui.error);}
            if !self.template_ui.status.is_empty() {ui.label(&self.template_ui.status);}
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save").clicked() {
                    match draft.save(&self.db) {
                        Ok(())=>{self.template_ui.baseline=Some(draft.clone());self.template_ui.error.clear();self.template_ui.status="Template saved".into();},
                        Err(error)=>self.template_ui.error=error.to_string(),
                    }
                }
                if ui.add_enabled(self.print_receiver.is_none(),egui::Button::new("Preview / Print…")).clicked() {
                    match crate::invoice::preview(&self.db,&draft) {
                        Ok(html)=>{self.start_print_preview(ctx,html);self.template_ui.error.clear();self.template_ui.status="Print preview is opening. Close the preview window when finished.".into();},
                        Err(error)=>self.template_ui.error=error.to_string(),
                    }
                }
                if ui.button("Reset draft").clicked() {self.template_ui.reset_guard.request();}
                if ui.button("Close").clicked() {self.template_ui.close_guard.request();}
            });
        });
        let escape = response.is_top_modal
            && !response.any_popup_open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            self.template_ui.close_guard.request();
        }
        let dirty = self.template_ui.baseline.as_ref() != Some(&draft);
        if self
            .template_ui
            .reset_guard
            .show(ctx, "template_reset", dirty)
        {
            draft = Default::default();
        }
        close |= self
            .template_ui
            .close_guard
            .show(ctx, "template_close", dirty);
        if !close {
            self.template_ui.draft = Some(draft);
        }
    }
}
