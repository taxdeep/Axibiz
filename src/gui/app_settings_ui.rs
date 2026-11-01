use super::*;
use crate::app_settings::{Clock, Preferences, Zone};

pub(super) struct SoftwareUi {
    pub(super) draft: Preferences,
    pub(super) saved: Preferences,
    path: PathBuf,
}

impl SoftwareUi {
    fn dirty(&self) -> bool {
        self.draft != self.saved
    }
    pub(super) fn load(database: &std::path::Path) -> Result<Self> {
        #[cfg(test)]
        let path = database.with_extension("software-test.json");
        #[cfg(not(test))]
        let path = {
            let _ = database;
            crate::app_settings::path()?
        };
        let saved = Preferences::load(&path)?;
        Ok(Self {
            draft: saved.clone(),
            saved,
            path,
        })
    }
}
impl Desktop {
    pub(super) fn software_settings_dirty(&self) -> bool {
        self.software.dirty()
    }
    pub(super) fn software_settings_page(&mut self, ui: &mut egui::Ui) {
        let dirty = self.software.dirty();
        let available = (ui.available_height() - 88.0).max(80.0);
        egui::ScrollArea::vertical().id_salt("software_settings_body").max_height(available).auto_shrink([false, false]).show(ui, |ui| {
            people_ui::section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Backups");
                ui.checkbox(&mut self.software.draft.automatic_backup, "Create a daily backup on startup");
                ui.label("Backup directory");
                ui.horizontal(|ui| {
                    ui.add_sized([(ui.available_width() - 85.0).max(100.0), theme::CONTROL_HEIGHT], egui::TextEdit::singleline(&mut self.software.draft.backup_directory).hint_text("Default: backups beside each company database"));
                    if ui.button("Browse...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().set_title("Choose Backup Directory").pick_folder() {
                            self.software.draft.backup_directory = folder.to_string_lossy().into_owned();
                        }
                    }
                });
                if ui.button("Use Default Directory").clicked() { self.software.draft.backup_directory.clear(); }
                ui.small(format!("Backup root for this company: {}", self.software.draft.backup_root(&self.path).display()));
                ui.small("Each company uses its own subfolder. Changing this directory does not move or remove existing backups.");
                ui.horizontal(|ui| {
                    ui.label("Daily backups to keep");
                    ui.add(egui::DragValue::new(&mut self.software.draft.daily_backups_kept).range(1..=365));
                });
                ui.small("Automatic backups run at startup, once per day. Settings apply on the next startup. Required upgrade backups are always created; manual and restore backups are retained.");
            });
            ui.add_space(8.0);
            people_ui::section_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Time Display");
                ui.horizontal(|ui| {
                    ui.label("Clock format");
                    egui::ComboBox::from_id_salt("software_clock").selected_text(match self.software.draft.clock { Clock::TwentyFour => "24-hour", Clock::Twelve => "12-hour (AM/PM)" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.software.draft.clock, Clock::TwentyFour, "24-hour");
                        ui.selectable_value(&mut self.software.draft.clock, Clock::Twelve, "12-hour (AM/PM)");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Display time zone");
                    egui::ComboBox::from_id_salt("software_zone").selected_text(match self.software.draft.zone { Zone::Vancouver => "America/Vancouver", Zone::System => "System local time", Zone::Utc => "UTC" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.software.draft.zone, Zone::Vancouver, "America/Vancouver");
                        ui.selectable_value(&mut self.software.draft.zone, Zone::System, "System local time");
                        ui.selectable_value(&mut self.software.draft.zone, Zone::Utc, "UTC");
                    });
                });
                ui.label(format!("Preview: {}", self.software.draft.display_time(&chrono::Utc::now().to_rfc3339())));
                ui.small("Applies to displayed event timestamps, including the Audit Log. Stored timestamps and business document dates are unchanged.");
            });
            ui.add_space(8.0);
            ui.small(format!("Software preferences: {}", self.software.path.display()));
            ui.small("These preferences apply to this computer user across companies. Company backups and restores do not replace them. Company information is under Setup > Company Settings.");
        });
        controls::footer(
            ui,
            Some(if dirty {
                "Unsaved software preferences"
            } else {
                "Software preferences are saved separately from company data."
            }),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let save = controls::primary(ui, "Save", dirty);
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("software_save_rect"), save.rect)
                    });
                    if save.clicked() {
                        self.save_software_settings(false);
                    }
                    if ui.button("Save & Close").clicked() {
                        self.save_software_settings(true);
                    }
                    ui.add_enabled_ui(dirty, |ui| {
                        ui.menu_button("Revert Changes", |ui| {
                            ui.label("Discard unsaved software preferences?");
                            if ui.button("Discard Changes").clicked() {
                                self.software.draft = self.software.saved.clone();
                                ui.close();
                            }
                        });
                    });
                });
            },
        );
    }
    fn save_software_settings(&mut self, close: bool) {
        match self.software.draft.save(&self.software.path) {
            Ok(saved) => {
                self.software.draft = saved.clone();
                self.software.saved = saved;
                self.error = false;
                self.message =
                    "Software preferences saved. Backup preferences apply on the next startup."
                        .into();
                if close {
                    self.navigate(0);
                }
            }
            Err(e) => {
                self.error = true;
                self.message = format!("Unable to save software preferences: {e:#}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn software_route_save_and_failed_draft_are_independent_of_company_and_restore() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "axibiz-software-ui-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&root)?;
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, root.join("company.db"))?;
        let company = app.saved_settings.clone();
        let changes = app.db.total_changes();
        app.navigate(16);
        app.software.draft.clock = Clock::Twelve;
        app.software.draft.backup_directory = "relative".into();
        app.save_software_settings(true);
        assert!(app.error);
        assert_eq!(app.page, 16);
        assert_eq!(app.software.draft.clock, Clock::Twelve);
        assert!(app.has_business_editors());
        app.navigate(5);
        assert_eq!(app.settings, company);
        app.navigate(16);
        app.software.draft.backup_directory =
            root.join("backup-target").to_string_lossy().into_owned();
        app.save_software_settings(false);
        assert!(!app.error, "{}", app.message);
        assert_eq!(Preferences::load(&app.software.path)?, app.software.saved);
        assert!(!app.has_business_editors());
        let preferences = app.software.saved.clone();
        app.reset_after_restore()?;
        assert_eq!(app.software.saved, preferences);
        assert_eq!(app.software.draft, preferences);
        assert_eq!(app.settings, company);
        assert_eq!(app.db.total_changes(), changes);
        drop(app);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
