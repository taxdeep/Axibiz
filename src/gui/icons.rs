//! Shared vector commands. Text labels remain on financial workflow actions.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Icon {
    Trash,
    Edit,
    Copy,
    Search,
    Filter,
    More,
    Print,
    Pdf,
    Payment,
    Export,
    Add,
    Clear,
    Menu,
    Tasks,
    Invoice,
    Customer,
    Vendor,
    Product,
    Bill,
    Journal,
    Ledger,
    Accounts,
    Tax,
    Settings,
}
fn paint(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, icon: Icon) {
    let color = if response.enabled() {
        ui.style().interact(response).fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    paint_color(ui, response, rect, icon, color);
}
pub(super) fn paint_color(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    icon: Icon,
    color: Color32,
) {
    let painter = ui
        .painter()
        .with_clip_rect(response.rect.intersect(ui.clip_rect()));
    let stroke = egui::Stroke::new(1.5_f32, color);
    let p = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + x * rect.width() / 24.0,
            rect.top() + y * rect.height() / 24.0,
        )
    };
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
    };
    let box_at = |x, y, w, h| {
        painter.rect_stroke(
            egui::Rect::from_min_max(p(x, y), p(x + w, y + h)),
            1.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Trash => {
            for (a, b) in [
                ((4., 6.), (20., 6.)),
                ((9., 6.), (9., 3.)),
                ((9., 3.), (15., 3.)),
                ((15., 3.), (15., 6.)),
                ((6., 6.), (7., 21.)),
                ((7., 21.), (17., 21.)),
                ((17., 21.), (18., 6.)),
                ((10., 10.), (10., 17.)),
                ((14., 10.), (14., 17.)),
            ] {
                line(a, b);
            }
        }
        Icon::Edit => {
            for (a, b) in [
                ((4., 16.), (16., 4.)),
                ((16., 4.), (20., 8.)),
                ((20., 8.), (8., 20.)),
                ((8., 20.), (4., 20.)),
                ((4., 20.), (4., 16.)),
                ((14., 6.), (18., 10.)),
            ] {
                line(a, b);
            }
        }
        Icon::Copy => {
            box_at(8., 8., 12., 13.);
            line((5., 16.), (3., 16.));
            line((3., 16.), (3., 3.));
            line((3., 3.), (15., 3.));
            line((15., 3.), (15., 5.));
        }
        Icon::Search => {
            painter.circle_stroke(p(10., 10.), rect.width() * 0.25, stroke);
            line((15., 15.), (21., 21.));
        }
        Icon::Filter => {
            for (a, b) in [
                ((3., 4.), (21., 4.)),
                ((21., 4.), (14., 12.)),
                ((14., 12.), (14., 19.)),
                ((14., 19.), (10., 21.)),
                ((10., 21.), (10., 12.)),
                ((10., 12.), (3., 4.)),
            ] {
                line(a, b);
            }
        }
        Icon::More => {
            for x in [5., 12., 19.] {
                painter.circle_filled(p(x, 12.), 1.5, color);
            }
        }
        Icon::Print => {
            box_at(6., 3., 12., 6.);
            box_at(3., 8., 18., 9.);
            box_at(6., 14., 12., 7.);
            line((16., 11.), (18., 11.));
        }
        Icon::Pdf => {
            box_at(5., 2., 14., 20.);
            line((8., 6.), (16., 6.));
            painter.text(
                p(12., 14.),
                egui::Align2::CENTER_CENTER,
                "PDF",
                egui::FontId::proportional(6.0),
                color,
            );
        }
        Icon::Payment => {
            box_at(2., 5., 20., 14.);
            line((2., 10.), (22., 10.));
            line((6., 15.), (10., 15.));
        }
        Icon::Export => {
            line((12., 3.), (12., 15.));
            line((7., 10.), (12., 15.));
            line((12., 15.), (17., 10.));
            line((4., 16.), (4., 21.));
            line((4., 21.), (20., 21.));
            line((20., 21.), (20., 16.));
        }
        Icon::Add => {
            line((12., 4.), (12., 20.));
            line((4., 12.), (20., 12.));
        }
        Icon::Clear => {
            line((6., 6.), (18., 18.));
            line((6., 18.), (18., 6.));
        }
        Icon::Menu => {
            for y in [6., 12., 18.] {
                line((3., y), (21., y));
            }
        }
        Icon::Tasks => {
            box_at(5., 4., 14., 17.);
            box_at(9., 2., 6., 4.);
            line((8., 13.), (11., 16.));
            line((11., 16.), (16., 10.));
        }
        Icon::Invoice => {
            for (a, b) in [
                ((5., 2.), (14., 2.)),
                ((14., 2.), (20., 8.)),
                ((20., 8.), (20., 22.)),
                ((20., 22.), (5., 22.)),
                ((5., 22.), (5., 2.)),
                ((14., 2.), (14., 8.)),
                ((14., 8.), (20., 8.)),
                ((8., 12.), (16., 12.)),
                ((8., 16.), (16., 16.)),
            ] {
                line(a, b);
            }
        }
        Icon::Customer => {
            painter.circle_stroke(p(12., 7.), rect.width() / 6.0, stroke);
            for (a, b) in [
                ((4., 21.), (4., 17.)),
                ((4., 17.), (8., 13.)),
                ((8., 13.), (16., 13.)),
                ((16., 13.), (20., 17.)),
                ((20., 17.), (20., 21.)),
                ((20., 21.), (4., 21.)),
            ] {
                line(a, b);
            }
        }
        Icon::Vendor => {
            for (a, b) in [
                ((3., 9.), (6., 3.)),
                ((6., 3.), (18., 3.)),
                ((18., 3.), (21., 9.)),
                ((3., 9.), (21., 9.)),
                ((5., 9.), (5., 21.)),
                ((5., 21.), (19., 21.)),
                ((19., 21.), (19., 9.)),
                ((10., 21.), (10., 14.)),
                ((10., 14.), (15., 14.)),
                ((15., 14.), (15., 21.)),
            ] {
                line(a, b);
            }
        }
        Icon::Product => {
            for (a, b) in [
                ((3., 3.), (12., 3.)),
                ((12., 3.), (22., 13.)),
                ((22., 13.), (13., 22.)),
                ((13., 22.), (3., 12.)),
                ((3., 12.), (3., 3.)),
            ] {
                line(a, b);
            }
            painter.circle_stroke(p(8., 8.), rect.width() / 12.0, stroke);
        }
        Icon::Bill => {
            for (a, b) in [
                ((5., 2.), (19., 2.)),
                ((19., 2.), (19., 22.)),
                ((19., 22.), (15., 19.)),
                ((15., 19.), (12., 22.)),
                ((12., 22.), (9., 19.)),
                ((9., 19.), (5., 22.)),
                ((5., 22.), (5., 2.)),
                ((8., 7.), (16., 7.)),
                ((8., 11.), (16., 11.)),
                ((8., 15.), (13., 15.)),
            ] {
                line(a, b);
            }
        }
        Icon::Journal => {
            box_at(4., 3., 16., 18.);
            line((8., 3.), (8., 21.));
            line((11., 8.), (17., 8.));
            line((11., 12.), (17., 12.));
            line((11., 16.), (15., 16.));
        }
        Icon::Ledger => {
            box_at(2., 4., 20., 17.);
            line((2., 9.), (22., 9.));
            line((8., 4.), (8., 21.));
            line((15., 9.), (15., 21.));
            line((2., 15.), (22., 15.));
        }
        Icon::Accounts => {
            box_at(8., 2., 8., 6.);
            box_at(2., 16., 7., 6.);
            box_at(15., 16., 7., 6.);
            line((12., 8.), (12., 12.));
            line((5.5, 12.), (18.5, 12.));
            line((5.5, 12.), (5.5, 16.));
            line((18.5, 12.), (18.5, 16.));
        }
        Icon::Tax => {
            painter.circle_stroke(p(12., 12.), rect.width() * 0.44, stroke);
            painter.circle_stroke(p(8., 8.), rect.width() / 12.0, stroke);
            painter.circle_stroke(p(16., 16.), rect.width() / 12.0, stroke);
            line((7., 17.), (17., 7.));
        }
        Icon::Settings => {
            painter.circle_stroke(p(12., 12.), rect.width() * 0.14, stroke);
            painter.circle_stroke(p(12., 12.), rect.width() * 0.32, stroke);
            for angle in 0..8 {
                let angle = angle as f32 * std::f32::consts::FRAC_PI_4;
                line(
                    (12. + 7.5 * angle.cos(), 12. + 7.5 * angle.sin()),
                    (12. + 11. * angle.cos(), 12. + 11. * angle.sin()),
                );
            }
        }
    }
}
pub(super) fn navigation_toggle(ui: &mut egui::Ui, collapsed: bool) -> egui::Response {
    let label = if collapsed {
        "Show main menu"
    } else {
        "Hide main menu"
    };
    let response = ui.add_sized([40.0, 40.0], egui::Button::new("").frame(false));
    if response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
        ui.painter().circle_filled(
            response.rect.center(),
            20.0,
            Color32::from_white_alpha(if response.is_pointer_button_down_on() {
                38
            } else {
                22
            }),
        );
    }
    if response.has_focus() {
        ui.painter().circle_stroke(
            response.rect.center(),
            19.0,
            egui::Stroke::new(1.5_f32, Color32::from_white_alpha(150)),
        );
    }
    paint_color(
        ui,
        &response,
        egui::Rect::from_center_size(response.rect.center(), egui::vec2(24.0, 24.0)),
        Icon::Menu,
        Color32::from_rgb(215, 225, 232),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    response.on_hover_text(label)
}
pub(super) fn button(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    button_sized(ui, icon, label, theme::ROW_HEIGHT)
}
fn button_sized(ui: &mut egui::Ui, icon: Icon, label: &str, size: f32) -> egui::Response {
    let response = ui.add_sized([size, size], egui::Button::new(""));
    paint(
        ui,
        &response,
        egui::Rect::from_center_size(response.rect.center(), egui::vec2(16.0, 16.0)),
        icon,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    response.on_hover_text(label).on_disabled_hover_text(label)
}
pub(super) fn button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: Icon,
    label: &str,
) -> egui::Response {
    let response = ui.put(rect, egui::Button::new(""));
    paint(
        ui,
        &response,
        egui::Rect::from_center_size(response.rect.center(), egui::vec2(16.0, 16.0)),
        icon,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    response.on_hover_text(label).on_disabled_hover_text(label)
}
pub(super) fn action(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    labelled_action(ui, icon, label, enabled, false)
}
pub(super) fn menu_item(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    labelled_action(ui, icon, label, true, true)
}
fn labelled_action(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    menu: bool,
) -> egui::Response {
    let icon_id = ui.next_auto_id().with("command_icon");
    let glyph = egui::Atom::custom(icon_id, egui::vec2(16.0, 16.0));
    let button = if menu {
        egui::Button::new((glyph, label, egui::Atom::grow()))
    } else {
        egui::Button::new((glyph, label))
    };
    let output = ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui)).inner;
    if let Some(rect) = output.rect(icon_id) {
        paint(ui, &output.response, rect, icon);
    }
    let response = output.response;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    response.on_hover_text(label).on_disabled_hover_text(label)
}
pub(super) fn menu(ui: &mut egui::Ui, icon: Icon, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let response = ui.menu_button(format!("       {label}"), add).response;
    paint(
        ui,
        &response,
        egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 15.0, response.rect.center().y),
            egui::vec2(16.0, 16.0),
        ),
        icon,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    response.on_hover_text(label);
}
pub(super) fn label(ui: &mut egui::Ui, icon: Icon, label: &str) {
    let response = ui.label(format!("        {label}"));
    paint(
        ui,
        &response,
        egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 10.0, response.rect.center().y),
            egui::vec2(16.0, 16.0),
        ),
        icon,
    );
}
pub(super) fn search_field(
    ui: &mut egui::Ui,
    value: &mut String,
    width: f32,
    hint: &str,
) -> egui::Response {
    let response = ui.add_sized(
        [width, theme::CONTROL_HEIGHT],
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .margin(egui::Margin {
                left: 26,
                right: 30,
                top: 4,
                bottom: 4,
            }),
    );
    paint(
        ui,
        &response,
        egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 13.0, response.rect.center().y),
            egui::vec2(16.0, 16.0),
        ),
        Icon::Search,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_icons_have_separate_targets_and_click_only_the_requested_row() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut rects = Vec::new();
        let mut clicked = Vec::new();
        let mut frame = |events: Vec<egui::Event>| {
            rects.clear();
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
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, icon) in [Icon::Trash, Icon::Edit, Icon::Copy]
                            .into_iter()
                            .enumerate()
                        {
                            let r = button(ui, icon, "Row action");
                            rects.push(r.rect);
                            if r.clicked() {
                                clicked.push(i);
                            }
                        }
                    });
                },
            );
            rects.clone()
        };
        let rects = frame(vec![]);
        for r in &rects {
            assert_eq!(r.height(), 28.0);
            assert_eq!(r.width(), 28.0);
        }
        for pair in rects.windows(2) {
            assert!(pair[0].bottom() <= pair[1].top());
        }
        let pos = rects[1].center();
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(clicked, vec![1]);
    }
}
