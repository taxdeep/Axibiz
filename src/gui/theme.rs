use super::*;
pub(super) const CONTROL_HEIGHT: f32 = 28.0;
pub(super) const ROW_HEIGHT: f32 = 28.0;
pub(super) const INK: Color32 = Color32::from_rgb(32, 48, 58);
pub(super) const MUTED: Color32 = Color32::from_rgb(92, 107, 118);
pub(super) const GREEN: Color32 = Color32::from_rgb(18, 126, 85);
pub(super) const NAV: Color32 = Color32::from_rgb(27, 42, 52);
pub(super) const BORDER: Color32 = Color32::from_rgb(222, 229, 233);
pub(super) const CANVAS: Color32 = Color32::from_rgb(245, 247, 249);
pub(super) use icons::Icon as DocumentIcon;
pub(super) fn document_action(
    ui: &mut egui::Ui,
    label: &str,
    icon: DocumentIcon,
    enabled: bool,
) -> egui::Response {
    icons::action(ui, icon, label, enabled)
}
fn typography(style: &mut egui::Style) {
    for text_style in [egui::TextStyle::Body, egui::TextStyle::Button] {
        style
            .text_styles
            .insert(text_style, egui::FontId::proportional(13.0));
    }
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
}
pub(super) fn business_typography(ui: &mut egui::Ui) {
    typography(ui.style_mut());
}
pub(super) fn trash_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    icons::button(ui, icons::Icon::Trash, tooltip)
}
pub(super) fn configure(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(INK);
    visuals.panel_fill = CANVAS;
    visuals.window_fill = Color32::WHITE;
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(246, 249, 250);
    visuals.selection.bg_fill = Color32::from_rgb(216, 240, 229);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, GREEN);
    visuals.hyperlink_color = GREEN;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(5);
        widget.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
        widget.fg_stroke = egui::Stroke::new(1.0_f32, INK);
    }
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(231, 244, 237);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(231, 244, 237);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, GREEN);
    visuals.widgets.active.bg_fill = Color32::from_rgb(210, 234, 221);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, GREEN);
    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.combo_width = 190.0;
    typography(&mut style);
    ctx.set_style(style);
}
pub(super) fn badge(ui: &mut egui::Ui, status: &str) {
    let (foreground, background) = match status {
        "completed" | "paid" | "posted" => (GREEN, Color32::from_rgb(224, 243, 234)),
        "invoiced" | "partially_paid" => (
            Color32::from_rgb(39, 96, 160),
            Color32::from_rgb(227, 238, 250),
        ),
        "cancel" | "draft" | "reversed" => (
            Color32::from_rgb(121, 129, 135),
            Color32::from_rgb(237, 240, 242),
        ),
        _ => (
            Color32::from_rgb(158, 110, 28),
            Color32::from_rgb(252, 243, 219),
        ),
    };
    egui::Frame::new()
        .fill(background)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(status).small().color(foreground).strong());
        });
}
pub(super) fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .corner_radius(4)
        .inner_margin(12)
}
