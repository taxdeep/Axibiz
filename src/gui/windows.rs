use super::*;

pub(super) struct SurfaceResponse {
    pub is_top_modal: bool,
    pub any_popup_open: bool,
}

// Native business windows own the whole client area. Nested pickers remain local modals.
pub(super) fn surface(
    ctx: &egui::Context,
    id: egui::Id,
    draw: impl FnOnce(&mut egui::Ui),
) -> SurfaceResponse {
    let draw = |ui: &mut egui::Ui| {
        theme::business_typography(ui);
        draw(ui);
    };
    if ctx.viewport_id() == egui::ViewportId::ROOT {
        let response = egui::Modal::new(id).show(ctx, draw);
        return SurfaceResponse {
            is_top_modal: response.is_top_modal,
            any_popup_open: response.any_popup_open,
        };
    }
    let any_popup_open = egui::Popup::is_any_open(ctx);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(16))
        .show(ctx, |ui| {
            ui.push_id(id, draw);
        });
    SurfaceResponse {
        is_top_modal: ctx.memory(|m| m.top_modal_layer().is_none()),
        any_popup_open,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Task,
    Invoice,
    Accounting,
    Journal,
    Ledger,
    Reconciliation,
    Checks,
    Payment,
    Customer,
    Vendor,
    PayBills,
}
impl Kind {
    const ALL: [Self; 11] = [
        Self::Task,
        Self::Invoice,
        Self::Accounting,
        Self::Journal,
        Self::Ledger,
        Self::Reconciliation,
        Self::Checks,
        Self::Payment,
        Self::Customer,
        Self::Vendor,
        Self::PayBills,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Task => "Task",
            Self::Invoice => "Invoice",
            Self::Accounting => "Bill / Account",
            Self::Journal => "Journal Entry",
            Self::Ledger => "General Ledger / Trial Balance",
            Self::Reconciliation => "Reconciliation",
            Self::Checks => "Accounting Checks",
            Self::Payment => "Receive Payment",
            Self::Customer => "Customer",
            Self::Vendor => "Vendor",
            Self::PayBills => "Pay Bills",
        }
    }
    fn id(self) -> egui::ViewportId {
        egui::ViewportId::from_hash_of(format!("axibiz-business-{self:?}"))
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Placement {
    position: [f32; 2],
    size: [f32; 2],
    scale: f32,
}
impl Placement {
    fn usable(&self) -> bool {
        self.position
            .iter()
            .chain(self.size.iter())
            .all(|v| v.is_finite())
            && self.size[0] >= 640.0
            && self.size[1] >= 480.0
            && self.size[0] <= 10000.0
            && self.size[1] <= 10000.0
            && self.position.iter().all(|v| v.abs() < 100000.0)
            && self.scale.is_finite()
            && self.scale > 0.0
            && self.scale < 10.0
    }
    fn visible(&self) -> bool {
        if !self.usable() {
            return false;
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::RECT,
                Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL},
            };
            // Require the title bar to intersect an attached display, including displays at negative coordinates.
            let x = (self.position[0] * self.scale) as i32;
            let y = (self.position[1] * self.scale) as i32;
            let rect = RECT {
                left: x,
                top: y,
                right: x + (self.size[0] * self.scale) as i32,
                bottom: y + 30,
            };
            unsafe { !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_null() }
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}
#[derive(Default)]
pub(super) struct Windows {
    placements: std::collections::BTreeMap<String, Placement>,
    // These are creation hints, not a mirror of the OS geometry during a drag.
    builders: std::collections::BTreeMap<String, egui::ViewportBuilder>,
    layout_changed: Option<std::time::Instant>,
    pub(super) ledger: bool,
    close: Option<Kind>,
    close_focus: bool,
    exit: bool,
    exit_focus: bool,
    allow_exit: bool,
    saved_layout: Vec<u8>,
    changes: u64,
}
impl Windows {
    pub(super) fn load(path: &std::path::Path) -> Self {
        let placements = std::fs::read(path.with_extension("windows.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            placements,
            ..Self::default()
        }
    }

    fn builder(
        &mut self,
        key: &str,
        title: String,
        home: egui::Pos2,
        initial: [f32; 2],
    ) -> egui::ViewportBuilder {
        self.builders
            .entry(key.into())
            .or_insert_with(|| {
                let mut builder = egui::ViewportBuilder::default()
                    .with_inner_size(initial)
                    .with_position(home)
                    .with_min_inner_size([640.0, 480.0]);
                // Restore or recover saved off-screen geometry once, when this window opens.
                if let Some(p) = self.placements.get(key).filter(|p| p.visible()) {
                    builder = builder.with_position(p.position).with_inner_size(p.size);
                }
                builder
            })
            .clone()
            .with_title(title)
    }

    fn observe(&mut self, key: &str, viewport: &egui::ViewportInfo, scale: f32) {
        if viewport.maximized == Some(true) || viewport.minimized == Some(true) {
            return;
        }
        if let (Some(outer), Some(inner)) = (viewport.outer_rect, viewport.inner_rect) {
            let placement = Placement {
                position: [outer.min.x, outer.min.y],
                size: [inner.width(), inner.height()],
                scale,
            };
            if placement.usable() && self.placements.get(key) != Some(&placement) {
                self.placements.insert(key.into(), placement);
                self.layout_changed = Some(std::time::Instant::now());
            }
        }
    }

    pub(super) fn reset_runtime(&mut self) {
        self.builders.clear();
        self.close = None;
        self.exit = false;
        self.allow_exit = false;
    }
}
impl Desktop {
    fn save_window_layout(&mut self, ctx: &egui::Context, force: bool) {
        const SAVE_DELAY: std::time::Duration = std::time::Duration::from_millis(400);
        let Some(changed) = self.windows.layout_changed else {
            return;
        };
        let elapsed = changed.elapsed();
        if !force && elapsed < SAVE_DELAY {
            ctx.request_repaint_after(SAVE_DELAY - elapsed);
            return;
        }
        // UI preferences are separate from the company database; a partial write is ignored on next launch.
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.windows.placements) {
            if bytes == self.windows.saved_layout || self.windows.placements.is_empty() {
                self.windows.layout_changed = None;
            } else if std::fs::write(self.path.with_extension("windows.json"), &bytes).is_ok() {
                self.windows.saved_layout = bytes;
                self.windows.layout_changed = None;
            } else {
                self.windows.layout_changed = Some(std::time::Instant::now());
                ctx.request_repaint_after(SAVE_DELAY);
            }
        }
    }

    pub(super) fn has_business_editors(&self) -> bool {
        self.has_other_editors() || self.users_ui.dirty()
    }
    pub(super) fn has_other_editors(&self) -> bool {
        Kind::ALL.into_iter().any(|k| {
            !matches!(k, Kind::Ledger | Kind::Reconciliation | Kind::Checks)
                && self.business_active(k)
        }) || self.new_record.is_some()
            || self.price_edit.is_some()
            || self.template_ui.is_open()
            || self.settings != self.saved_settings
            || self.software_settings_dirty()
            || self.inline_settings_dirty()
    }
    fn business_active(&self, kind: Kind) -> bool {
        match kind {
            Kind::Task => self.new_task_open || self.task_editor_open,
            Kind::Invoice => {
                self.direct_invoice.form.is_some()
                    || self.invoice_workbench.open
                    || self.billing_ui.open
            }
            Kind::Accounting => self.accounting.form.is_some(),
            Kind::Journal => self.gl.form.is_some(),
            Kind::Ledger => self.windows.ledger,
            Kind::Reconciliation => self.menus.reconciliation.is_some(),
            Kind::Checks => self.menus.checks.is_some(),
            Kind::Payment => self.payment.is_some(),
            Kind::Customer => self.customer_ui.form.is_some(),
            Kind::Vendor => self.vendor_ui.form.is_some(),
            Kind::PayBills => self.pay_bills.is_some(),
        }
    }
    pub(super) fn focus_business(&self, kind: Kind) {
        if let Some(ctx) = &self.ui_context {
            ctx.send_viewport_cmd_to(kind.id(), egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(kind.id(), egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    }
    // An existing form is never replaced, even if it has not been edited yet.
    pub(super) fn reuse_business(&mut self, kind: Kind) -> bool {
        if !self.business_active(kind) {
            return false;
        }
        self.focus_business(kind);
        self.message = format!(
            "{} is already open. Save and close it, or discard its draft before starting another.",
            kind.title()
        );
        self.error = false;
        true
    }
    pub(super) fn request_business_close(&mut self, kind: Kind) {
        self.windows.close = Some(kind);
        self.windows.close_focus = true;
    }
    fn discard_business(&mut self, kind: Kind) {
        match kind {
            Kind::Task => {
                self.task_review = None;
                self.new_task_open = false;
                self.task_editor_open = false;
                self.lookup_row = None;
                if self.new_item_row.is_some() {
                    self.new_record = None;
                }
                self.new_item_row = None;
            }
            Kind::Invoice => {
                self.direct_invoice = Default::default();
                self.invoice_workbench = Default::default();
                self.billing_ui.open = false;
            }
            Kind::Accounting => self.accounting.form = None,
            Kind::Journal => self.gl.form = None,
            Kind::Ledger => self.windows.ledger = false,
            Kind::Reconciliation => self.menus.reconciliation = None,
            Kind::Checks => self.menus.checks = None,
            Kind::Payment => self.payment = None,
            Kind::Customer => self.customer_ui.form = None,
            Kind::Vendor => self.vendor_ui.form = None,
            Kind::PayBills => self.pay_bills = None,
        }
    }
    fn render_business(&mut self, kind: Kind, ctx: &egui::Context) {
        match kind {
            Kind::Task => {
                self.task_editor_dialog(ctx);
                self.new_task_dialog(ctx);
                self.item_lookup_dialog(ctx);
                if self.new_item_row.is_some() {
                    self.record_dialog(ctx);
                }
            }
            Kind::Invoice => {
                self.direct_invoice_dialog(ctx);
                self.invoice_workbench_dialog(ctx);
                self.invoice_dialog(ctx);
            }
            Kind::Reconciliation => self.reconciliation_dialog(ctx),
            Kind::Checks => self.accounting_checks_dialog(ctx),
            Kind::Accounting => self.accounting_dialog(ctx),
            Kind::Journal => self.journal_editor(ctx),
            Kind::Payment => self.payment_dialog(ctx),
            Kind::Customer => self.customer_dialog(ctx),
            Kind::Vendor => self.vendor_dialog(ctx),
            Kind::PayBills => self.pay_bills_dialog(ctx),
            Kind::Ledger => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    theme::business_typography(ui);
                    egui::ScrollArea::both().show(ui, |ui| self.ledger_page(ui));
                });
            }
        }
    }
    pub(super) fn business_windows(&mut self, ctx: &egui::Context) {
        let mut closed = false;
        for kind in Kind::ALL {
            let key = format!("{kind:?}");
            if !self.business_active(kind) {
                closed |= self.windows.builders.remove(&key).is_some();
                continue;
            }
            let title = format!(
                "{} - {} - {}",
                kind.title(),
                crate::APP_NAME,
                self.settings.company_name
            );
            let monitor = ctx
                .input(|i| i.viewport().monitor_size)
                .unwrap_or(egui::vec2(1400.0, 1000.0));
            let initial = [
                (if kind == Kind::Payment {
                    1000.0_f32
                } else {
                    1200.0
                })
                .min((monitor.x - 80.0).max(640.0)),
                (if kind == Kind::Payment {
                    700.0_f32
                } else {
                    850.0
                })
                .min((monitor.y - 100.0).max(480.0)),
            ];
            let home = ctx
                .input(|i| i.viewport().outer_rect.map(|r| r.min))
                .unwrap_or_default()
                + egui::vec2(30.0, 30.0);
            let builder = self.windows.builder(&key, title, home, initial);
            ctx.show_viewport_immediate(kind.id(), builder, |child, class| {
                if class == egui::ViewportClass::Embedded {
                    if self.windows.close == Some(kind) {
                        self.confirm_business_close(child, kind);
                    } else {
                        self.render_business(kind, child);
                    }
                    return;
                }
                let scale = child.pixels_per_point();
                self.windows
                    .observe(&key, &child.input(|i| i.viewport().clone()), scale);
                let close_shortcut = child.memory(|m| m.top_modal_layer().is_none())
                    && child.input_mut(|i| {
                        i.consume_shortcut(&egui::KeyboardShortcut::new(
                            egui::Modifiers::CTRL,
                            egui::Key::W,
                        ))
                    });
                if child.input(|i| i.viewport().close_requested()) || close_shortcut {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if matches!(kind, Kind::Ledger | Kind::Reconciliation | Kind::Checks) {
                        self.discard_business(kind);
                    } else {
                        self.request_business_close(kind);
                    }
                }
                if self.windows.close == Some(kind) {
                    egui::CentralPanel::default().show(child, |_| {});
                    self.confirm_business_close(child, kind);
                } else {
                    self.render_business(kind, child);
                }
            });
            if !self.business_active(kind) {
                self.windows.builders.remove(&key);
                closed = true;
            }
        }
        let changes = self.db.total_changes();
        if changes != self.windows.changes {
            self.windows.changes = changes;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            for kind in Kind::ALL {
                if self.business_active(kind) {
                    ctx.request_repaint_of(kind.id());
                }
            }
        }
        if !ctx.embed_viewports() {
            self.save_window_layout(ctx, closed || ctx.input(|i| i.viewport().close_requested()));
        }
        // Supporting pickers stay attached to their editor; unrelated dialogs remain in the workspace.
        if self.new_item_row.is_none() {
            self.record_dialog(ctx);
        }
        self.price_edit_dialog(ctx);
        self.invoice_template_dialog(ctx);

        let busy = Kind::ALL.into_iter().any(|k| {
            !matches!(k, Kind::Ledger | Kind::Reconciliation | Kind::Checks)
                && self.business_active(k)
        }) || self.new_record.is_some()
            || self.price_edit.is_some()
            || self.template_ui.is_open()
            || self.settings != self.saved_settings
            || self.inline_settings_dirty()
            || self.pdf_receiver.is_some()
            || self.print_receiver.is_some();
        if ctx.input(|i| i.viewport().close_requested()) && busy && !self.windows.allow_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.windows.exit {
                self.windows.exit_focus = true;
            }
            self.windows.exit = true;
        }
        if self.windows.exit {
            match controls::review(ctx,controls::Review{ cancel: "Keep Working",id:"confirm_application_exit",title:"Close Axibiz Desktop Beta?",values:&[],error:None,confirm:"Discard Changes & Exit",notice:"Unsaved input will be discarded and pending background work will stop. Keep editing to finish your work."},&mut self.windows.exit_focus) {
                Some(controls::ReviewAction::Confirm)=>{self.windows.allow_exit=true;self.windows.exit=false;ctx.send_viewport_cmd(egui::ViewportCommand::Close);},
                Some(controls::ReviewAction::Keep)=>self.windows.exit=false,
                None=>{},
            }
        }
    }
    fn confirm_business_close(&mut self, ctx: &egui::Context, kind: Kind) {
        let dirty = match kind {
            Kind::Vendor => self.vendor_ui.form.as_ref().map(|f| f.dirty()),
            Kind::Customer => self.customer_ui.form.as_ref().map(|f| f.dirty()),
            Kind::Payment => self.payment.as_ref().map(|f| f.dirty()),
            Kind::Accounting => self.accounting.dirty(),
            Kind::Journal => self.gl.form.as_ref().map(|f| f.has_changes()),
            Kind::PayBills => self.pay_bills.as_ref().map(|f| f.dirty()),
            Kind::Invoice if self.direct_invoice.form.is_none() && !self.invoice_workbench.open => {
                Some(false)
            }
            _ => None,
        };
        if dirty == Some(false) {
            self.discard_business(kind);
            self.windows.close = None;
            return;
        }

        egui::Modal::new(egui::Id::new(("business_close", kind.id()))).show(ctx, |ui| {
            theme::business_typography(ui);
            ui.heading("Close this window?");
            ui.label(
                "Unsaved changes will be lost. Return to save your work, or discard the changes.",
            );
            ui.horizontal(|ui| {
                let keep = ui.button("Return to editing");
                if self.windows.close_focus {
                    keep.request_focus();
                    self.windows.close_focus = false;
                }
                if keep.clicked() {
                    self.windows.close = None;
                }
                if dirty.is_some()
                    && (kind != Kind::Journal
                        || self.gl.form.as_ref().is_some_and(|f| f.saveable_draft()))
                    && ui
                        .button(if matches!(kind, Kind::PayBills | Kind::Payment) {
                            "Record & Close"
                        } else if kind == Kind::Accounting
                            && matches!(
                                self.accounting.form,
                                Some(accounting_ui::Form::Expense { .. })
                            )
                        {
                            "Review & Close"
                        } else if kind == Kind::Journal {
                            "Save Draft & Close"
                        } else {
                            "Save & Close"
                        })
                        .clicked()
                {
                    self.windows.close = None;
                    match kind {
                        Kind::Customer => {
                            if let Some(mut f) = self.customer_ui.form.take() {
                                if !self.save_customer_form(&mut f) {
                                    self.customer_ui.form = Some(f)
                                }
                            }
                        }
                        Kind::Journal => {
                            if let Some(mut f) = self.gl.form.take() {
                                if !self.save_journal_form(&mut f, false) {
                                    self.gl.form = Some(f);
                                }
                            }
                        }
                        Kind::Payment => self.review_receipt_close(),
                        Kind::Vendor => {
                            if let Some(mut f) = self.vendor_ui.form.take() {
                                if !self.save_vendor_form(&mut f) {
                                    self.vendor_ui.form = Some(f)
                                }
                            }
                        }
                        Kind::Accounting
                            if matches!(
                                self.accounting.form,
                                Some(accounting_ui::Form::Expense { .. })
                            ) =>
                        {
                            self.review_accounting_close()
                        }
                        Kind::Accounting => {
                            if let Some(mut f) = self.accounting.form.take() {
                                if !self.save_accounting_form(&mut f) {
                                    self.accounting.form = Some(f)
                                }
                            }
                        }
                        Kind::PayBills => self.review_supplier_payment(true),
                        _ => {}
                    }
                }
                if ui.button("Discard and close").clicked() {
                    self.discard_business(kind);
                    self.windows.close = None;
                }
            });
        });
    }
    pub(super) fn window_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.menu_button("Window", |ui| {
            for kind in Kind::ALL {
                if self.business_active(kind) && ui.button(kind.title()).clicked() {
                    self.focus_business(kind);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Move business windows to this monitor").clicked() {
                let pos = ctx
                    .input(|i| i.viewport().outer_rect.map(|r| r.min))
                    .unwrap_or_default();
                for (n, kind) in Kind::ALL
                    .into_iter()
                    .filter(|k| self.business_active(*k))
                    .enumerate()
                {
                    ctx.send_viewport_cmd_to(kind.id(), egui::ViewportCommand::Maximized(false));
                    ctx.send_viewport_cmd_to(kind.id(), egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd_to(
                        kind.id(),
                        egui::ViewportCommand::OuterPosition(
                            pos + egui::vec2(30.0 + n as f32 * 24.0, 30.0 + n as f32 * 24.0),
                        ),
                    );
                }
                ui.close();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct NativeState {
        info: egui::ViewportInfo,
        events: Vec<egui::Event>,
        text: Vec<(String, egui::Pos2)>,
        builders: egui::ViewportIdMap<egui::ViewportBuilder>,
        commands: Vec<egui::ViewportCommand>,
    }

    // Run the same nested immediate-viewport callbacks as eframe, with scripted OS observations.
    fn native_context() -> (egui::Context, Rc<RefCell<NativeState>>) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.set_embed_viewports(false);
        let state = Rc::new(RefCell::new(NativeState::default()));
        let backend = state.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let (info, events) = {
                let mut state = backend.borrow_mut();
                let (commands, recreate) = state
                    .builders
                    .entry(viewport.ids.this)
                    .or_insert_with(|| viewport.builder.clone())
                    .patch(viewport.builder);
                assert!(!recreate);
                state.commands.extend(commands);
                (state.info.clone(), std::mem::take(&mut state.events))
            };
            let mut viewports = ctx.input(|i| i.raw.viewports.clone());
            viewports.insert(viewport.ids.this, info.clone());
            let size = info.inner_rect.unwrap().size();
            let output = ctx.run(
                egui::RawInput {
                    viewport_id: viewport.ids.this,
                    viewports,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |child| (viewport.viewport_ui_cb)(child),
            );
            backend.borrow_mut().text = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        Some((
                            text.galley.job.text.clone(),
                            text.pos + text.galley.size() / 2.0,
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            for viewport in output.viewport_output.values() {
                backend
                    .borrow_mut()
                    .commands
                    .extend(viewport.commands.clone());
            }
        });
        (ctx, state)
    }

    fn native_frame(
        app: &mut Desktop,
        ctx: &egui::Context,
        state: &Rc<RefCell<NativeState>>,
        position: [f32; 2],
        scale: f32,
    ) {
        let origin = egui::pos2(position[0], position[1]);
        let events = state.borrow().info.events.clone();
        state.borrow_mut().info = egui::ViewportInfo {
            parent: Some(egui::ViewportId::ROOT),
            native_pixels_per_point: Some(scale),
            outer_rect: Some(egui::Rect::from_min_size(origin, egui::vec2(1016.0, 739.0))),
            inner_rect: Some(egui::Rect::from_min_size(
                origin + egui::vec2(8.0, 31.0),
                egui::vec2(1000.0, 700.0),
            )),
            minimized: Some(false),
            maximized: Some(false),
            events,
            ..Default::default()
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| app.business_windows(ctx),
        );
        for viewport in output.viewport_output.values() {
            state
                .borrow_mut()
                .commands
                .extend(viewport.commands.clone());
        }
    }

    fn app() -> Result<Desktop> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        Desktop::new(db, PathBuf::from("test.db"))
    }

    fn isolate_layout(app: &mut Desktop) {
        // A nonexistent parent prevents writes to real workspace preferences.
        app.path = std::env::temp_dir()
            .join(format!(
                "axibiz-window-test-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ))
            .join("test.db");
    }

    #[test]
    fn account_native_close_skips_clean_forms_and_saves_changed_input() -> Result<()> {
        let mut app = app()?;
        isolate_layout(&mut app);
        app.page = 7;
        app.open_accounting_form(0);
        let changes = app.db.total_changes();
        let (ctx, state) = native_context();
        app.request_business_close(Kind::Accounting);
        for _ in 0..3 {
            native_frame(&mut app, &ctx, &state, [200.0, 100.0], 1.0);
        }
        assert!(app.accounting.form.is_none());
        assert_eq!(app.db.total_changes(), changes);
        app.open_accounting_form(0);
        if let Some(accounting_ui::Form::Account { code, name, .. }) = app.accounting.form.as_mut()
        {
            *code = "5998".into();
            *name = "Controlled close test".into();
        } else {
            panic!("Expected account editor")
        }
        app.request_business_close(Kind::Accounting);
        for _ in 0..4 {
            native_frame(&mut app, &ctx, &state, [200.0, 100.0], 1.0);
        }
        assert!(app.accounting.form.is_some());
        let save = state
            .borrow()
            .text
            .iter()
            .find(|(s, _)| s == "Save & Close")
            .unwrap()
            .1;
        for pressed in [true, false] {
            state.borrow_mut().events = vec![
                egui::Event::PointerMoved(save),
                egui::Event::PointerButton {
                    pos: save,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            native_frame(&mut app, &ctx, &state, [200.0, 100.0], 1.0);
        }
        assert!(app.accounting.form.is_none());
        assert_eq!(
            app.db.query_row(
                "SELECT count(*) FROM accounts WHERE code='5998' AND name='Controlled close test'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    }

    #[test]
    fn native_drag_observations_do_not_issue_position_or_size_commands() -> Result<()> {
        let mut app = app()?;
        isolate_layout(&mut app);
        app.open_pay_bills(0, None);
        app.open_customer_editor(0);
        app.customer_ui.form.as_mut().unwrap().customer.name = "Keep unsaved customer".into();
        app.open_vendor_editor(0);
        app.vendor_ui.form.as_mut().unwrap().vendor.name = "Keep unsaved vendor".into();
        let changes = app.db.total_changes();
        let (ctx, state) = native_context();
        native_frame(&mut app, &ctx, &state, [100.0, 100.0], 1.0);
        state.borrow_mut().commands.clear();
        for (position, scale) in [
            ([240.0, 150.0], 1.0),
            ([320.0, 200.0], 1.0),
            ([-1200.0, -100.0], 1.5),
            ([-9000.0, -9000.0], 1.5),
            ([400.0, 250.0], 1.0),
        ] {
            native_frame(&mut app, &ctx, &state, position, scale);
            let observed = &app.windows.placements["Customer"];
            assert_eq!(observed.position, position);
            assert_eq!(observed.scale, scale);
            assert_eq!(app.windows.placements["Vendor"].position, position);
            assert_eq!(app.windows.placements["PayBills"].position, position);
            assert!(
                state.borrow().commands.iter().all(|command| !matches!(
                    command,
                    egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
                )),
                "Dragging must not reapply stale geometry: {:?}",
                state.borrow().commands
            );
            state.borrow_mut().commands.clear();
        }
        assert_eq!(
            app.customer_ui.form.as_ref().unwrap().customer.name,
            "Keep unsaved customer"
        );
        assert_eq!(app.db.total_changes(), changes);
        assert_eq!(
            app.vendor_ui.form.as_ref().unwrap().vendor.name,
            "Keep unsaved vendor"
        );
        let saved = app.windows.placements["Customer"].clone();
        app.discard_business(Kind::Customer);
        native_frame(&mut app, &ctx, &state, saved.position, saved.scale);
        assert!(!app.windows.builders.contains_key("Customer"));
        app.open_customer_editor(0);
        native_frame(&mut app, &ctx, &state, saved.position, saved.scale);
        if saved.visible() {
            assert_eq!(
                app.windows.builders["Customer"].position,
                Some(egui::pos2(saved.position[0], saved.position[1]))
            );
        }
        Ok(())
    }

    #[test]
    fn window_layout_saves_after_motion_settles_and_flushes_on_close() -> Result<()> {
        let mut app = app()?;
        app.path = std::env::temp_dir().join(format!(
            "axibiz-layout-save-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let path = app.path.with_extension("windows.json");
        let ctx = egui::Context::default();
        let mut info = egui::ViewportInfo {
            outer_rect: Some(egui::Rect::from_min_size(
                egui::pos2(100.0, 100.0),
                egui::vec2(916.0, 689.0),
            )),
            inner_rect: Some(egui::Rect::from_min_size(
                egui::pos2(108.0, 131.0),
                egui::vec2(900.0, 650.0),
            )),
            ..Default::default()
        };
        app.windows.observe("Payment", &info, 1.5);
        app.save_window_layout(&ctx, false);
        assert!(
            !path.exists(),
            "Dragging should not synchronously rewrite layout preferences"
        );
        info.outer_rect.as_mut().unwrap().min = egui::pos2(200.0, 150.0);
        app.windows.observe("Payment", &info, 1.5);
        app.save_window_layout(&ctx, false);
        assert!(!path.exists());
        app.windows.layout_changed =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        app.save_window_layout(&ctx, false);
        let saved: std::collections::BTreeMap<String, Placement> =
            serde_json::from_slice(&std::fs::read(&path)?)?;
        assert_eq!(saved["Payment"].position, [200.0, 150.0]);
        assert_eq!(saved["Payment"].scale, 1.5);
        let normal = app.windows.placements["Payment"].clone();
        info.outer_rect.as_mut().unwrap().min = egui::pos2(300.0, 200.0);
        info.maximized = Some(true);
        app.windows.observe("Payment", &info, 1.5);
        assert_eq!(app.windows.placements["Payment"], normal);
        info.maximized = Some(false);
        info.minimized = Some(true);
        app.windows.observe("Payment", &info, 1.5);
        assert_eq!(app.windows.placements["Payment"], normal);
        info.minimized = Some(false);
        app.windows.observe("Payment", &info, 1.5);
        app.save_window_layout(&ctx, true);
        let saved: std::collections::BTreeMap<String, Placement> =
            serde_json::from_slice(&std::fs::read(&path)?)?;
        assert_eq!(saved["Payment"].position, [300.0, 200.0]);
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn native_payment_close_can_return_to_the_draft_or_discard_without_recording_payment(
    ) -> Result<()> {
        let mut app = app()?;
        isolate_layout(&mut app);
        let customer = crate::customer::save(
            &mut app.db,
            &crate::customer::Customer {
                name: "Buyer".into(),
                ..Default::default()
            },
        )?;
        let draft = crate::billing::DirectInvoice {
            request_key: "native-payment-close".into(),
            customer,
            date: crate::today().to_string(),
            currency: "CAD".into(),
            title: "Service".into(),
            lines: vec![crate::billing::DirectLine {
                price_id: 0,
                description: "Service".into(),
                unit: "ea".into(),
                quantity: 1,
                unit_price: Some(10000),
                tax_code: Some(0),
            }],
        };
        let preview = crate::billing::direct_preview(&app.db, &draft)?;
        let invoice = crate::billing::direct_confirm(&mut app.db, &draft, &preview)?;
        let changes = app.db.total_changes();
        app.open_payment(invoice);
        let (ctx, state) = native_context();
        let frame = |app: &mut Desktop| native_frame(app, &ctx, &state, [400.0, 200.0], 1.0);
        for _ in 0..3 {
            frame(&mut app);
        }
        let amount = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("receive_amount_rect")))
            .unwrap()
            .center();
        let click = |app: &mut Desktop, position| {
            for pressed in [true, false] {
                state.borrow_mut().events = vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                frame(app);
            }
        };
        click(&mut app, amount);
        state.borrow_mut().events = vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text("25.00".into()),
        ];
        frame(&mut app);
        assert!(state.borrow().text.iter().any(|(text, _)| text == "25.00"));
        state.borrow_mut().commands.clear();
        state
            .borrow_mut()
            .info
            .events
            .push(egui::ViewportEvent::Close);
        frame(&mut app);
        assert!(app.payment.is_some());
        assert_eq!(app.windows.close, Some(Kind::Payment));
        assert!(state
            .borrow()
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        state.borrow_mut().info.events.clear();
        for _ in 0..3 {
            frame(&mut app);
        }
        let return_button = state
            .borrow()
            .text
            .iter()
            .find(|(text, _)| text == "Return to editing")
            .unwrap()
            .1;
        click(&mut app, return_button);
        assert_eq!(app.windows.close, None);
        frame(&mut app);
        assert!(
            state.borrow().text.iter().any(|(text, _)| text == "25.00"),
            "Return to editing must preserve the typed amount"
        );
        state
            .borrow_mut()
            .info
            .events
            .push(egui::ViewportEvent::Close);
        frame(&mut app);
        state.borrow_mut().info.events.clear();
        for _ in 0..3 {
            frame(&mut app);
        }
        let discard_button = state
            .borrow()
            .text
            .iter()
            .find(|(text, _)| text == "Discard and close")
            .unwrap()
            .1;
        click(&mut app, discard_button);
        assert!(app.payment.is_none());
        assert_eq!(app.windows.close, None);
        assert!(!app.windows.builders.contains_key("Payment"));
        assert_eq!(app.db.total_changes(), changes);
        assert_eq!(crate::billing::balance(&app.db, invoice)?.remaining, 10000);
        Ok(())
    }
    #[test]
    fn task_new_and_edit_share_one_slot_and_close_request_preserves_input() -> Result<()> {
        let mut app = app()?;
        app.open_new_task();
        app.task_title = "Unsaved task".into();
        app.open_new_task();
        app.open_task_editor(999999);
        assert_eq!(app.task_title, "Unsaved task");
        assert!(app.new_task_open);
        assert!(!app.task_editor_open);
        app.request_business_close(Kind::Task);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.business_windows(ctx));
        assert!(app.new_task_open);
        assert_eq!(app.task_title, "Unsaved task");
        app.windows.close = None;
        app.discard_business(Kind::Task);
        assert!(!app.has_business_editors());
        Ok(())
    }
    #[test]
    fn invoice_creation_and_receipt_cannot_replace_each_other() -> Result<()> {
        let mut app = app()?;
        app.open_direct_invoice();
        assert!(app.direct_invoice.form.is_some());
        app.open_invoice_workbench();
        app.open_invoice(999999);
        assert!(app.direct_invoice.form.is_some());
        assert!(!app.invoice_workbench.open);
        assert!(!app.billing_ui.open);
        app.discard_business(Kind::Invoice);
        app.open_invoice_workbench();
        app.open_direct_invoice();
        assert!(app.invoice_workbench.open);
        assert!(app.direct_invoice.form.is_none());
        Ok(())
    }
    #[test]
    fn accounting_slot_and_report_remain_independent() -> Result<()> {
        let mut app = app()?;
        app.page = 6;
        app.open_accounting_form(0);
        app.page = 7;
        app.open_accounting_form(999999);
        assert!(app.accounting.form.is_some());
        app.windows.ledger = true;
        app.discard_business(Kind::Ledger);
        assert!(app.has_business_editors());
        app.discard_business(Kind::Accounting);
        assert!(!app.has_business_editors());
        Ok(())
    }
    #[test]
    fn invalid_window_placements_are_rejected() {
        for position in [[f32::NAN, 0.0], [f32::INFINITY, 0.0], [1e9, 0.0]] {
            assert!(!Placement {
                position,
                size: [1200.0, 800.0],
                scale: 1.0
            }
            .usable());
        }
        assert!(Placement {
            position: [-1920.0, 0.0],
            size: [1200.0, 800.0],
            scale: 1.5
        }
        .usable());
    }
    #[test]
    fn journal_reopen_keeps_unsaved_form() -> Result<()> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let mut app = Desktop::new(db, PathBuf::from("test.db"))?;
        app.open_journal(0);
        let original = app.gl.form.as_ref().unwrap() as *const _;
        app.open_journal(999999);
        assert_eq!(original, app.gl.form.as_ref().unwrap() as *const _);
        assert!(app.reuse_business(Kind::Journal));
        app.discard_business(Kind::Journal);
        assert!(!app.business_active(Kind::Journal));
        Ok(())
    }
}
