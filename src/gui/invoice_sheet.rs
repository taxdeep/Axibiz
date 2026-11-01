use super::*;
use serde_json::Value;

pub(super) const BLUE: Color32 = Color32::from_rgb(96, 127, 158);
const STRIPE: Color32 = Color32::from_rgb(240, 245, 250);
const LINE: Color32 = Color32::from_rgb(202, 209, 217);
const ROW_HEIGHT: f32 = theme::ROW_HEIGHT;
const TAX_COLUMN_WIDTH: f32 = 64.0;

pub(super) fn compact(ui: &mut egui::Ui) {
    theme::business_typography(ui);
    let style = ui.style_mut();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = theme::CONTROL_HEIGHT;
    style.spacing.combo_width = 110.0;
}

pub(super) fn tax_code(ui: &mut egui::Ui, db: &Connection, key: u64, value: &mut i64) {
    let rows = query(
        db,
        &format!(
            "SELECT id,code,name FROM tax_codes WHERE active=1 OR id={} ORDER BY id",
            *value
        ),
    )
    .unwrap_or_default();
    let selected = rows
        .iter()
        .find(|r| r[0] == value.to_string())
        .map(|r| r[1].as_str())
        .unwrap_or("NONE");
    egui::ComboBox::from_id_salt(("direct_tax", key))
        .width((ui.available_width() - 6.0).max(60.0))
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for row in &rows {
                if let Ok(id) = row[0].parse() {
                    ui.selectable_value(value, id, &row[1])
                        .on_hover_text(&row[2]);
                }
            }
        });
}

pub(super) fn finite_width(ui: &egui::Ui) -> f32 {
    let width = ui.available_width();
    if width.is_finite() {
        width
    } else {
        (ui.ctx().content_rect().width() - 64.0).max(300.0)
    }
}

pub(super) fn band(ui: &mut egui::Ui, data: &Value, number: &str) {
    let width = (finite_width(ui) - 20.0).max(200.0);
    egui::Frame::new()
        .fill(BLUE)
        .inner_margin(6)
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.horizontal_wrapped(|ui| {
                for text in [
                    format!("CUSTOMER  {}", data["customer"].as_str().unwrap_or("")),
                    format!("CURRENCY  {}", data["currency"].as_str().unwrap_or("")),
                    format!("DATE  {}", data["invoice_date"].as_str().unwrap_or("")),
                    format!("INVOICE #  {number}"),
                ] {
                    ui.label(RichText::new(text).color(Color32::WHITE));
                    ui.add_space(16.0);
                }
            });
        });
    ui.add_space(8.0);
}

// Real rows and visual filler are separate: blank stripes never become invoice lines.
pub(super) fn sheet(
    ui: &mut egui::Ui,
    id: &str,
    height: f32,
    editable: bool,
    count: usize,
    cell: impl FnMut(&mut egui::Ui, usize, usize),
) {
    let minimum = if editable { 980.0 } else { 760.0 };
    let width = finite_width(ui).max(minimum);
    let mut widths = if editable {
        vec![180.0, 52.0, 0.0, 48.0, 100.0, 110.0, 130.0, 32.0]
    } else {
        vec![90.0, 52.0, 0.0, 48.0, 100.0, 110.0, TAX_COLUMN_WIDTH]
    };
    widths[2] = width - widths.iter().sum::<f32>();
    let titles = if editable {
        vec![
            "ITEM",
            "QTY",
            "DESCRIPTION",
            "UNIT",
            "RATE",
            "AMOUNT",
            "TAX",
            "",
        ]
    } else {
        vec![
            "ITEM",
            "QTY",
            "DESCRIPTION",
            "UNIT",
            "RATE",
            "AMOUNT",
            "TAX",
        ]
    };
    sheet_columns(ui, id, height, &titles, &widths, count, cell);
}

pub(super) fn sheet_columns(
    ui: &mut egui::Ui,
    id: &str,
    height: f32,
    titles: &[&str],
    widths: &[f32],
    count: usize,
    cell: impl FnMut(&mut egui::Ui, usize, usize),
) {
    columns(ui, id, height, titles, widths, count, true, cell);
}

pub(super) fn register_columns(
    ui: &mut egui::Ui,
    id: &str,
    height: f32,
    titles: &[&str],
    widths: &[f32],
    count: usize,
    cell: impl FnMut(&mut egui::Ui, usize, usize),
) {
    columns(ui, id, height, titles, widths, count, false, cell);
}

#[allow(clippy::too_many_arguments)]
fn columns(
    ui: &mut egui::Ui,
    id: &str,
    height: f32,
    titles: &[&str],
    widths: &[f32],
    count: usize,
    fill: bool,
    mut cell: impl FnMut(&mut egui::Ui, usize, usize),
) {
    let width: f32 = widths.iter().sum();
    let height = height.max(84.0);
    ui.push_id(id, |ui| {
        let scroll = egui::ScrollArea::horizontal()
            .id_salt("horizontal")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.set_min_width(width);
                let (header, _) =
                    ui.allocate_exact_size(egui::vec2(width, 25.0), egui::Sense::hover());
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new(("sheet_header", id)), header));
                ui.painter().rect_filled(header, 0.0, Color32::WHITE);
                let mut x = header.left();
                for (column, (title, col_width)) in titles.iter().zip(widths).enumerate() {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(x + 5.0, header.top()),
                        egui::vec2((col_width - 10.0).max(1.0), header.height()),
                    );
                    let upper = title.to_ascii_uppercase();
                    let numeric = [
                        "AMOUNT", "AMT.", "PAYMENT", "DEBIT", "CREDIT", "BALANCE", "OPEN", "TOTAL",
                        "RATE", "QTY", "RECOV",
                    ]
                    .iter()
                    .any(|part| upper.contains(part));
                    text_cell(
                        ui,
                        ("header", column),
                        rect,
                        RichText::new(*title).small().color(theme::MUTED),
                        if numeric {
                            egui::Align::RIGHT
                        } else {
                            egui::Align::LEFT
                        },
                    )
                    .on_hover_text(*title);
                    x += col_width;
                }
                ui.painter().line_segment(
                    [header.left_bottom(), header.right_bottom()],
                    egui::Stroke::new(1.0_f32, LINE),
                );
                let filler = if fill {
                    (height / ROW_HEIGHT).ceil() as usize
                } else {
                    0
                };
                let rows = egui::ScrollArea::vertical()
                    .id_salt("rows")
                    .auto_shrink([false, false])
                    .min_scrolled_height(height)
                    .max_height(height)
                    .show_rows(ui, ROW_HEIGHT, count.max(filler), |ui, range| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for row in range {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(width, ROW_HEIGHT),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                0.0,
                                if row % 2 == 1 { STRIPE } else { Color32::WHITE },
                            );
                            let mut x = rect.left();
                            for (column, col_width) in widths.iter().enumerate() {
                                let cell_rect = egui::Rect::from_min_size(
                                    egui::pos2(x + 4.0, rect.top() + 2.0),
                                    egui::vec2(col_width - 8.0, ROW_HEIGHT - 4.0),
                                );
                                #[cfg(test)]
                                if row == 0 {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::new(("sheet_cell", id, column)),
                                            cell_rect,
                                        )
                                    });
                                }
                                if row < count {
                                    let mut child = ui.new_child(
                                        egui::UiBuilder::new()
                                            .id_salt((row, column))
                                            .max_rect(cell_rect)
                                            .layout(egui::Layout::left_to_right(
                                                egui::Align::Center,
                                            )),
                                    );
                                    child.set_clip_rect(cell_rect.intersect(ui.clip_rect()));
                                    child.spacing_mut().button_padding = egui::vec2(3.0, 2.0);
                                    child.spacing_mut().interact_size.y = 24.0;
                                    child.visuals_mut().widgets.inactive.bg_fill =
                                        Color32::TRANSPARENT;
                                    cell(&mut child, row, column);
                                }
                                ui.painter().line_segment(
                                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                                    egui::Stroke::new(1.0_f32, LINE),
                                );
                                x += col_width;
                            }
                            ui.painter().line_segment(
                                [rect.right_top(), rect.right_bottom()],
                                egui::Stroke::new(1.0_f32, LINE),
                            );
                        }
                    });
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new(("sheet_rows_viewport", id)), rows.inner_rect)
                });
                #[cfg(not(test))]
                let _ = rows;
                ui.separator();
            });
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new(("sheet_viewport", id)), scroll.inner_rect));
        #[cfg(not(test))]
        let _ = scroll;
    });
}

pub(super) fn amount(ui: &mut egui::Ui, value: &str) {
    let available = ui.available_width();
    let width = if available.is_finite() {
        available.min(240.0)
    } else {
        140.0
    };
    ui.allocate_ui_with_layout(
        egui::vec2(width, 24.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(value).truncate())
                .on_hover_text(value);
        },
    );
}
/// Bounded summary geometry also works inside grids/scroll areas with unbounded layout hints.
fn text_cell(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    rect: egui::Rect,
    text: impl Into<egui::WidgetText>,
    align: egui::Align,
) -> egui::Response {
    let mut cell = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect).layout(
        if align == egui::Align::RIGHT {
            egui::Layout::right_to_left(egui::Align::Center)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        },
    ));
    cell.set_clip_rect(rect.intersect(ui.clip_rect()));
    cell.add(egui::Label::new(text).truncate())
}
pub(super) fn money_summary(
    ui: &mut egui::Ui,
    title: &str,
    currency: &str,
    rows: &[(&str, Option<i64>)],
) {
    let width = finite_width(ui);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, 26.0 + rows.len() as f32 * 26.0),
        egui::Sense::hover(),
    );
    let left = (rect.right() - 380.0).max(rect.left());
    let value_width = (rect.right() - left).min(148.0);
    text_cell(
        ui,
        ("summary", title),
        egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(rect.right(), rect.top() + 24.0),
        ),
        RichText::new(title).small().color(theme::MUTED),
        egui::Align::LEFT,
    );
    for (index, (label, value)) in rows.iter().enumerate() {
        let top = rect.top() + 26.0 + index as f32 * 26.0;
        let label_rect = egui::Rect::from_min_max(
            egui::pos2(left, top),
            egui::pos2(rect.right() - value_width - 8.0, top + 24.0),
        );
        text_cell(
            ui,
            ("summary_label", index),
            label_rect,
            *label,
            egui::Align::LEFT,
        )
        .on_hover_text(*label);
        let value_rect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - value_width, top),
            egui::pos2(rect.right(), top + 24.0),
        );
        let text = value
            .map(|v| format!("{currency} {}", crate::format_money(v)))
            .unwrap_or_else(|| "--".into());
        text_cell(
            ui,
            ("summary_value", index),
            value_rect,
            text,
            egui::Align::RIGHT,
        );
    }
}

pub(super) fn snapshot_lines(ui: &mut egui::Ui, id: &str, data: &Value, height: f32) {
    let items = data["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    sheet(ui, id, height, false, items.len(), |ui, row, column| {
        let item = &items[row];
        let value = match column {
            0 => item["source_task_number"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| (row + 1).to_string()),
            1 => item["quantity"].to_string(),
            2 => item["description"].as_str().unwrap_or("").to_owned(),
            3 => item["unit"].as_str().unwrap_or("").to_owned(),
            4 => crate::format_money(item["unit_price"].as_i64().unwrap_or(0)),
            5 => crate::format_money(item["subtotal"].as_i64().unwrap_or(0)),
            _ => item["tax_snapshot"]["code"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
        };
        if matches!(column, 1 | 4 | 5) {
            amount(ui, &value);
        } else {
            ui.add(egui::Label::new(&value).truncate())
                .on_hover_text(&value);
        }
    });
}

pub(super) fn totals(
    ui: &mut egui::Ui,
    currency: &str,
    net: i64,
    tax: i64,
    total: i64,
    received: Option<i64>,
) {
    let mut rows = vec![("SUBTOTAL", net), ("TAX", tax), ("TOTAL", total)];
    if let Some(received) = received {
        rows.push(("PAYMENTS APPLIED", received));
        rows.push(("BALANCE DUE", total.saturating_sub(received)));
    }
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(finite_width(ui), rows.len() as f32 * 26.0),
        egui::Sense::hover(),
    );
    let amount_right = rect.right() - TAX_COLUMN_WIDTH - 4.0;
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new("invoice_summary_right"), rect.right());
        d.insert_temp(egui::Id::new("invoice_summary_amount_right"), amount_right);
    });
    for (index, (label, value)) in rows.iter().enumerate() {
        let y = rect.top() + index as f32 * 26.0 + 13.0;
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.painter().text(
            egui::pos2(amount_right - 206.0, y),
            egui::Align2::RIGHT_CENTER,
            label,
            font.clone(),
            theme::MUTED,
        );
        ui.painter().text(
            egui::pos2(amount_right - 186.0, y),
            egui::Align2::LEFT_CENTER,
            currency,
            font.clone(),
            theme::INK,
        );
        ui.painter().text(
            egui::pos2(amount_right, y),
            egui::Align2::RIGHT_CENTER,
            crate::format_money(*value),
            font,
            theme::INK,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invoice_grid_and_summary_share_right_edge_and_amount_alignment() {
        for width in [800.0, 1200.0, 1920.0] {
            let ctx = egui::Context::default();
            let data = serde_json::json!({"items":[{"description":"Service", "quantity":2, "unit":"ea", "unit_price":5000, "subtotal":10000, "tax_snapshot":{"code":"NONE"}}]});
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            compact(ui);
                            snapshot_lines(ui, "alignment_lines", &data, 280.0);
                            totals(ui, "CAD", 10000, 0, 10000, Some(0));
                        });
                    },
                );
            }
            let amount = ctx
                .data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new(("sheet_cell", "alignment_lines", 5)))
                })
                .unwrap();
            let tax = ctx
                .data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new(("sheet_cell", "alignment_lines", 6)))
                })
                .unwrap();
            let summary_right = ctx
                .data(|d| d.get_temp::<f32>(egui::Id::new("invoice_summary_right")))
                .unwrap();
            let summary_amount = ctx
                .data(|d| d.get_temp::<f32>(egui::Id::new("invoice_summary_amount_right")))
                .unwrap();
            assert!(
                (tax.right() + 4.0 - summary_right).abs() < 0.1,
                "Table/summary right edge mismatch at {width}"
            );
            assert!(
                (amount.right() - summary_amount).abs() < 0.1,
                "Amount column mismatch at {width}"
            );
        }
    }
    #[test]
    fn invoice_summary_uses_one_font_size_for_every_label_currency_and_amount() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                compact(ui);
                let styles = &ui.style().text_styles;
                for text_style in [
                    egui::TextStyle::Body,
                    egui::TextStyle::Button,
                    egui::TextStyle::Monospace,
                ] {
                    assert_eq!(styles[&text_style].size, 13.0);
                }
                totals(ui, "CAD", 10000, 0, 10000, Some(0));
            });
        });
        let text: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape {
                    Some(text)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text.len(), 15);
        for text in text {
            for section in &text.galley.job.sections {
                assert_eq!(section.format.font_id.size, 13.0);
            }
        }
    }

    #[test]
    fn invoice_sheet_keeps_columns_aligned_and_blank_rows_are_only_visual() {
        for width in [800.0, 1920.0] {
            let ctx = egui::Context::default();
            let mut cells = Vec::new();
            for _ in 0..3 {
                cells.clear();
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            sheet(ui, "test_lines", 300.0, true, 2, |ui, row, col| {
                                cells.push((row, col, ui.max_rect()));
                            });
                        });
                    },
                );
            }
            assert_eq!(cells.len(), 16);
            let description = cells.iter().find(|(r, c, _)| *r == 0 && *c == 2).unwrap().2;
            assert!(description.width() >= 210.0);
            if width > 1500.0 {
                assert!(description.width() > 800.0);
            }
            for col in 0..8 {
                let a = cells
                    .iter()
                    .find(|(r, c, _)| *r == 0 && *c == col)
                    .unwrap()
                    .2;
                let b = cells
                    .iter()
                    .find(|(r, c, _)| *r == 1 && *c == col)
                    .unwrap()
                    .2;
                assert_eq!(a.left(), b.left());
                assert!((b.top() - a.top() - ROW_HEIGHT).abs() < 0.1);
            }
        }
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                sheet(ui, "empty_lines", 300.0, false, 0, |_, _, _| {
                    panic!("filler must not create a data row")
                });
            });
        });
    }
}
