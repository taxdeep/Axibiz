//! Shared presentation and interaction primitives. Domain modules still own validation/writes.
use super::*;

pub(super) fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(theme::GREEN),
    )
}
pub(super) fn error(ui: &mut egui::Ui, id: impl std::hash::Hash, message: &str) {
    if !message.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(48.0)
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(message).color(Color32::DARK_RED)).wrap());
            });
    }
}
/// A stable workflow surface. Callers retain their own domain actions and validation.
pub(super) fn footer(ui: &mut egui::Ui, reason: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    ui.separator();
    if let Some(reason) = reason {
        ui.add(egui::Label::new(RichText::new(reason).small().color(theme::MUTED)).wrap());
    }
    add(ui);
}
pub(super) fn search(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut String,
    width: f32,
    hint: &str,
) {
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            let width = width.min(ui.available_width().max(80.0));
            let entry = icons::search_field(ui, value, width, hint);
            if !egui::Popup::is_any_open(ui.ctx())
                && ui.ctx().input_mut(|i| {
                    i.consume_shortcut(&egui::KeyboardShortcut::new(
                        egui::Modifiers::CTRL,
                        egui::Key::F,
                    ))
                })
            {
                entry.request_focus();
            }
            if !value.is_empty()
                && icons::button_at(
                    ui,
                    egui::Rect::from_min_size(
                        egui::pos2(entry.rect.right() - theme::CONTROL_HEIGHT, entry.rect.top()),
                        egui::vec2(theme::CONTROL_HEIGHT, theme::CONTROL_HEIGHT),
                    ),
                    icons::Icon::Clear,
                    "Clear search",
                )
                .clicked()
            {
                value.clear();
            }
        });
    });
}
pub(super) fn escape(ctx: &egui::Context, top: bool, popup: bool) -> bool {
    top && !popup && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}
/// Read-only registers share sizing, scrolling, selection and row actions.
pub(super) fn register(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    selected: &mut i64,
    edit: bool,
) -> (bool, Option<i64>) {
    let mut changed = false;
    let mut action = None;
    if rows.is_empty() {
        ui.label("No matching records.");
        return (false, None);
    }
    let widths: Vec<f32> = headers
        .iter()
        .enumerate()
        .map(|(col, title)| {
            if edit && col + 1 == headers.len() {
                return 40.0;
            }
            let len = rows
                .iter()
                .map(|r| r.get(col).map(|v| v.chars().count()).unwrap_or(0))
                .max()
                .unwrap_or(0)
                .max(title.len());
            (len as f32 * 7.0 + 18.0).clamp(64.0, 280.0)
        })
        .collect();
    let remaining = (ui.ctx().content_rect().bottom() - ui.cursor().min.y - 64.0).max(100.0);
    let height = (rows.len() as f32 * theme::ROW_HEIGHT + 34.0).min(remaining);
    invoice_sheet::register_columns(
        ui,
        id,
        height,
        headers,
        &widths,
        rows.len(),
        |ui, row, column| {
            let data = &rows[row];
            let key = data.first().and_then(|v| v.parse::<i64>().ok());
            if edit && column + 1 == headers.len() {
                let response = icons::button(ui, icons::Icon::Edit, "Edit record");
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new(("register_action", id, data[0].clone())),
                        response.rect,
                    )
                });
                if response.clicked() {
                    action = key;
                }
            } else if let Some(value) = data.get(column) {
                let response = if column == 0 && key.is_some() {
                    ui.selectable_label(key == Some(*selected), value)
                } else {
                    let numeric = ["amount", "total", "balance", "debit", "credit"]
                        .iter()
                        .any(|s| headers[column].to_lowercase().contains(s));
                    if numeric {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(value)
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            )
                        })
                        .inner
                    } else {
                        ui.add(
                            egui::Label::new(value)
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                    }
                }
                .on_hover_text(value);
                if response.clicked() {
                    if let Some(key) = key {
                        *selected = key;
                        changed = true
                    }
                }
            }
        },
    );
    (changed, action)
}

pub(super) enum ReviewAction {
    Keep,
    Confirm,
}
pub(super) struct Review<'a> {
    pub cancel: &'a str,
    pub id: &'a str,
    pub title: &'a str,
    pub values: &'a [(&'a str, String)],
    pub error: Option<&'a str>,
    pub confirm: &'a str,
    pub notice: &'a str,
}
pub(super) fn review(
    ctx: &egui::Context,
    model: Review<'_>,
    focus: &mut bool,
) -> Option<ReviewAction> {
    review_content(ctx, model, focus, |_| {})
}
pub(super) fn review_content(
    ctx: &egui::Context,
    model: Review<'_>,
    focus: &mut bool,
    extra: impl FnOnce(&mut egui::Ui),
) -> Option<ReviewAction> {
    let mut action = None;
    let response = egui::Modal::new(egui::Id::new(model.id)).show(ctx, |ui| {
        theme::business_typography(ui);
        ui.set_width(480.0_f32.min((ctx.content_rect().width() - 64.0).max(250.0)));
        ui.heading(model.title);
        egui::ScrollArea::vertical()
            .id_salt((model.id, "review_values"))
            .max_height((ctx.content_rect().height() - 175.0).max(100.0))
            .show(ui, |ui| {
                let value_width = (ui.available_width() - 108.0).max(120.0);
                egui::Grid::new((model.id, "summary"))
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in model.values {
                            ui.allocate_ui_with_layout(
                                egui::vec2(90.0, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(90.0);
                                    ui.add(egui::Label::new(RichText::new(*label).small()).wrap());
                                },
                            );
                            let field = ui.allocate_ui_with_layout(
                                egui::vec2(value_width, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(value_width);
                                    ui.add(egui::Label::new(value).wrap());
                                },
                            );
                            #[cfg(test)]
                            ctx.data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new((model.id, "review_value", *label)),
                                    field.response.rect,
                                )
                            });
                            #[cfg(not(test))]
                            let _ = field;
                            ui.end_row();
                        }
                    });
                extra(ui);
                if let Some(message) = model.error {
                    error(ui, (model.id, "error"), message)
                }
            });
        if !model.notice.is_empty() {
            ui.add(egui::Label::new(RichText::new(model.notice).small()).wrap());
        }
        ui.separator();
        ui.horizontal(|ui| {
            let keep = ui.button(model.cancel);
            if *focus {
                keep.request_focus();
                *focus = false;
            }
            if keep.clicked() {
                action = Some(ReviewAction::Keep)
            }
            let confirm = primary(ui, model.confirm, model.error.is_none());
            #[cfg(test)]
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new((model.id, "review_confirm")), confirm.rect)
            });
            if confirm.clicked() {
                action = Some(ReviewAction::Confirm)
            }
        });
    });
    if action.is_none() && escape(ctx, response.is_top_modal, response.any_popup_open) {
        action = Some(ReviewAction::Keep)
    }
    action
}
#[derive(Default)]
pub(super) struct CloseGuard {
    pending: bool,
    focus: bool,
}
impl CloseGuard {
    pub fn request(&mut self) {
        self.pending = true;
        self.focus = true;
    }
    pub fn show(&mut self, ctx: &egui::Context, id: &str, dirty: bool) -> bool {
        if !self.pending {
            return false;
        }
        if !dirty {
            self.pending = false;
            return true;
        }
        match review(
            ctx,
            Review {
                cancel: "Keep Changes",
                id,
                title: "Discard unsaved changes?",
                values: &[],
                error: None,
                confirm: "Discard Changes",
                notice: "Your saved records will remain. Unsaved input will be discarded.",
            },
            &mut self.focus,
        ) {
            Some(ReviewAction::Confirm) => {
                self.pending = false;
                true
            }
            Some(ReviewAction::Keep) => {
                self.pending = false;
                false
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        focus: &mut bool,
        events: Vec<egui::Event>,
        blocked: bool,
    ) -> Option<ReviewAction> {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                result = review(
                    ctx,
                    Review {
                        cancel: "Keep Editing",
                        id: "safe_review",
                        title: "Review Payment",
                        values: &[("Reference", "Long customer reference / CAD 100.00".into())],
                        error: blocked.then_some("Account is inactive."),
                        confirm: "Record Payment",
                        notice: "Records the reviewed payment only.",
                    },
                    focus,
                );
            },
        );
        result
    }
    fn key(key: egui::Key) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }
    #[test]
    fn review_defaults_to_keep_editing_and_escape_never_confirms() {
        for keypress in [egui::Key::Enter, egui::Key::Escape] {
            let ctx = egui::Context::default();
            let mut focus = true;
            for _ in 0..3 {
                assert!(frame(&ctx, &mut focus, vec![], false).is_none());
            }
            assert!(matches!(
                frame(&ctx, &mut focus, key(keypress), false),
                Some(ReviewAction::Keep)
            ));
        }
    }
    #[test]
    fn blocked_review_cannot_commit_and_footer_fits_small_window() {
        let ctx = egui::Context::default();
        let mut focus = true;
        for _ in 0..3 {
            frame(&ctx, &mut focus, vec![], true);
        }
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("safe_review", "review_confirm"))))
            .unwrap();
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))
                .contains_rect(rect)
        );
        for pressed in [true, false] {
            assert!(frame(
                &ctx,
                &mut focus,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE
                    }
                ],
                true
            )
            .is_none());
        }
    }
}
