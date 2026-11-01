//! One navigation registry and one renderer for expanded and icon-only panes.
use super::*;
use icons::Icon;
use theme::{GREEN, NAV};

const ROW_HEIGHT: f32 = 40.0;
const FOOTER_HEIGHT: f32 = 52.0;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Workspace,
    Accounting,
    Settings,
}
#[derive(Clone, Copy)]
struct Item {
    page: usize,
    icon: Icon,
    group: Group,
}
const ITEMS: [Item; 11] = [
    Item {
        page: 0,
        icon: Icon::Tasks,
        group: Group::Workspace,
    },
    Item {
        page: 8,
        icon: Icon::Invoice,
        group: Group::Workspace,
    },
    Item {
        page: 1,
        icon: Icon::Customer,
        group: Group::Workspace,
    },
    Item {
        page: 2,
        icon: Icon::Vendor,
        group: Group::Workspace,
    },
    Item {
        page: 3,
        icon: Icon::Product,
        group: Group::Workspace,
    },
    Item {
        page: 6,
        icon: Icon::Bill,
        group: Group::Accounting,
    },
    Item {
        page: 13,
        icon: Icon::Journal,
        group: Group::Accounting,
    },
    Item {
        page: 14,
        icon: Icon::Ledger,
        group: Group::Accounting,
    },
    Item {
        page: 7,
        icon: Icon::Accounts,
        group: Group::Accounting,
    },
    Item {
        page: 11,
        icon: Icon::Tax,
        group: Group::Accounting,
    },
    Item {
        page: 16,
        icon: Icon::Settings,
        group: Group::Settings,
    },
];

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Preferences {
    #[serde(default)]
    sidebar_collapsed: bool,
}
pub(super) fn load_collapsed(path: &std::path::Path) -> bool {
    std::fs::read(path.with_extension("ui.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .unwrap_or_default()
        .sidebar_collapsed
}

fn item(ui: &mut egui::Ui, item: Item, selected: bool, compact: bool) -> egui::Response {
    let label = if item.group == Group::Settings {
        "Settings"
    } else {
        PAGES[item.page].title
    };
    let width = ui.available_width();
    let response = ui.add_sized(
        [width, ROW_HEIGHT],
        egui::Button::new("").frame(false).selected(selected),
    );
    let rect = response.rect.shrink2(egui::vec2(4.0, 0.0));
    let bg = if selected {
        Color32::from_rgb(43, 59, 70)
    } else if response.hovered() || response.has_focus() {
        Color32::from_rgb(36, 53, 64)
    } else {
        NAV
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 3.0, rect.center().y),
                egui::vec2(3.0, 20.0),
            ),
            1.5,
            GREEN,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            4.0,
            egui::Stroke::new(1.0_f32, Color32::from_rgb(125, 194, 169)),
            egui::StrokeKind::Inside,
        );
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(response.rect.left() + 28.0, response.rect.center().y),
        egui::vec2(20.0, 20.0),
    );
    let color = if selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(207, 220, 228)
    };
    icons::paint_color(ui, &response, icon_rect, item.icon, color);
    if !compact {
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(response.rect.left() + 52.0, response.rect.top()),
            egui::pos2(response.rect.right() - 12.0, response.rect.bottom()),
        );
        let galley = ui.fonts_mut(|f| {
            f.layout_no_wrap(label.to_owned(), egui::FontId::proportional(13.0), color)
        });
        ui.painter()
            .with_clip_rect(text_rect.intersect(ui.clip_rect()))
            .galley(
                egui::pos2(
                    text_rect.left(),
                    text_rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                color,
            );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            response.enabled(),
            selected,
            PAGES[item.page].title,
        )
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new(("nav_item", item.page)), response.rect);
        d.insert_temp(egui::Id::new(("nav_icon", item.page)), icon_rect);
        d.insert_temp(egui::Id::new(("nav_widget", item.page)), response.id);
    });
    response.on_hover_text(PAGES[item.page].title)
}

fn group_header(ui: &mut egui::Ui, label: &str, compact: bool) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    if compact {
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 16.0, rect.center().y),
                egui::pos2(rect.right() - 16.0, rect.center().y),
            ],
            egui::Stroke::new(1.0_f32, Color32::from_rgb(64, 82, 94)),
        );
    } else {
        ui.painter().text(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(142, 164, 176),
        );
    }
}

impl Desktop {
    fn set_navigation_collapsed(&mut self, collapsed: bool) {
        self.sidebar_collapsed = collapsed;
        let result = (|| -> Result<()> {
            let bytes = serde_json::to_vec_pretty(&Preferences {
                sidebar_collapsed: collapsed,
            })?;
            std::fs::write(self.path.with_extension("ui.json"), bytes)?;
            Ok(())
        })();
        if let Err(error) = result {
            self.message = format!("Menu changed; its preference could not be saved: {error}");
            self.error = false;
        }
    }
    pub(super) fn navigation(&mut self, ctx: &egui::Context) {
        let compact = self.sidebar_collapsed;
        egui::SidePanel::left("navigation")
            .exact_width(if compact { 56.0 } else { 220.0 })
            .resizable(false)
            .frame(egui::Frame::new().fill(NAV))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let toggle = icons::navigation_toggle(ui, compact);
                    #[cfg(test)]
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("nav_toggle"), toggle.rect));
                    if toggle.clicked() {
                        self.set_navigation_collapsed(!compact);
                    }
                    if !compact {
                        ui.add(
                            egui::Label::new(
                                RichText::new(crate::APP_NAME)
                                    .size(13.0)
                                    .color(Color32::WHITE),
                            )
                            .truncate(),
                        )
                        .on_hover_text(concat!("Axibiz Desktop Beta v", env!("CARGO_PKG_VERSION")));
                    }
                });
                ui.add_space(4.0);
                let height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("navigation_items")
                    .auto_shrink([false, false])
                    .min_scrolled_height(height)
                    .max_height(height)
                    .show(ui, |ui| {
                        for (group, label) in [
                            (Group::Workspace, "WORKSPACE"),
                            (Group::Accounting, "ACCOUNTING"),
                        ] {
                            group_header(ui, label, compact);
                            for entry in ITEMS.iter().copied().filter(|i| i.group == group) {
                                if item(ui, entry, self.page == entry.page, compact).clicked() {
                                    self.navigate(entry.page);
                                }
                            }
                        }
                    });
                ui.add_space(4.0);
                let settings = ITEMS[10];
                if item(ui, settings, self.page == settings.page, compact).clicked() {
                    self.navigate(settings.page);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Result<Desktop> {
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let token = crate::posting::request_id(&db)?;
        Desktop::new(
            db,
            std::env::temp_dir().join(format!("axibiz-navigation-{token}.db")),
        )
    }
    fn frame(app: &mut Desktop, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.navigation(ctx),
        )
    }
    fn click(app: &mut Desktop, ctx: &egui::Context, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    fn rect(ctx: &egui::Context, page: usize) -> egui::Rect {
        ctx.data(|d| d.get_temp(egui::Id::new(("nav_item", page))))
            .unwrap()
    }
    #[test]
    fn both_modes_keep_all_routes_and_navigation_preserves_open_drafts_and_queries() -> Result<()> {
        let mut app = app()?;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.list_query = "Retain search".into();
        app.selected = 42;
        app.new_task_open = true;
        app.task_title = "Unsaved task".into();
        let changes = app.db.total_changes();
        for compact in [false, true] {
            app.sidebar_collapsed = compact;
            ctx.data_mut(|d| {
                for entry in ITEMS {
                    d.remove::<egui::Rect>(egui::Id::new(("nav_item", entry.page)));
                }
            });
            for _ in 0..3 {
                frame(&mut app, &ctx, vec![]);
            }
            assert_eq!(
                ITEMS
                    .iter()
                    .map(|i| i.page)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                11
            );
            assert_eq!(
                ITEMS
                    .iter()
                    .map(|i| i.icon)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                11
            );
            for entry in ITEMS {
                let target = rect(&ctx, entry.page);
                assert!(
                    target.left() >= 0.0 && target.right() <= if compact { 56.0 } else { 220.0 }
                );
                assert_eq!(target.height(), ROW_HEIGHT);
                assert!(
                    target.bottom() <= 650.0,
                    "Route {} must remain reachable",
                    entry.page
                );
            }
            click(&mut app, &ctx, rect(&ctx, 7).center());
            assert_eq!(app.page, 7, "compact={compact}");
            click(&mut app, &ctx, rect(&ctx, 0).center());
            assert_eq!(app.page, 0);
            assert_eq!(app.list_query, "Retain search");
            assert_eq!(app.selected, 42);
            assert!(app.new_task_open);
            assert_eq!(app.task_title, "Unsaved task");
        }
        assert_eq!(
            app.db.total_changes(),
            changes,
            "Navigation is not a financial write"
        );
        Ok(())
    }
    #[test]
    fn toggle_is_anchored_and_preference_survives_restart_and_restore() -> Result<()> {
        let mut app = app()?;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let toggle: egui::Rect = ctx
            .data(|d| d.get_temp(egui::Id::new("nav_toggle")))
            .unwrap();
        let icon: egui::Rect = ctx
            .data(|d| d.get_temp(egui::Id::new(("nav_icon", 0))))
            .unwrap();
        click(&mut app, &ctx, toggle.center());
        assert!(app.sidebar_collapsed);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("nav_toggle")))
                .unwrap(),
            toggle
        );
        assert_eq!(
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("nav_icon", 0))))
                .unwrap()
                .center()
                .x,
            icon.center().x
        );
        assert!(load_collapsed(&app.path));
        let db = Connection::open_in_memory()?;
        crate::init(&db)?;
        let restarted = Desktop::new(db, app.path.clone())?;
        assert!(restarted.sidebar_collapsed);
        app.reset_after_restore()?;
        assert!(app.sidebar_collapsed);
        click(&mut app, &ctx, toggle.center());
        assert!(!app.sidebar_collapsed);
        assert!(!load_collapsed(&app.path));
        std::fs::write(app.path.with_extension("ui.json"), b"partial JSON")?;
        assert!(
            !load_collapsed(&app.path),
            "Invalid UI metadata falls back to expanded navigation"
        );
        std::fs::remove_file(app.path.with_extension("ui.json"))?;
        Ok(())
    }
    #[test]
    fn icon_only_navigation_supports_keyboard_activation() -> Result<()> {
        let mut app = app()?;
        app.sidebar_collapsed = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let id = ctx
            .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("nav_widget", 13))))
            .unwrap();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.page, 13);
        Ok(())
    }
}
