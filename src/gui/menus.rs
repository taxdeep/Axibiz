use super::*;
#[derive(Default)]
pub(super) struct Menus {
    about: bool,
    restore: Option<PathBuf>,
    restore_focus: bool,
    pub(super) legacy_currency: Option<String>,
    legacy_focus: bool,
    pub(super) checks: Option<Vec<crate::compliance::Finding>>,
    pub(super) reconciliation: Option<super::reconciliation_ui::ReconciliationUi>,
}
impl Desktop {
    pub(super) fn menu_bar(&mut self, ctx: &egui::Context) {
        let mut action = None;
        egui::TopBottomPanel::top("main_menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Backup…").clicked() {
                        action = Some(0);
                        ui.close();
                    }
                    let admin=crate::auth::current(&self.db).is_ok_and(|u|u.admin);
                    if ui.add_enabled(admin,egui::Button::new("Restore…")).on_hover_text("Company restore requires administrator access").clicked() {
                        action = Some(1);
                        ui.close();
                    }
                    if ui.button("Verify Backup...").clicked() {
                        action = Some(8);
                        ui.close();
                    }
                    ui.separator();
                    let can_lock=crate::auth::required(&self.db).unwrap_or(false) && !self.has_business_editors();
                    if ui.add_enabled(can_lock,egui::Button::new("Lock / Switch User")).on_hover_text("Available when password sign-in is enabled and all drafts are saved or discarded").clicked() {action=Some(17);ui.close();}
                    if ui.button("Exit").clicked() {
                        action = Some(2);
                        ui.close();
                    }
                });
                ui.menu_button("Lists", |ui| {
                    for page in [1, 2, 3, 7] {
                        if ui.button(PAGES[page].title).clicked() {
                            self.navigate(page);
                            ui.close();
                        }
                    }
                });
                let transactions = ui.menu_button("Transactions", |ui| {
                    for page in [0, 8, 6, 13] {
                        if let Some(label) = creation_label(page) {
                            let label = label.trim_start_matches(['+', '＋']).trim_start();
                            if icons::menu_item(ui, icons::Icon::Add, label).clicked() {
                                self.create_on(page);
                                ui.close();
                            }
                        }
                    }
                    ui.separator();
                    if icons::menu_item(ui, icons::Icon::Payment, "Receive Payment...")
                        .on_hover_text("Choose an outstanding invoice first")
                        .clicked()
                    {
                        self.navigate(8);
                        ui.close();
                    }
                    if icons::menu_item(ui, icons::Icon::Payment, "Pay Bills...").clicked() {
                        self.open_pay_bills(0, None);
                        ui.close();
                    }
                });
                #[cfg(test)]
                ctx.data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("transactions_menu_rect"),
                        transactions.response.rect,
                    )
                });
                #[cfg(not(test))]
                let _ = transactions;
                ui.menu_button("Setup", |ui| {
                    if ui.button("Users...").clicked() {
                        action = Some(16);
                        ui.close();
                    }
                    if ui.button("Invoice Template").clicked() {
                        action = Some(13);
                        ui.close();
                    }
                    if ui.button("GL Setup").clicked() {
                        action = Some(11);
                        ui.close();
                    }
                    if ui.button("Sales Tax Setup").clicked() {
                        action = Some(9);
                        ui.close();
                    }
                    if ui.button("Exchange Rates").clicked() {
                        action = Some(5);
                        ui.close();
                    }
                    if ui.button("Company Settings").clicked() {
                        action = Some(3);
                        ui.close();
                    }
                    if ui.button("Document Numbering").clicked() {
                        action = Some(6);
                        ui.close();
                    }
                    if ui.button("Audit Log").clicked() {
                        action = Some(7);
                        ui.close();
                    }
                });
                ui.menu_button("Reports", |ui| {
                    if ui.button("Reconciliation").clicked() {
                        action = Some(15);
                        ui.close();
                    }
                    if ui.button("Accounting Checks").clicked() {
                        action = Some(14);
                        ui.close();
                    }
                    if ui.button("General Ledger / Trial Balance").clicked() {
                        action = Some(12);
                        ui.close();
                    }
                    if ui.button("Sales Tax").clicked() {
                        action = Some(10);
                        ui.close();
                    }
                });
                self.window_menu(ui);
                ui.menu_button("Help", |ui| {
                    if ui.button("About Axibiz Desktop Beta").clicked() {
                        action = Some(4);
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(if self.saved_settings.company_name.is_empty() {
                                crate::APP_NAME
                            } else {
                                &self.saved_settings.company_name
                            })
                            .size(12.0)
                            .color(theme::MUTED),
                        )
                        .truncate(),
                    );
                });
            });
        });
        match action {
            Some(17) => {
                crate::auth::clear();
                self.users_ui = Default::default();
                ctx.request_repaint();
            }
            Some(16) => self.navigate(17),
            Some(15) => self.open_reconciliation(),
            Some(14) => self.open_accounting_checks(),
            Some(13) => self.open_invoice_template(),
            Some(11) => {
                self.navigate(15);
            }
            Some(12) => {
                self.navigate(14);
            }
            Some(9) => {
                self.navigate(12);
            }
            Some(10) => {
                self.navigate(11);
            }
            Some(8) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Verify Backup")
                    .add_filter("Axibiz Desktop Beta database", &["db"])
                    .pick_file()
                {
                    match crate::backup::verify(&path) {
                        Ok(()) => {
                            self.message =
                                "备份验证通过：可在临时数据库中升级并恢复，当前数据未变更".into();
                            self.error = false;
                        }
                        Err(e) => {
                            self.message = format!("备份验证失败：{e:#}");
                            self.error = true;
                        }
                    }
                }
            }
            Some(0) => {
                let filename = format!(
                    "Axibiz-{}.db",
                    chrono::Utc::now()
                        .with_timezone(&chrono_tz::America::Vancouver)
                        .format("%Y%m%d-%H%M%S")
                );
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Backup Axibiz Desktop Beta")
                    .set_directory(self.software.saved.backup_root(&self.path))
                    .add_filter("Axibiz Desktop Beta database", &["db"])
                    .set_file_name(filename)
                    .save_file()
                {
                    match crate::backup::save(&self.db, &path) {
                        Ok(()) => {
                            self.message = format!("备份已保存：{}", path.display());
                            self.error = false;
                        }
                        Err(e) => {
                            self.message = format!("备份失败：{e:#}");
                            self.error = true;
                        }
                    }
                }
            }
            Some(1) if self.has_business_editors() => {
                self.message = "请先保存并关闭所有业务录入窗口，再恢复备份。".into();
                self.error = true;
            }
            Some(1) => {
                if let Some(source) = rfd::FileDialog::new()
                    .set_title("Restore Axibiz Desktop Beta")
                    .add_filter("Axibiz Desktop Beta database", &["db", "sqlite", "sqlite3"])
                    .pick_file()
                {
                    self.menus.restore = Some(source);
                    self.menus.restore_focus = true;
                }
            }
            Some(2) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Some(3) => self.navigate(5),
            Some(4) => self.menus.about = true,
            Some(5) => self.navigate(4),
            Some(6) => self.navigate(9),
            Some(7) => {
                self.navigate(10);
            }
            _ => {}
        }
    }
    fn restore_review(&mut self, ctx: &egui::Context) {
        let Some(source) = self.menus.restore.take() else {
            return;
        };
        let values = [
            ("Backup", source.display().to_string()),
            ("Current company", self.saved_settings.company_name.clone()),
        ];
        let blocked = self.has_business_editors();
        match controls::review(ctx,controls::Review{ cancel: "Cancel Restore",id:"restore_review",title:"Restore Company Backup",values:&values,error:blocked.then_some("Save or discard open business/settings drafts before restoring."),confirm:"Confirm & Restore",notice:"Replaces current company data after validation. A safety backup is created first."},&mut self.menus.restore_focus) {
            Some(controls::ReviewAction::Keep)=>{},
            Some(controls::ReviewAction::Confirm)=>{
                let result=(||->Result<PathBuf>{
                    anyhow::ensure!(!self.has_business_editors(),"Open drafts block restore.");
                    let folder=self.software.saved.backup_root(&self.path);std::fs::create_dir_all(&folder)?;
                    let safety=folder.join(format!("Axibiz-before-restore-{}.db",chrono::Utc::now().format("%Y%m%d-%H%M%S-%f")));
                    crate::backup::restore(&mut self.db,&source,&safety)?;self.reset_after_restore()?;egui::Popup::close_all(ctx);Ok(safety)
                })();
                match result {Ok(path)=>{self.message=format!("Restored. Safety backup: {}",path.display());self.error=false},Err(e)=>{self.message=format!("Restore failed: {e:#}");self.error=true;}}
            },
            None=>self.menus.restore=Some(source),
        }
    }
    pub(super) fn about_dialog(&mut self, ctx: &egui::Context) {
        self.restore_review(ctx);
        self.legacy_currency_dialog(ctx);
        let mut open = self.menus.about;
        if open {
            egui::Window::new("About Axibiz Desktop Beta")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading(crate::APP_NAME);
                    ui.label(concat!("Version ", env!("CARGO_PKG_VERSION")));
                    ui.label("Task、Invoice、客户、供应商、费用、销售税与 GL 总账管理。");
                    ui.separator();
                    ui.label(format!("当前数据库：{}", self.path.display()));
                    ui.label("数据保存在本地 SQLite 数据库。");
                });
        }
        self.menus.about = open;
    }
}
impl Desktop {
    pub(super) fn review_legacy_currency(&mut self) {
        self.menus.legacy_currency = Some(self.target.clone());
        self.menus.legacy_focus = true;
    }
    fn legacy_currency_dialog(&mut self, ctx: &egui::Context) {
        let Some(currency) = self.menus.legacy_currency.clone() else {
            return;
        };
        let values = [("Currency", currency.clone())];
        let blocked = self.has_business_editors();
        match controls::review(ctx, controls::Review { cancel: "Cancel", id:"legacy_currency_review",title:"Label Legacy Currency",values:&values,error:blocked.then_some("Save or discard open business/settings drafts first."),confirm:"Confirm & Apply",notice:"Applies to all legacy records with an unspecified currency. Amounts are not converted. Continue only if all affected records originally used this currency." }, &mut self.menus.legacy_focus) {
            Some(controls::ReviewAction::Keep)=>self.menus.legacy_currency=None,
            Some(controls::ReviewAction::Confirm) if !self.has_business_editors() && self.action(Command::LegacyCurrency {currency}) => self.menus.legacy_currency=None,
            _=>{}
        }
    }
}
