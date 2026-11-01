use super::*;
use anyhow::Context;
use zeroize::Zeroize;

#[derive(Default)]
pub(super) struct Login {
    username: String,
    password: String,
    error: String,
}

impl Drop for Login {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
impl Login {
    pub(super) fn show(&mut self, ctx: &egui::Context, c: &Connection) -> bool {
        let mut success = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(24))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.12).max(8.0));
                    ui.heading("Sign In");
                    let company = c
                        .query_row("SELECT company_name FROM settings WHERE id=1", [], |r| {
                            r.get::<_, String>(0)
                        })
                        .unwrap_or_default();
                    ui.label(if company.is_empty() {
                        crate::APP_NAME
                    } else {
                        &company
                    });
                    ui.add_space(16.0);
                    ui.scope(|ui| {
                        ui.set_max_width(380.0);
                        people_ui::section_frame().show(ui, |ui| {
                            ui.set_width(340.0_f32.min(ui.available_width()));
                            ui.label("Username");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.username)
                                    .char_limit(64)
                                    .desired_width(ui.available_width()),
                            );
                            ui.label("Password");
                            let password = ui.add(
                                egui::TextEdit::singleline(&mut self.password)
                                    .password(true)
                                    .char_limit(128)
                                    .desired_width(ui.available_width()),
                            );
                            controls::error(ui, "login_error", &self.error);
                            let enter = password.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let submit =
                                controls::primary(ui, "Sign In", !self.username.trim().is_empty());
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("login_submit_rect"), submit.rect)
                            });
                            if submit.clicked() || enter {
                                match crate::auth::sign_in(c, &self.username, &self.password) {
                                    Ok(()) => {
                                        self.password.zeroize();
                                        self.password.clear();
                                        self.error.clear();
                                        success = true;
                                    }
                                    Err(e) => {
                                        self.password.zeroize();
                                        self.password.clear();
                                        self.error = e.to_string();
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(12.0);
                    ui.small("User accounts belong to this company. Close this window to cancel.");
                });
            });
        success
    }
}

pub(super) struct Shell {
    db: Option<Connection>,
    path: PathBuf,
    login: Login,
    desktop: Option<Desktop>,
}
impl Shell {
    pub(super) fn new(db: Connection, path: PathBuf) -> Result<Self> {
        let mut shell = Self {
            db: Some(db),
            path,
            login: Default::default(),
            desktop: None,
        };
        if !crate::auth::required(shell.db.as_ref().unwrap())? {
            crate::auth::clear();
            shell.open()?;
        }
        Ok(shell)
    }
    fn open(&mut self) -> Result<()> {
        let c = self.db.as_ref().context("Company connection is missing")?;
        crate::prepare_company(c, &self.path)?;
        self.desktop = Some(Desktop::new(self.db.take().unwrap(), self.path.clone())?);
        Ok(())
    }
    pub(super) fn start(&mut self, ctx: &egui::Context) {
        if let Some(app) = self.desktop.as_mut() {
            app.start_fx_sync(ctx);
        }
    }
}
impl eframe::App for Shell {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(app) = self.desktop.as_mut() {
            eframe::App::update(app, ctx, frame);
            return;
        }
        if let Some(c) = self.db.as_ref() {
            if self.login.show(ctx, c) {
                match self.open() {
                    Ok(()) => self.start(ctx),
                    Err(e) => self.login.error = format!("Unable to open company: {e:#}"),
                }
                ctx.request_repaint();
            }
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Unable to open company");
                ui.label(&self.login.error);
                ui.label("Close the application and correct the reported issue before reopening.");
            });
        }
    }
}

#[derive(Default)]
pub(super) struct UsersUi {
    form: Option<Edit>,
    pub(super) login: Login,
    error: String,
}
struct Edit {
    original: crate::auth::User,
    display_name: String,
    username: String,
    active: bool,
    admin: bool,
    enabled: bool,
    original_enabled: bool,
    current_password: String,
    password: String,
    confirmation: String,
    remove_password: bool,
}
impl Drop for Edit {
    fn drop(&mut self) {
        self.current_password.zeroize();
        self.password.zeroize();
        self.confirmation.zeroize();
    }
}
impl Edit {
    fn from(user: crate::auth::User, enabled: bool) -> Self {
        Self {
            display_name: user.display_name.clone(),
            username: user.username.clone(),
            active: user.active,
            admin: user.admin,
            original: user,
            enabled,
            original_enabled: enabled,
            current_password: String::new(),
            password: String::new(),
            confirmation: String::new(),
            remove_password: false,
        }
    }
    fn dirty(&self) -> bool {
        self.username != self.original.username
            || self.display_name != self.original.display_name
            || self.active != self.original.active
            || self.admin != self.original.admin
            || self.enabled != self.original_enabled
            || !self.current_password.is_empty()
            || !self.password.is_empty()
            || !self.confirmation.is_empty()
            || self.remove_password
            || self.original.id == 0
    }
}
impl UsersUi {
    pub(super) fn dirty(&self) -> bool {
        self.form.as_ref().is_some_and(Edit::dirty)
    }
}
fn password(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.label(label);
    ui.add_sized(
        [ui.available_width().min(400.0), theme::CONTROL_HEIGHT],
        egui::TextEdit::singleline(text)
            .password(true)
            .char_limit(128),
    );
}
impl Desktop {
    pub(super) fn users_page(&mut self, ui: &mut egui::Ui) {
        let result = (|| -> Result<_> {
            Ok((
                crate::auth::current(&self.db)?,
                crate::auth::users(&self.db)?,
                crate::auth::required(&self.db)?,
            ))
        })();
        let (who, users, enabled) = match result {
            Ok(v) => v,
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
                return;
            }
        };
        if self.users_ui.form.is_none() {
            self.users_ui.form = Some(Edit::from(who.clone(), enabled));
        }
        let dirty = self.users_ui.dirty();
        let blocked = self.has_other_editors();
        let height = (ui.available_height() - 140.0).max(80.0);
        egui::ScrollArea::vertical().id_salt("company_users_body").max_height(height).auto_shrink([false,false]).show(ui,|ui| {
            people_ui::section_frame().show(ui,|ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Company Access");
                ui.label(format!("Current user: {} ({})",who.display_name,if who.admin {"Administrator"} else {"User"}));
                ui.small("Accounts and passwords are saved in this company's database and travel with its backups.");
                if who.admin {
                    ui.add_enabled_ui(!dirty,|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let selected=self.users_ui.form.as_ref().map(|f|f.original.id).unwrap_or(who.id);
                            egui::ComboBox::from_id_salt("company_user_selection").selected_text(self.users_ui.form.as_ref().map(|f|f.username.as_str()).unwrap_or("")).show_ui(ui,|ui| {
                                for user in &users {
                                    if ui.selectable_label(selected==user.id,format!("{} - {}{}",user.username,user.display_name,if user.active {""}else{" (inactive)"})).clicked() {
                                        self.users_ui.form=Some(Edit::from(user.clone(),enabled));
                                    }
                                }
                            });
                            if ui.button("New User").clicked() { self.users_ui.form=Some(Edit::from(crate::auth::User{id:0,username:String::new(),display_name:String::new(),admin:false,active:true,has_password:false},enabled)); }
                        });
                    });
                }
                let form=self.users_ui.form.as_mut().unwrap();
                ui.add_enabled_ui(who.admin,|ui| {ui.checkbox(&mut form.enabled,"Require password sign-in for this company");});
                ui.small("When enabled, every active user needs a password. Leave sign-in off to open this company directly.");
            });
            ui.add_space(8.0);
            people_ui::section_frame().show(ui,|ui| {
                ui.set_min_width(ui.available_width());
                let form=self.users_ui.form.as_mut().unwrap();
                ui.strong(if form.original.id==0 {"New User"}else{"User Details"});
                ui.small(if form.original.has_password {"Password: set"}else{"Password: not set"});
                ui.add_enabled_ui(who.admin,|ui|field(ui,"Username",&mut form.username));
                field(ui,"Display name",&mut form.display_name);
                ui.add_enabled_ui(who.admin,|ui| {
                    ui.horizontal(|ui| {ui.checkbox(&mut form.active,"Active");ui.checkbox(&mut form.admin,"Administrator");});
                });
                ui.small("Administrators manage users and company sign-in. Business features are shared; these roles do not define accounting approval permissions.");
                password(ui,&format!("Your current password ({})",who.username),&mut form.current_password);
                if !who.has_password {ui.small("No current password is set; leave this field empty.");}
                password(ui,"New password (optional; leave blank to keep saved password)",&mut form.password);
                password(ui,"Confirm new password",&mut form.confirmation);
                ui.small("Use 12–128 characters. Passwords are never displayed or stored as plain text.");
                if form.original.has_password {ui.add_enabled_ui(who.admin && !form.enabled,|ui|{ui.checkbox(&mut form.remove_password,"Remove this user's saved password");});}
            });
            ui.add_space(8.0);
            ui.small("Sign-in protects access through this application. Database and backup files remain unencrypted; retain their OS access protection.");
        });
        controls::error(ui, "company_user_error", &self.users_ui.error);
        controls::footer(
            ui,
            Some(if blocked {
                "Save or close other business/settings drafts before changing users."
            } else {
                "Confirm with your current password. Changes apply only to this company."
            }),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let save = controls::primary(ui, "Save User", dirty && !blocked);
                    #[cfg(test)]
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("users_save_rect"), save.rect));
                    if save.clicked() {
                        self.save_company_user();
                    }
                    ui.add_enabled_ui(dirty, |ui| {
                        ui.menu_button("Revert Changes", |ui| {
                            ui.label("Discard unsaved user changes?");
                            if ui.button("Discard Changes").clicked() {
                                self.users_ui = Default::default();
                                ui.close();
                            }
                        });
                    });
                });
            },
        );
    }
    fn save_company_user(&mut self) {
        if self.has_other_editors() {
            self.users_ui.error = "Save or close other drafts first.".into();
            return;
        }
        let Some(form) = self.users_ui.form.as_mut() else {
            return;
        };
        if form.password != form.confirmation {
            self.users_ui.error = "New passwords do not match.".into();
            return;
        }
        let result = crate::auth::save_user(
            &self.db,
            &form.current_password,
            crate::auth::UserChange {
                id: form.original.id,
                username: &form.username,
                display_name: &form.display_name,
                admin: form.admin,
                active: form.active,
                password: (!form.password.is_empty()).then_some(form.password.as_str()),
                remove_password: form.remove_password,
                require_login: form.enabled,
            },
        );
        match result {
            Ok(()) => {
                self.users_ui = Default::default();
                self.error = false;
                self.message = "Company user settings saved.".into();
            }
            Err(e) => {
                form.current_password.zeroize();
                form.current_password.clear();
                self.users_ui.error = e.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "test-only owner phrase 123";
    fn protected() -> Result<Connection> {
        let c = Connection::open_in_memory()?;
        crate::init(&c)?;
        crate::auth::clear();
        crate::auth::save_user(
            &c,
            "",
            crate::auth::UserChange {
                id: 1,
                username: "owner",
                display_name: "Company Owner",
                admin: true,
                active: true,
                password: Some(PASSWORD),
                remove_password: false,
                require_login: true,
            },
        )?;
        Ok(c)
    }
    #[test]
    fn startup_waits_for_login_and_company_screens_remain_hidden_until_authenticated() -> Result<()>
    {
        let c = protected()?;
        crate::auth::clear();
        if let Some(path) = std::env::var_os("AXIBIZ_AUTH_TEST_FIXTURE") {
            crate::backup::save(&c, &PathBuf::from(path))?;
        }
        let shell = Shell::new(c, PathBuf::from("protected-startup-test.db"))?;
        assert!(shell.desktop.is_none());
        assert!(shell.db.is_some());
        drop(shell);
        let c = protected()?;
        crate::auth::clear();
        let mut app = Desktop::new(c, PathBuf::from("protected-preview-test.db"))?;
        let before = app.db.total_changes();
        for (name, width, height) in [
            ("company-login", 1440.0, 900.0),
            ("company-login-small", 900.0, 650.0),
        ] {
            super::super::design_preview::capture(
                &mut app,
                name,
                width,
                height,
                false,
                Desktop::draw,
            )?;
        }
        assert_eq!(app.db.total_changes(), before);
        crate::auth::sign_in(&app.db, "owner", PASSWORD)?;
        app.navigate(17);
        let before = app.db.total_changes();
        for (name, width, height) in [
            ("company-users", 1440.0, 900.0),
            ("company-users-small", 900.0, 650.0),
        ] {
            super::super::design_preview::capture(
                &mut app,
                name,
                width,
                height,
                false,
                Desktop::draw,
            )?;
        }
        assert_eq!(app.db.total_changes(), before);
        app.users_ui.form.as_mut().unwrap().password = "a changed draft password".into();
        app.users_ui.form.as_mut().unwrap().confirmation = "different".into();
        app.save_company_user();
        assert!(app.users_ui.error.contains("do not match"));
        assert!(app.has_business_editors());
        app.navigate(0);
        app.navigate(17);
        assert_eq!(
            app.users_ui.form.as_ref().unwrap().password,
            "a changed draft password"
        );
        assert_eq!(app.db.total_changes(), before);
        crate::auth::clear();
        Ok(())
    }
}
