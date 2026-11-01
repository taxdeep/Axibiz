//! Optional, local raster evidence produced from the real desktop widgets.
use super::*;
use serde_json::json;

pub(super) fn capture(
    app: &mut Desktop,
    name: &str,
    width: f32,
    height: f32,
    native: bool,
    draw: fn(&mut Desktop, &egui::Context),
) -> Result<()> {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let viewport = if native {
        egui::ViewportId::from_hash_of("native-review")
    } else {
        egui::ViewportId::ROOT
    };
    let mut textures = Vec::new();
    let mut last = None;
    for frame in 0..if name == "transactions-menu" { 12 } else { 5 } {
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            viewport,
            egui::ViewportInfo {
                parent: native.then_some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let events = if name == "transactions-menu" && (frame == 1 || frame == 2) {
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("transactions_menu_rect")))
                .map(|rect| {
                    vec![
                        egui::Event::PointerMoved(rect.center()),
                        egui::Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed: frame == 1,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                })
                .unwrap_or_default()
        } else {
            vec![]
        };
        let output = ctx.run(
            egui::RawInput {
                viewport_id: viewport,
                viewports,
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                ..Default::default()
            },
            |ctx| draw(app, ctx),
        );
        for (_, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(image) = &delta.image;
            textures.push(json!({"size":image.size,"pos":delta.pos,"pixels":image.pixels.iter().flat_map(|p|p.to_array()).collect::<Vec<_>>()}));
        }
        last = Some(output);
    }
    if !native && [1, 2].contains(&app.page) {
        let scope = if app.page == 1 { "customer" } else { "vendor" };
        let title = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(format!("{scope}_title_rect"))))
            .unwrap();
        let balance = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(format!("{scope}_balance_rect"))))
            .unwrap();
        assert!(
            balance.left() > title.right(),
            "The balance must be grouped at the right, not placed beside the name: {name}"
        );
        assert!(
            balance.right() <= width - 16.0 && title.left() >= 0.0,
            "Overview must fit {name}"
        );
    }
    if name.starts_with("company-login") {
        let submit = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("login_submit_rect")))
            .unwrap();
        assert!(
            submit.bottom() < height && submit.right() < width,
            "Login action must fit the window"
        );
        assert!(
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("nav_toggle")))
                .is_none(),
            "Company content must not render while signed out"
        );
    }
    if name.starts_with("company-users") {
        let save = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("users_save_rect")))
            .unwrap();
        assert!(
            save.bottom() < height && save.right() < width,
            "User save action must remain visible"
        );
    }
    if name.starts_with("software-settings") {
        let save = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("software_save_rect")))
            .unwrap();
        assert!(
            save.bottom() < height && save.left() > 0.0 && save.right() < width,
            "Software save action must remain visible: {name}"
        );
    }
    if name.starts_with("company-settings") {
        let save = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("company_save_rect")))
            .unwrap();
        assert!(
            save.bottom() < height && save.left() > 0.0 && save.right() < width,
            "Company save action must remain visible: {name}"
        );
    }
    if name.ends_with("-review") {
        let (id, label) = if name.starts_with("receive") {
            ("receipt_review", "Customer")
        } else {
            ("supplier_payment_review", "Supplier")
        };
        // Protect the review's readable columns and final action, not just the underlying form.
        let values =
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new((id, "review_value", label))));
        let confirm = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new((id, "review_confirm"))));
        if let (Some(value), Some(confirm)) = (values, confirm) {
            assert!(
                value.width() >= 250.0,
                "Review values must not wrap into a few characters: {name}"
            );
            assert!(
                confirm.bottom() < height && confirm.right() < width,
                "Review confirmation must remain visible: {name}"
            );
        } else {
            panic!("Missing review controls for {name}")
        }
    }
    if let Some(path) = std::env::var_os("AXIBIZ_NATIVE_PREVIEW") {
        let output = last.unwrap();
        let meshes=ctx.tessellate(output.shapes,output.pixels_per_point).into_iter().filter_map(|p|{
            if let egui::epaint::Primitive::Mesh(mesh)=p.primitive {Some(json!({"clip":[p.clip_rect.min.x,p.clip_rect.min.y,p.clip_rect.max.x,p.clip_rect.max.y],"indices":mesh.indices,"vertices":mesh.vertices.iter().map(|v|json!([v.pos.x,v.pos.y,v.uv.x,v.uv.y,v.color.to_array()])).collect::<Vec<_>>()}))}else{None}
        }).collect::<Vec<_>>();
        let folder = PathBuf::from(path);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join(format!("{name}.json")),
            serde_json::to_vec(
                &json!({"width":width,"height":height,"textures":textures,"meshes":meshes}),
            )?,
        )?;
    }
    Ok(())
}

#[test]
fn actual_vendor_bill_and_payment_widgets_render_without_changing_records() -> Result<()> {
    let mut db = Connection::open_in_memory()?;
    crate::init(&db)?;
    let account = crate::accounting::default_account(&db)?;
    let other: i64 = db.query_row("SELECT id FROM accounts WHERE code='5400'", [], |r| {
        r.get(0)
    })?;
    let mut vendor = crate::vendor::Vendor {
        name: "Example Supplier LLC".into(),
        ..Default::default()
    };
    vendor.profile.company = "Example Supplier LLC".into();
    vendor.profile.contact_name = "Alex Taylor".into();
    vendor.profile.email = "accounts@supplier.example".into();
    vendor.profile.phone = "+1 604 555 0128".into();
    vendor.profile.address = "240 Main Street\nVancouver, BC V6B 1A1\nCanada".into();
    vendor.profile.payment_terms = "Net 30".into();
    vendor.profile.default_accounts = vec![account, other];
    vendor.profile.notes = "Include the supplier reference with supporting documents.".into();
    vendor.id = crate::vendor::save(&mut db, &vendor)?;
    let mut first = 0;
    for (reference, amount, memo) in [
        ("C-2026-10", "1250", "Professional services"),
        ("C-2026-11", "2150", "Annual service renewal"),
    ] {
        let document = crate::expense_doc::Document {
            id: 0,
            vendor: Some(vendor.id),
            date: crate::today().to_string(),
            currency: "CAD".into(),
            reference: reference.into(),
            due_date: Some((crate::today() + chrono::Duration::days(30)).to_string()),
            memo: memo.into(),
            terms: "Net 30".into(),
            lines: vec![crate::accounting::Expense {
                id: 0,
                date: crate::today().to_string(),
                description: memo.into(),
                account: other,
                vendor: Some(vendor.id),
                currency: "CAD".into(),
                amount: amount.into(),
                task: None,
                tax: Default::default(),
            }],
        };
        let id = crate::expense_doc::save(&mut db, &document)?;
        if first == 0 {
            first = id
        }
    }
    for name in [
        "COSCO Shipping",
        "InCorp",
        "North Shore Supplies",
        "West Coast Services",
    ] {
        crate::vendor::save(
            &mut db,
            &crate::vendor::Vendor {
                name: name.into(),
                ..Default::default()
            },
        )?;
    }
    let bank = crate::gl::role(&db, "bank")?;
    crate::ap::configure_bank(&mut db, bank, "CAD")?;
    let mut customer = crate::customer::Customer {
        name: "Example Customer Ltd.".into(),
        ..Default::default()
    };
    customer.profile.contact_name = "Jordan Smith".into();
    customer.profile.email = "sarah@example.test".into();
    customer.profile.billing_address = "100 Main Street\nVancouver, BC\nCanada".into();
    customer.id = crate::customer::save(&mut db, &customer)?;
    let invoice = crate::billing::DirectInvoice {
        request_key: "preview-customer-invoice".into(),
        customer: customer.id,
        date: crate::today().to_string(),
        currency: "CAD".into(),
        title: "Annual services".into(),
        lines: vec![crate::billing::DirectLine {
            price_id: 0,
            description: "Annual report".into(),
            unit: "item".into(),
            quantity: 1,
            unit_price: Some(10000),
            tax_code: Some(0),
        }],
    };
    let preview = crate::billing::direct_preview(&db, &invoice)?;
    let invoice = crate::billing::direct_confirm(&mut db, &invoice, &preview)?;
    let mut app = Desktop::new(db, PathBuf::from("native-design-preview.db"))?;
    app.page = 2;
    let changes = app.db.total_changes();
    capture(
        &mut app,
        "vendor-center",
        1440.0,
        900.0,
        false,
        Desktop::draw,
    )?;
    capture(
        &mut app,
        "vendor-center-small",
        900.0,
        650.0,
        false,
        Desktop::draw,
    )?;
    app.open_vendor_editor(vendor.id);
    capture(
        &mut app,
        "vendor-editor",
        1100.0,
        740.0,
        true,
        Desktop::vendor_dialog,
    )?;
    app.vendor_ui.form = None;
    app.open_vendor_expense(&vendor);
    capture(
        &mut app,
        "bill-entry",
        1200.0,
        800.0,
        true,
        Desktop::accounting_dialog,
    )?;
    app.accounting.form = None;
    app.open_pay_bills(vendor.id, Some(first));
    capture(
        &mut app,
        "pay-bills",
        1200.0,
        800.0,
        true,
        Desktop::pay_bills_dialog,
    )?;
    capture(
        &mut app,
        "pay-bills-small",
        900.0,
        650.0,
        true,
        Desktop::pay_bills_dialog,
    )?;
    app.review_supplier_payment(false);
    capture(
        &mut app,
        "pay-bills-review",
        900.0,
        650.0,
        true,
        Desktop::pay_bills_dialog,
    )?;
    app.pay_bills = None;
    app.page = 1;
    for (name, width, height) in [
        ("customer-center", 1440.0, 900.0),
        ("customer-center-small", 900.0, 650.0),
    ] {
        capture(&mut app, name, width, height, false, Desktop::draw)?;
    }
    app.sidebar_collapsed = true;
    capture(
        &mut app,
        "customer-center-compact",
        1440.0,
        900.0,
        false,
        Desktop::draw,
    )?;
    app.page = 2;
    capture(
        &mut app,
        "vendor-center-compact",
        1440.0,
        900.0,
        false,
        Desktop::draw,
    )?;
    app.sidebar_collapsed = false;
    app.page = 1;
    app.open_customer_editor(customer.id);
    capture(
        &mut app,
        "customer-editor",
        1100.0,
        740.0,
        true,
        Desktop::customer_dialog,
    )?;
    app.customer_ui.form = None;
    app.open_payment(invoice);
    capture(
        &mut app,
        "receive-payment",
        1200.0,
        800.0,
        true,
        Desktop::payment_dialog,
    )?;
    capture(
        &mut app,
        "receive-payment-small",
        900.0,
        650.0,
        true,
        Desktop::payment_dialog,
    )?;
    app.review_receipt_close();
    capture(
        &mut app,
        "receive-payment-review",
        900.0,
        650.0,
        true,
        Desktop::payment_dialog,
    )?;
    app.payment = None;
    for (page, name) in [(1, "customer-empty"), (2, "vendor-empty")] {
        app.page = page;
        if page == 1 {
            app.customer_ui.browser.transaction_query = "No matching record".into()
        } else {
            app.vendor_ui.browser.transaction_query = "No matching record".into()
        }
        capture(&mut app, name, 1440.0, 900.0, false, Desktop::draw)?;
    }
    app.page = 5;
    app.settings.company_name = "Example Services Ltd".into();
    app.settings.address = "240 Main Street\nVancouver, BC V6B 1A1".into();
    app.settings.profile.phone = "+1 604 555 0128".into();
    app.settings.profile.email = "accounts@example.com".into();
    for (name, width, height) in [
        ("company-settings", 1440.0, 900.0),
        ("company-settings-small", 900.0, 650.0),
    ] {
        capture(&mut app, name, width, height, false, Desktop::draw)?;
    }
    app.settings = app.saved_settings.clone();
    app.page = 16;
    for (name, width, height) in [
        ("software-settings", 1440.0, 900.0),
        ("software-settings-small", 900.0, 650.0),
    ] {
        capture(&mut app, name, width, height, false, Desktop::draw)?;
    }
    app.page = 0;
    app.sidebar_collapsed = false;
    capture(
        &mut app,
        "transactions-menu",
        900.0,
        650.0,
        false,
        Desktop::draw,
    )?;
    assert_eq!(
        app.db.total_changes(),
        changes,
        "Rendering must not record a payment or mutate supplier data."
    );
    Ok(())
}
