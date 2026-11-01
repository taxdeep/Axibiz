//! Reusable prefix-search picker, shared by service, customer and vendor inputs.
use eframe::egui::{self, Color32, Id, Key};
pub const ARROW_WIDTH: f32 = 28.0;

pub struct Choice {
    pub id: i64,
    pub name: String,
    pub detail: String,
}
impl Choice {
    pub fn matches(&self, input: &str) -> bool {
        let query = input.trim().to_lowercase();
        query.is_empty()
            || self.name.trim().to_lowercase().starts_with(&query)
            || self.detail.trim().to_lowercase().starts_with(&query)
            || self
                .id
                .to_string()
                .starts_with(query.trim_start_matches('#'))
    }
}
#[derive(Clone, Default)]
struct State {
    bound: i64,
    text: String,
    query: String,
    highlight: usize,
    selected_zero: bool,
}
#[derive(Default)]
pub struct Outcome {
    pub create: Option<String>,
    pub pending: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    id: Id,
    value: &mut i64,
    choices: &[Choice],
    width: f32,
    allow_create: bool,
) -> Outcome {
    let state_id = id.with("state");
    let popup_id = id.with("popup");
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<State>(state_id))
        .unwrap_or_default();
    if state.bound != *value {
        state.bound = *value;
        state.text = choices
            .iter()
            .find(|c| c.id == *value)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        state.query.clear();
        state.highlight = 0;
        state.selected_zero = *value == 0;
    }
    let mut outcome = Outcome::default();
    let mut selected = None;
    let mut navigate = false;
    let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let keyboard = ui.memory(|m| m.has_focus(id));
    let down = keyboard && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
    let up = keyboard && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
    let enter =
        keyboard && was_open && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
    let height = ui
        .spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body) + 8.0);
    let gap = ui.spacing().item_spacing.x;
    // Allocate one complete grid cell, then place the two controls without
    // advancing the parent cursor or inheriting a column's measured width.
    let (anchor, _) = ui.allocate_exact_size(
        egui::vec2(width + gap + ARROW_WIDTH, height),
        egui::Sense::hover(),
    );
    let input_rect = egui::Rect::from_min_size(anchor.min, egui::vec2(width, height));
    let arrow_rect = egui::Rect::from_min_size(
        egui::pos2(input_rect.right() + gap, anchor.top()),
        egui::vec2(ARROW_WIDTH, height),
    );
    let response = {
        let input = ui.place(
            input_rect,
            egui::TextEdit::singleline(&mut state.text)
                .id(id)
                .desired_width(width)
                .hint_text("Search name or code..."),
        );
        // A focus transfer in a large form must not steal another picker's
        // popup. Open only on an explicit click, typing or navigation key.
        if input.clicked() {
            if *value != 0 || state.selected_zero {
                state.text.clear();
                state.query.clear();
            }
            egui::Popup::open_id(ui.ctx(), popup_id);
        }
        if input.changed() {
            state.query = state.text.clone();
            state.highlight = 0;
            *value = 0;
            state.bound = 0;
            state.selected_zero = false;
            egui::Popup::open_id(ui.ctx(), popup_id);
        }
        let arrow = ui.interact(arrow_rect, id.with("arrow"), egui::Sense::click());
        arrow.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "展开选项")
        });
        let visuals = ui.style().interact(&arrow);
        ui.painter().rect(
            arrow.rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let center = arrow.rect.center();
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(-4.0, -2.0),
                center + egui::vec2(4.0, -2.0),
                center + egui::vec2(0.0, 3.0),
            ],
            visuals.fg_stroke.color,
            egui::Stroke::NONE,
        ));
        if arrow.clicked() {
            state.query.clear();
            state.highlight = 0;
            egui::Popup::open_id(ui.ctx(), popup_id);
            input.request_focus();
            ui.ctx().request_repaint();
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(id.with("arrow_rect"), arrow.rect));
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(id.with("input_rect"), input.rect));
        input
    };
    let visible: Vec<_> = choices.iter().filter(|c| c.matches(&state.query)).collect();
    if down || up {
        egui::Popup::open_id(ui.ctx(), popup_id);
    }
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    if open && (response.has_focus() || response.lost_focus()) {
        if down {
            state.highlight = if was_open {
                (state.highlight + 1).min(visible.len().saturating_sub(1))
            } else {
                0
            };
            navigate = true;
        }
        if up {
            state.highlight = state.highlight.saturating_sub(1);
            navigate = true;
        }
        if enter {
            if let Some(choice) = visible.get(state.highlight) {
                selected = Some(choice.id);
            }
        }
    }
    let popup = egui::Popup::new(popup_id, ui.ctx().clone(), anchor, response.layer_id)
        .open_memory(None)
        .width((width + 32.0).max(340.0))
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .show(|ui| {
            if allow_create {
                let label = if state.query.trim().is_empty() {
                    "＋ Add new · 新建业务".into()
                } else {
                    format!("＋ Add new · 新建 {}", state.query.trim())
                };
                if ui.button(label).clicked() {
                    outcome.create = Some(state.query.trim().to_owned());
                    ui.close();
                }
                ui.separator();
            }
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .id_salt(id.with("results"))
                .show(ui, |ui| {
                    for (index, choice) in visible.iter().enumerate() {
                        let label = if choice.detail.is_empty() {
                            format!("{}    #{}", choice.name, choice.id)
                        } else {
                            format!("{}    {}", choice.name, choice.detail)
                        };
                        let item = ui.selectable_label(index == state.highlight, label);
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(id.with(("choice_rect", choice.id)), item.rect)
                        });
                        if navigate && index == state.highlight {
                            item.scroll_to_me(Some(egui::Align::Center));
                        }
                        if item.clicked() {
                            selected = Some(choice.id);
                            ui.close();
                        }
                    }
                    if visible.is_empty() {
                        ui.colored_label(Color32::GRAY, "No matching records.");
                    }
                });
        });
    // TextEdit gains focus on mouse press. Closing on the following release
    // would treat the same input click as an outside click and hide the list.
    if let Some(popup) = popup {
        let outside = ui.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !anchor.contains(p) && !popup.response.rect.contains(p))
        });
        if outside {
            egui::Popup::close_id(ui.ctx(), popup_id);
        }
    }
    if let Some(key) = selected {
        *value = key;
        state.bound = key;
        state.selected_zero = key == 0;
        state.text = choices
            .iter()
            .find(|c| c.id == key)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        state.query.clear();
        egui::Popup::close_id(ui.ctx(), popup_id);
        response.surrender_focus();
    } else if !response.has_focus() && !egui::Popup::is_id_open(ui.ctx(), popup_id) && *value != 0 {
        state.text = choices
            .iter()
            .find(|c| c.id == *value)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        state.query.clear();
    }
    outcome.pending = *value == 0 && !state.selected_zero && !state.text.trim().is_empty();
    ui.ctx().data_mut(|d| d.insert_temp(state_id, state));
    outcome
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_transfers_do_not_replace_popup_and_keyboard_can_choose_none() {
        let ctx = egui::Context::default();
        let id = Id::new("focus_picker");
        let other = Id::new("optional_picker");
        let choices = vec![
            Choice {
                id: 0,
                name: "None".into(),
                detail: String::new(),
            },
            Choice {
                id: 2,
                name: "Bank".into(),
                detail: String::new(),
            },
        ];
        let frame = |events: Vec<egui::Event>, first: &mut i64, second: &mut i64| {
            let mut pending = false;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::Modal::new(Id::new("focus_modal")).show(ctx, |ui| {
                        show(ui, id, first, &choices, 240.0, false);
                        pending = show(ui, other, second, &choices, 240.0, false).pending;
                    });
                },
            );
            pending
        };
        let mut first = 0;
        let mut second = 0;
        for _ in 0..3 {
            frame(vec![], &mut first, &mut second);
        }
        egui::Popup::open_id(&ctx, id.with("popup"));
        ctx.memory_mut(|m| m.request_focus(other));
        frame(vec![], &mut first, &mut second);
        assert!(
            egui::Popup::is_id_open(&ctx, id.with("popup")),
            "focus transfer must not replace an explicitly opened list"
        );
        assert!(!egui::Popup::is_id_open(&ctx, other.with("popup")));
        egui::Popup::close_all(&ctx);
        let key = |key| {
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(key(Key::ArrowDown), &mut first, &mut second);
        assert!(
            egui::Popup::is_id_open(&ctx, other.with("popup")),
            "Down must open the focused picker from its closed state"
        );
        assert!(
            !frame(key(Key::Enter), &mut first, &mut second),
            "selecting explicit None must not become pending input"
        );
        assert_eq!(second, 0);
        assert!(!frame(vec![], &mut first, &mut second));
        ctx.memory_mut(|m| m.request_focus(other));
        assert!(
            frame(
                vec![egui::Event::Text("Unknown".into())],
                &mut first,
                &mut second
            ),
            "unselected text must still be pending"
        );
    }
    #[test]
    fn prefixes_are_case_insensitive_and_match_ids_and_unicode() {
        let bc = Choice {
            id: 12,
            name: "BC Registry".into(),
            detail: String::new(),
        };
        for q in ["bc", "BC", " bc ", "#12", "1", ""] {
            assert!(bc.matches(q));
        }
        for q in ["registry", "abc", "13"] {
            assert!(!bc.matches(q));
        }
        assert!(Choice {
            id: 2,
            name: "北京服务".into(),
            detail: String::new()
        }
        .matches("北京"));
    }
    #[test]
    fn typing_and_keyboard_confirm_select_a_real_record() {
        let ctx = egui::Context::default();
        let id = Id::new("test_picker");
        let mut value = 0;
        let choices = vec![
            Choice {
                id: 1,
                name: "BC Registry".into(),
                detail: String::new(),
            },
            Choice {
                id: 2,
                name: "BC Services".into(),
                detail: String::new(),
            },
            Choice {
                id: 3,
                name: "Annual Report".into(),
                detail: String::new(),
            },
        ];
        let frame = |events: Vec<egui::Event>, value: &mut i64| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show(ui, id, value, &choices, 240.0, true);
                    });
                },
            );
        };
        frame(vec![], &mut value);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(vec![egui::Event::Text("bc".into())], &mut value);
        assert_eq!(
            ctx.data_mut(|d| d.get_temp::<State>(id.with("state")))
                .unwrap()
                .query,
            "bc"
        );
        assert_eq!(value, 0);
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![key(Key::ArrowDown)], &mut value);
        frame(vec![key(Key::Enter)], &mut value);
        assert_eq!(value, 2);
        assert!(!egui::Popup::is_id_open(&ctx, id.with("popup")));
    }
    #[test]
    fn mouse_click_opens_and_keeps_popup_in_modal_with_multiple_pickers() {
        let ctx = egui::Context::default();
        let id = Id::new("customer_mouse_picker");
        let other_id = Id::new("service_mouse_picker");
        let mut customer = 0;
        let mut service = 0;
        let choices = vec![Choice {
            id: 1,
            name: "BC Registry".into(),
            detail: String::new(),
        }];
        let frame = |events: Vec<egui::Event>, customer: &mut i64, service: &mut i64| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::Modal::new(Id::new("task_mouse_modal")).show(ctx, |ui| {
                        ui.set_width(600.0);
                        show(ui, id, customer, &choices, 240.0, false);
                        ui.add_space(180.0);
                        show(ui, other_id, service, &choices, 240.0, true);
                    });
                },
            );
        };
        for _ in 0..3 {
            frame(vec![], &mut customer, &mut service);
        }
        let click = |position, pressed| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let pos = ctx.read_response(id).unwrap().rect.center();
        frame(click(pos, true), &mut customer, &mut service);
        frame(click(pos, false), &mut customer, &mut service);
        frame(vec![], &mut customer, &mut service);
        assert!(
            egui::Popup::is_id_open(&ctx, id.with("popup")),
            "input click must leave candidate list open after mouse release"
        );
        egui::Popup::close_id(&ctx, id.with("popup"));
        let arrow = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(id.with("arrow_rect")))
            .unwrap()
            .center();
        frame(click(arrow, true), &mut customer, &mut service);
        frame(click(arrow, false), &mut customer, &mut service);
        frame(vec![], &mut customer, &mut service);
        assert!(
            egui::Popup::is_id_open(&ctx, id.with("popup")),
            "arrow click must open candidate list"
        );
        let candidate = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(id.with(("choice_rect", 1i64))))
            .unwrap()
            .center();
        frame(click(candidate, true), &mut customer, &mut service);
        frame(click(candidate, false), &mut customer, &mut service);
        assert_eq!(
            customer, 1,
            "candidate must be clickable above the parent modal"
        );
        frame(click(arrow, true), &mut customer, &mut service);
        frame(click(arrow, false), &mut customer, &mut service);
        frame(vec![], &mut customer, &mut service);
        frame(
            click(egui::pos2(50.0, 50.0), true),
            &mut customer,
            &mut service,
        );
        frame(
            click(egui::pos2(50.0, 50.0), false),
            &mut customer,
            &mut service,
        );
        assert!(
            !egui::Popup::is_id_open(&ctx, id.with("popup")),
            "outside click must close list"
        );
    }
}
