//! The phone layout (narrow windows, the Android app): app bar, navigation bar, sheets, Back,
//! and the in-app file browser that stands in for file dialogs on Android.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use printcraft_ui_egui::compact::Sheet;
use printcraft_ui_egui::pick::{PickKind, PickRequest};
use printcraft_ui_egui::{PrintCraftApp, QuickTool, RightPanel};

/// Two pages, two bookmarks and a sticky note.
const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Annots [9 0 R] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >> endobj
6 0 obj << /Type /Outlines /First 7 0 R /Last 8 0 R /Count 2 >> endobj
7 0 obj << /Title (Alpha section) /Parent 6 0 R /Next 8 0 R /Dest [3 0 R /Fit] >> endobj
8 0 obj << /Title (Beta section) /Parent 6 0 R /Prev 7 0 R /Dest [4 0 R /Fit] >> endobj
9 0 obj << /Type /Annot /Subtype /Text /Rect [10 370 30 390] /T (Tester) /Contents (Check the numbers) >> endobj
trailer << /Root 1 0 R >>
%%EOF";

/// A phone-sized window (points).
fn phone(setup: impl FnOnce(&mut PrintCraftApp) + 'static) -> Harness<'static, PrintCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 860.0)).build_eframe(move |_cc| {
        let mut app = PrintCraftApp::new();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

fn with_doc(app: &mut PrintCraftApp) {
    app.open_bytes("fixture.pdf", None, FIXTURE.to_vec()).expect("fixture opens");
}

#[test]
fn narrow_windows_use_the_phone_layout() {
    let h = phone(|_| {});
    assert!(h.state().compact);
    // No desktop chrome: no mode bar tabs, no tool panel.
    assert!(h.query_by_label("All tools").is_none());
    h.get_by_label("Menu");
    h.get_by_label("Open a file");
    h.get_by_label_contains("Welcome to PrintCraft");
    let wide = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_| PrintCraftApp::new());
    let mut wide = wide;
    wide.run_steps(4);
    assert!(!wide.state().compact);
}

#[test]
fn a_document_opens_fit_to_width_with_the_hand_tool_and_no_panel() {
    let h = phone(with_doc);
    let app = h.state();
    assert_eq!(app.views[0].fit, printcraft_ui_egui::canvas::Fit::Width);
    assert_eq!(app.quick_tool, QuickTool::Hand);
    assert_eq!(app.phone_sheet, None, "the Comments panel would cover the page");
    h.get_by_label("Page 1 / 2");
    for item in ["Tools", "Comment", "Pages", "Panels"] {
        h.get_by_label(item);
    }
    // The navigation bar's and the quick bar's.
    assert_eq!(h.query_all_by_label("Fill & Sign").count(), 2);
}

#[test]
fn navigation_bar_opens_and_closes_sheets() {
    let mut h = phone(with_doc);
    h.get_by_label("Comment").click();
    h.run_steps(3);
    assert_eq!(h.state().phone_sheet, Some(Sheet::Tools));
    h.get_by_label("Pages").click();
    h.run_steps(3);
    assert_eq!(h.state().phone_sheet, Some(Sheet::Panel));
    assert_eq!(h.state().right, Some(RightPanel::Pages));
}

#[test]
fn a_bookmark_tap_goes_there_and_closes_the_sheet() {
    let mut h = phone(|app| {
        with_doc(app);
        app.set_option("sheet", "bookmarks").unwrap();
    });
    h.run_steps(3);
    h.get_by_label("Beta section").click();
    h.run_steps(4);
    assert_eq!(h.state().phone_sheet, None);
    h.get_by_label("Page 2 / 2");
}

#[test]
fn picking_a_tool_closes_the_sheet() {
    let mut h = phone(|app| {
        with_doc(app);
        app.set_option("sheet", "comment").unwrap();
    });
    h.run_steps(3);
    assert_eq!(h.state().phone_sheet, Some(Sheet::Tools));
    h.state_mut().run_command("comment.highlight");
    h.run_steps(3);
    assert_eq!(h.state().phone_sheet, None);
}

#[test]
fn back_closes_layers_one_at_a_time() {
    let mut h = phone(|app| {
        with_doc(app);
        app.set_option("sheet", "pages").unwrap();
    });
    h.run_steps(3);
    let back = |h: &mut Harness<'static, PrintCraftApp>| {
        h.key_press(egui::Key::BrowserBack);
        h.run_steps(3);
    };
    back(&mut h);
    assert_eq!(h.state().phone_sheet, None);
    assert!(h.state().active.is_some());
    back(&mut h);
    assert!(h.state().active.is_none(), "back to Home");
    assert_eq!(h.state().views.len(), 1, "the document stays open");
}

#[test]
fn dialogs_fit_the_screen() {
    let mut h = phone(|app| {
        with_doc(app);
        app.set_option("dialog", "properties").unwrap();
    });
    h.run_steps(3);
    let title = h.get_by_label("Document Properties");
    let r = title.rect();
    assert!(r.left() >= 0.0 && r.right() <= 400.0, "title at {r:?}");
}

#[test]
fn file_browser_opens_a_file_and_reruns_the_command() {
    let dir = std::env::temp_dir().join(format!("printcraft-phone-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fixture.pdf"), FIXTURE).unwrap();
    let d = dir.clone();
    let mut h = phone(move |app| {
        app.browse_dir = Some(d);
        let mut req = PickRequest::new(PickKind::File);
        req.filters.push(("PDF".into(), vec!["pdf".into()]));
        app.open_browser(req);
    });
    h.run_steps(3);
    h.get_by_label("fixture.pdf").click();
    h.run_steps(4);
    // Asked from no command: the answer waits for the dialog's next call.
    assert!(h.state().browser.is_none());
    assert!(h.state().toast.as_ref().is_some_and(|(m, _)| m.starts_with("Chosen")));
    let picked = printcraft_ui_egui::pick::Picker::new().add_filter("PDF", &["pdf"]).pick_file();
    assert_eq!(picked, Some(dir.join("fixture.pdf")));
    assert_eq!(h.state().browse_dir.as_deref(), Some(dir.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}
