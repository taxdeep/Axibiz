use super::*;

pub(super) fn input(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add_sized(
        [250.0, theme::CONTROL_HEIGHT],
        egui::TextEdit::singleline(value),
    );
    ui.end_row();
}

pub(super) fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(6)
        .inner_margin(14)
}

pub(super) fn tab(ui: &mut egui::Ui, current: &mut usize, index: usize, name: &str) {
    let selected = *current == index;
    let response = ui.add(
        egui::Button::new(RichText::new(name).color(if selected {
            theme::GREEN
        } else {
            theme::MUTED
        }))
        .frame(false)
        .min_size(egui::vec2(0.0, 32.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    if selected {
        ui.painter().hline(
            response.rect.x_range(),
            response.rect.bottom(),
            egui::Stroke::new(2.0_f32, theme::GREEN),
        );
    }
    if response.clicked() {
        *current = index;
    }
}

pub(super) fn list_item(
    ui: &mut egui::Ui,
    scope: &str,
    id: i64,
    name: &str,
    balance: &str,
    selected: bool,
) -> egui::Response {
    let response = ui.add_sized(
        [ui.available_width(), 29.0],
        egui::Button::new("")
            .selected(selected)
            .corner_radius(0)
            .stroke(egui::Stroke::NONE),
    );
    let rect = response.rect.shrink2(egui::vec2(6.0, 2.0));
    let split = rect.left() + rect.width() * 0.50;
    let mut text = ui.new_child(
        egui::UiBuilder::new()
            .id_salt((scope, id, "name"))
            .max_rect(egui::Rect::from_min_max(
                rect.min,
                egui::pos2(split - 4.0, rect.bottom()),
            )),
    );
    text.add(egui::Label::new(name).truncate().selectable(false));
    let mut amount = ui.new_child(
        egui::UiBuilder::new()
            .id_salt((scope, id, "amount"))
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(split, rect.top()),
                rect.max,
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    amount.add(egui::Label::new(balance).truncate().selectable(false));
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new((format!("{scope}_list_row"), id)),
            response.rect,
        )
    });
    response.on_hover_text(format!("{name}\n{balance}"))
}

pub(super) fn empty_state(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    action: &str,
    enabled: bool,
) -> bool {
    section_frame()
        .fill(theme::CANVAS)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            ui.strong(title);
            ui.add(egui::Label::new(RichText::new(description).color(theme::MUTED)).wrap());
            ui.add_space(8.0);
            let response = ui.add_enabled(
                enabled,
                egui::Button::new(action).min_size(egui::vec2(0.0, 30.0)),
            );
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new(("party_empty_action", title)), response.rect)
            });
            response.clicked()
        })
        .inner
}

pub(super) fn editor_body(
    ui: &mut egui::Ui,
    scope: &str,
    height: f32,
    tabs: &[(usize, &str)],
    selected: &mut usize,
    fields: impl FnOnce(&mut egui::Ui, usize),
) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(145.0, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(145.0);
                ui.set_min_height(height);
                for (index, label) in tabs {
                    if ui
                        .add_sized(
                            [145.0, 32.0],
                            egui::Button::new(*label).selected(*selected == *index),
                        )
                        .clicked()
                    {
                        *selected = *index
                    }
                }
            },
        );
        let width = (ui.available_width() - 8.0).max(180.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                ui.set_min_height(height);
                egui::ScrollArea::both()
                    .id_salt((scope, "editor_fields"))
                    .max_height(height)
                    .show(ui, |ui| fields(ui, *selected));
            },
        );
    });
}

pub(super) fn contact_fields(ui: &mut egui::Ui, scope: &str, fields: &mut [(&str, &mut String)]) {
    let count = if ui.available_width() >= 800.0 { 2 } else { 1 };
    let split = fields.len().div_ceil(count);
    ui.columns(count, |columns| {
        for (index, chunk) in fields.chunks_mut(split).enumerate() {
            egui::Grid::new((scope, "contact_fields", index))
                .spacing([12.0, 6.0])
                .show(&mut columns[index], |ui| {
                    for (label, value) in chunk {
                        input(ui, label, value);
                    }
                });
        }
    });
}
pub(super) fn addresses(
    ui: &mut egui::Ui,
    labels: (&str, &str),
    primary: &mut String,
    shipping: &mut String,
    same: &mut bool,
) {
    ui.separator();
    ui.label("ADDRESS DETAILS");
    let primary_field = |ui: &mut egui::Ui, primary: &mut String| {
        ui.label(labels.0);
        ui.add(
            egui::TextEdit::multiline(primary)
                .desired_rows(4)
                .desired_width(ui.available_width().max(240.0)),
        );
    };
    let shipping_field =
        |ui: &mut egui::Ui, shipping: &mut String, same: &mut bool, primary: &str| {
            ui.label(labels.1);
            ui.checkbox(same, "Same as primary address");
            if *same {
                ui.add(egui::Label::new(primary).wrap());
            } else {
                ui.add(
                    egui::TextEdit::multiline(shipping)
                        .desired_rows(4)
                        .desired_width(ui.available_width().max(240.0)),
                );
                if ui.button("Copy Primary Address").clicked() {
                    *shipping = primary.into()
                }
            }
        };
    if ui.available_width() > 650.0 {
        ui.columns(2, |columns| {
            primary_field(&mut columns[0], primary);
            shipping_field(&mut columns[1], shipping, same, primary);
        });
    } else {
        primary_field(ui, primary);
        shipping_field(ui, shipping, same, primary);
    }
}

pub(super) enum ReviewAction {
    Keep,
    Record,
}
pub(super) fn settlement_review(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    values: &[(&str, String)],
    error: Option<&str>,
    focus: &mut bool,
) -> Option<ReviewAction> {
    controls::review(
        ctx,
        controls::Review {
            cancel: "Keep Editing",
            id,
            title,
            values,
            error,
            confirm: "Confirm & Record",
            notice: "Records an accounting event. It does not initiate a bank transfer.",
        },
        focus,
    )
    .map(|a| match a {
        controls::ReviewAction::Keep => ReviewAction::Keep,
        controls::ReviewAction::Confirm => ReviewAction::Record,
    })
}
