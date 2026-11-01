use super::*;
impl Desktop {
    pub(super) fn open_accounting_checks(&mut self) {
        if self.reuse_business(windows::Kind::Checks) {
            return;
        }
        match crate::compliance::review(&self.db) {
            Ok(findings) => self.menus.checks = Some(findings),
            Err(error) => {
                self.message = format!("检查无法完成：{error:#}");
                self.error = true;
            }
        }
    }
    pub(super) fn accounting_checks_dialog(&mut self, ctx: &egui::Context) {
        let Some(findings) = self.menus.checks.take() else {
            return;
        };
        let mut close = false;
        let response = windows::surface(ctx, egui::Id::new("accounting_checks"), |ui| {
            ui.set_width(850.0_f32.min((ctx.content_rect().width() - 64.0).max(300.0)));
            ui.heading("Accounting Checks");
            ui.label("只读检查，不代替供应地点判税、抵扣资格审核或专业会计复核。");
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-240.0).max(180.0)).show(ui,|ui|{
                if findings.is_empty() {ui.label("未发现本轮自动检查所覆盖的异常。仍须完成下列人工核对。");}
                for finding in &findings {
                    ui.colored_label(if finding.error {Color32::DARK_RED}else{Color32::from_rgb(158,110,28)},format!("{} · {}",finding.code,finding.message));
                    ui.separator();
                }
                ui.strong("必须另行核对");
                ui.label("供应地点与 BC PST 适用性；ITC 单据、注册号码和商业用途；期初及控制科目对账；信用票据/预收款/供应商支付；税务申报期；六年电子资料保管和外部备份。");
                ui.small("本机文件权限不能阻止同一 Windows 账户改库；本程序不提供不可篡改审计认证。");
            });
            if ctx.viewport_id() == egui::ViewportId::ROOT && ui.button("Close").clicked() {
                close = true;
            }
        });
        if !close
            && !(response.is_top_modal
                && !response.any_popup_open
                && ctx.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            self.menus.checks = Some(findings);
        }
    }
    pub(super) fn show_invoice_checks(&self, ui: &mut egui::Ui, data: &serde_json::Value) {
        let findings = crate::compliance::invoice_findings(data, "待开票");
        if !findings.is_empty() {
            egui::CollapsingHeader::new(format!("资料核对提醒 ({})", findings.len()))
                .default_open(true)
                .show(ui, |ui| {
                    for finding in findings {
                        ui.colored_label(Color32::from_rgb(158, 110, 28), finding.message);
                    }
                });
        }
    }
}
