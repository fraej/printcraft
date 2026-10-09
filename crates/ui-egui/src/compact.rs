//! The phone layout: one column, sized for fingers.
//!
//! Below [`COMPACT_WIDTH`] points the desktop chrome (tab strip, mode bar, side panels, right
//! rail) gives way to an app bar at the top, a navigation bar at the bottom and one sheet at a
//! time that slides up over the lower part of the page: the tool panel (All tools and each
//! tool) or one of the side panels (Comments, Bookmarks, Pages…). Everything else, from the
//! canvas to the dialogs, is the same code as on the desktop.

use egui::{Align, Align2, CornerRadius, Layout, Rect, Sense, Stroke, vec2};

use crate::theme::{self, ThemeKind, Tokens};
use crate::{LeftPanel, PrintCraftApp, QuickTool, RightPanel, icons};

/// Windows narrower than this (in points) use the phone layout.
pub const COMPACT_WIDTH: f32 = 600.0;
const BAR_H: f32 = 52.0;
const NAV_H: f32 = 62.0;
/// Touch targets (Material's minimum is 48 dp; 44 keeps five fit across a small phone).
const TAP: f32 = 44.0;

/// What the sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    /// The tool panel (`app.left`).
    Tools,
    /// A side panel (`app.right`).
    Panel,
}

/// The panel state the phone layout last saw, to notice panels opened elsewhere (the
/// palette, Home's tools, commands) and show them in the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    left: LeftPanel,
    left_open: bool,
    right: Option<RightPanel>,
    tool: QuickTool,
}

impl PrintCraftApp {
    /// Whether this frame uses the phone layout (the window's width, unless overridden).
    pub fn compact_for(&self, ctx: &egui::Context) -> bool {
        self.compact_override.unwrap_or_else(|| ctx.content_rect().width() < COMPACT_WIDTH)
    }

    /// Open the sheet on a tool panel (`None`: whatever the tool panel shows now).
    pub fn open_tool_sheet(&mut self, tool: Option<&'static str>) {
        if let Some(id) = tool {
            self.left = LeftPanel::Tool(id);
        } else if !self.left_open {
            self.left = LeftPanel::AllTools;
        }
        self.left_open = true;
        self.phone_sheet = Some(Sheet::Tools);
    }

    /// Open the sheet on a side panel.
    pub fn open_panel_sheet(&mut self, panel: RightPanel) {
        self.right = Some(panel);
        self.phone_sheet = Some(Sheet::Panel);
    }

    /// A phone reads a page by its width and pans it with a finger.
    pub(crate) fn phone_defaults(&mut self) {
        for v in &mut self.views {
            v.fit = crate::canvas::Fit::Width;
            v.side = 8.0;
        }
        if self.quick_tool == QuickTool::Select {
            self.quick_tool = QuickTool::Hand;
        }
    }

    /// Android's Back: close whatever is on top, then leave the document, then the app.
    pub fn back(&mut self, ctx: &egui::Context) {
        if self.browser.is_some() {
            self.browser = None;
        } else if self.password_prompt.is_some() {
            self.submit_password(None);
        } else if self.close_request.is_some() {
            self.resolve_close(ctx, None);
        } else if self.palette_open {
            self.palette_open = false;
        } else if self.dialog.is_some() {
            self.dialog = None;
        } else if self.phone_sheet.is_some() {
            self.phone_sheet = None;
        } else if let Some(v) = self.active.and_then(|i| self.views.get_mut(i)).filter(|v| v.find.is_some()) {
            v.find = None;
        } else if self.full_screen {
            self.set_full_screen(ctx, false);
        } else if !matches!(self.quick_tool, QuickTool::Hand | QuickTool::Select) {
            self.quick_tool = QuickTool::Hand;
        } else if self.active.is_some() {
            self.active = None;
        } else {
            // Like closing the window: documents with unsaved changes ask first.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl PrintCraftApp {
    /// Keep the shell clear of the system bars (Android draws the app under them): plain dark
    /// strips, on which the system's white icons stay readable.
    pub(crate) fn system_bars(&self, ui: &mut egui::Ui) {
        let Some(p) = &self.platform else { return };
        let ppp = ui.ctx().pixels_per_point().max(0.1);
        let [l, t, r, b] = p.insets().map(|v| if v.is_finite() { (v / ppp).clamp(0.0, 400.0) } else { 0.0 });
        let frame = egui::Frame::NONE.fill(egui::Color32::BLACK);
        if t > 0.0 {
            egui::Panel::top("system_top").exact_size(t).frame(frame).show(ui, |_| {});
        }
        if b > 0.0 {
            egui::Panel::bottom("system_bottom").exact_size(b).frame(frame).show(ui, |_| {});
        }
        if l > 0.0 {
            egui::Panel::left("system_left").resizable(false).exact_size(l).frame(frame).show(ui, |_| {});
        }
        if r > 0.0 {
            egui::Panel::right("system_right").resizable(false).exact_size(r).frame(frame).show(ui, |_| {});
        }
    }
}

/// Whether the phone layout is on (for widgets that see only the context).
pub fn is_on(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new("printcraft-compact"))).unwrap_or(false)
}

/// Apply or remove the touch sizes (after `theme::apply`, which resets them).
pub fn style(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("printcraft-compact"), on));
    if !on {
        return;
    }
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 9.0);
        s.spacing.interact_size = vec2(44.0, 36.0);
        s.spacing.icon_width = 20.0;
        s.spacing.icon_width_inner = 11.0;
        s.spacing.menu_margin = egui::Margin::same(8);
        s.spacing.combo_width = 140.0;
        s.spacing.slider_width = 160.0;
        s.spacing.scroll.bar_width = 6.0;
        s.text_styles.insert(egui::TextStyle::Body, theme::regular(15.0));
        s.text_styles.insert(egui::TextStyle::Button, theme::regular(15.0));
        s.text_styles.insert(egui::TextStyle::Small, theme::regular(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, theme::semibold(18.0));
        // Fingers wobble: a short drag is still a tap.
        s.interaction.tooltip_delay = 0.6;
    });
}

/// The phone layout for one frame (the caller draws dialogs, the palette and toasts after).
pub fn show(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    sync_sheet(app);
    app_bar(app, ui);
    if app.active.is_some() {
        nav_bar(app, ui);
    }
    sheet(app, ui);
    let t = Tokens::get(&ctx);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.pasteboard)).show(ui, |ui| match app.active {
        None => crate::home::show(app, ui),
        Some(i) => crate::canvas::document_area(app, i, ui),
    });
    app.phone_seen = Some(Seen { left: app.left, left_open: app.left_open, right: app.right, tool: app.quick_tool });
}

/// Panels opened since the last frame (by a command, the palette, Home…) open in the sheet;
/// a tool picked since then closes it.
fn sync_sheet(app: &mut PrintCraftApp) {
    let Some(seen) = app.phone_seen else { return };
    // A tool picked from the palette or a command: the page is what matters now.
    if app.quick_tool != seen.tool && app.phone_sheet == Some(Sheet::Tools) {
        app.phone_sheet = None;
        return;
    }
    if app.right.is_some() && app.right != seen.right {
        app.phone_sheet = Some(Sheet::Panel);
    } else if app.left_open && (app.left != seen.left || !seen.left_open) && app.left != LeftPanel::AllTools {
        app.phone_sheet = Some(Sheet::Tools);
    }
}

fn app_bar(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("phone_bar")
        .exact_size(BAR_H)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(4, 0)).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let doc = app.active.and_then(|i| app.views.get(i)).and_then(|v| app.session.get(v.id)).map(|d| (d.display_name(), d.dirty));
                match doc {
                    Some(_) => {
                        if icons::button(ui, "chevron-left", TAP, false, "Home").clicked() {
                            app.active = None;
                            app.phone_sheet = None;
                        }
                    }
                    None => {
                        ui.add_space(8.0);
                        crate::widgets::artcraft_mark(ui, 24.0);
                        ui.add_space(8.0);
                    }
                }
                // Right to left: menu, save, find, page; the title takes what is left.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let menu = icons::button(ui, "ellipsis-vertical", TAP, false, "Menu");
                    egui::Popup::menu(&menu).show(|ui| {
                        ui.set_min_width(220.0);
                        overflow_menu(app, ui);
                    });
                    match app.active {
                        Some(i) => {
                            if icons::button(ui, "save", TAP, false, "Save").clicked() {
                                app.run_command("file.save");
                            }
                            if icons::button(ui, "search", TAP, false, "Find in document").clicked()
                                && let Some(v) = app.views.get_mut(i)
                            {
                                if v.find.is_some() {
                                    v.find = None;
                                } else {
                                    v.open_find();
                                }
                            }
                            page_chip(app, i, ui);
                        }
                        None => {
                            if icons::button(ui, "search", TAP, false, "Find tools and commands").clicked() {
                                app.palette_open = true;
                            }
                            if icons::button(ui, "folder-open", TAP, false, "Open a file").clicked() {
                                app.run_command("file.open");
                            }
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let (name, dirty) = doc.unwrap_or_else(|| ("PrintCraft".to_string(), false));
                        title(app, ui, &t, &name, dirty);
                    });
                });
            });
        });
}

/// The document's name; tapping it lists the open documents.
fn title(app: &mut PrintCraftApp, ui: &mut egui::Ui, t: &Tokens, name: &str, dirty: bool) {
    let w = ui.available_width().max(40.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BAR_H - 8.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), t.pressed);
    }
    let font = theme::semibold(16.0);
    let max = rect.width() - if dirty { 22.0 } else { 8.0 };
    let galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), font, t.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(max.max(20.0));
        f.layout_job(job)
    });
    let pos = rect.left_center() + vec2(4.0, -galley.size().y / 2.0);
    let end = pos.x + galley.size().x;
    ui.painter().galley(pos, galley, t.text);
    if dirty {
        ui.painter().circle_filled(egui::pos2(end + 9.0, rect.center().y), 4.0, t.text_muted);
    }
    if app.views.len() < 2 && app.active.is_some() {
        return;
    }
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(240.0);
        if ui.add(egui::Button::new("Home").min_size(vec2(0.0, TAP)).selected(app.active.is_none())).clicked() {
            app.active = None;
        }
        let mut close = None;
        for i in 0..app.views.len() {
            let Some(doc) = app.session.get(app.views[i].id) else { continue };
            let label = if doc.dirty { format!("{} •", doc.display_name()) } else { doc.display_name() };
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(label).min_size(vec2(180.0, TAP)).selected(app.active == Some(i))).clicked() {
                    app.active = Some(i);
                    ui.close();
                }
                if icons::button(ui, "x", TAP, false, "Close").clicked() {
                    close = Some(i);
                }
            });
        }
        if let Some(i) = close {
            app.request_close_tab(i);
            ui.close();
        }
    });
}

/// "3 / 12": tapping it offers previous, next and go to page.
fn page_chip(app: &mut PrintCraftApp, index: usize, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(doc) = app.views.get(index).and_then(|v| app.session.get(v.id)) else { return };
    let labels: Vec<String> = doc.info.pages.iter().map(|p| p.label.clone()).collect();
    let Some(view) = app.views.get_mut(index) else { return };
    let text = format!("{} / {}", view.current + 1, labels.len());
    let font = theme::medium(13.0);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), font.clone(), t.text).size().x) + 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 4.0, TAP), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Page {text}")));
    let chip = Rect::from_center_size(rect.center(), vec2(w, 30.0));
    ui.painter().rect(
        chip,
        CornerRadius::same(15),
        if resp.is_pointer_button_down_on() { t.pressed } else { t.field },
        Stroke::new(1.0, t.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(chip.center(), Align2::CENTER_CENTER, text, font, t.text);
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(200.0);
        ui.horizontal(|ui| {
            if icons::button(ui, "chevron-up", TAP, false, "Previous page").clicked() {
                view.go_to_page(view.current.saturating_sub(1));
            }
            if icons::button(ui, "chevron-down", TAP, false, "Next page").clicked() {
                view.go_to_page(view.current + 1);
            }
            let edit = egui::TextEdit::singleline(&mut view.page_input).id(egui::Id::new("phone-page-input")).desired_width(64.0).hint_text("Page");
            let r = ui.add(edit);
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let typed = view.page_input.clone();
                if !view.go_to_typed(&typed, &labels) {
                    view.page_input = (view.current + 1).to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("First page").min_size(vec2(0.0, TAP))).clicked() {
                view.go_to_page(0);
            }
            if ui.add(egui::Button::new("Last page").min_size(vec2(0.0, TAP))).clicked() {
                view.go_to_page(labels.len().saturating_sub(1));
            }
        });
        ui.horizontal(|ui| {
            use crate::canvas::Fit;
            if ui.add(egui::Button::new("Fit width").min_size(vec2(0.0, TAP)).selected(view.fit == Fit::Width)).clicked() {
                view.fit = Fit::Width;
                view.goto = Some((view.current, 0.0));
            }
            if ui.add(egui::Button::new("Whole page").min_size(vec2(0.0, TAP)).selected(view.fit == Fit::Page)).clicked() {
                view.fit = Fit::Page;
                view.goto = Some((view.current, 0.0));
            }
        });
    });
}

/// ⋮: the main menu, the side panels and the theme.
fn overflow_menu(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    if app.active.is_some() {
        ui.menu_button("Side panels", |ui| {
            for (p, label) in PANELS {
                if ui.add(egui::Button::new(label).min_size(vec2(180.0, TAP)).selected(app.right == Some(p))).clicked() {
                    app.open_panel_sheet(p);
                    ui.close();
                }
            }
        });
    }
    crate::chrome::main_menu_items(app, ui);
    ui.separator();
    let (label, next) = match app.theme {
        ThemeKind::Light => ("Dark theme", ThemeKind::Dark),
        ThemeKind::Dark => ("Light theme", ThemeKind::Light),
    };
    if ui.add(egui::Button::new(label).min_size(vec2(0.0, TAP))).clicked() {
        let ctx = ui.ctx().clone();
        app.follow_system_theme = false;
        app.set_theme(&ctx, next);
        ui.close();
    }
}

const PANELS: [(RightPanel, &str); 7] = [
    (RightPanel::Comments, "Comments"),
    (RightPanel::Bookmarks, "Bookmarks"),
    (RightPanel::Pages, "Pages"),
    (RightPanel::Fields, "Form fields"),
    (RightPanel::Layers, "Layers"),
    (RightPanel::Attachments, "Attachments"),
    (RightPanel::Signatures, "Signatures"),
];

/// The bottom navigation bar (a document is open).
fn nav_bar(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("phone_nav").exact_size(NAV_H).frame(egui::Frame::NONE.fill(t.chrome).stroke(Stroke::new(1.0, t.divider))).show(ui, |ui| {
        let tools_on = app.phone_sheet == Some(Sheet::Tools);
        let tool_is = |id: &'static str| tools_on && app.left == LeftPanel::Tool(id);
        let items: [(&str, &str, bool); 5] = [
            ("layout-grid", "Tools", tools_on && !matches!(app.left, LeftPanel::Tool("comment" | "fill_sign"))),
            ("message-square-plus", "Comment", tool_is("comment")),
            ("signature", "Fill & Sign", tool_is("fill_sign")),
            ("files", "Pages", app.phone_sheet == Some(Sheet::Panel) && app.right == Some(RightPanel::Pages)),
            ("panel-right", "Panels", app.phone_sheet == Some(Sheet::Panel) && app.right != Some(RightPanel::Pages)),
        ];
        let w = ui.available_width() / items.len() as f32;
        let top = ui.max_rect().left_top();
        for (n, (icon, label, on)) in items.into_iter().enumerate() {
            let rect = Rect::from_min_size(top + vec2(w * n as f32, 0.0), vec2(w, NAV_H));
            let resp = ui.interact(rect, ui.id().with(("nav", n)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, label));
            let pill = Rect::from_center_size(rect.center_top() + vec2(0.0, 20.0), vec2(52.0, 28.0));
            if on {
                ui.painter().rect_filled(pill, CornerRadius::same(14), t.accent_soft);
            } else if resp.is_pointer_button_down_on() {
                ui.painter().rect_filled(pill, CornerRadius::same(14), t.pressed);
            }
            let tint = if on { t.accent_text } else { t.icon };
            icons::paint(ui, pill, icon, 20.0, tint);
            ui.painter().text(
                rect.center_top() + vec2(0.0, 46.0),
                Align2::CENTER_CENTER,
                label,
                theme::medium(11.5),
                if on { t.text } else { t.text_muted },
            );
            if n == 4 {
                egui::Popup::menu(&resp).align(egui::RectAlign::TOP_END).show(|ui| {
                    ui.set_min_width(200.0);
                    for (p, label) in PANELS {
                        if ui.add(egui::Button::new(label).min_size(vec2(180.0, TAP)).selected(app.right == Some(p))).clicked() {
                            app.open_panel_sheet(p);
                            ui.close();
                        }
                    }
                });
                continue;
            }
            if resp.clicked() {
                if on {
                    app.phone_sheet = None;
                } else {
                    match n {
                        0 => {
                            if matches!(app.left, LeftPanel::Tool("comment" | "fill_sign")) {
                                app.left = LeftPanel::AllTools;
                            }
                            app.open_tool_sheet(None);
                        }
                        1 => app.open_tool_sheet(Some("comment")),
                        2 => app.open_tool_sheet(Some("fill_sign")),
                        _ => app.open_panel_sheet(RightPanel::Pages),
                    }
                }
            }
        }
    });
}

/// The sheet: the tool panel or a side panel over the lower part of the screen.
fn sheet(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    let Some(which) = app.phone_sheet else { return };
    if which == Sheet::Panel && (app.active.is_none() || app.right.is_none()) {
        app.phone_sheet = None;
        return;
    }
    let tool_before = app.quick_tool;
    let goto_before = app.active.and_then(|i| app.views.get(i)).map(|v| (v.current, v.goto.is_some()));
    match which {
        Sheet::Tools => {
            app.left_open = true;
            crate::panels::left_panel(app, ui);
            if !app.left_open {
                app.phone_sheet = None;
            }
        }
        Sheet::Panel => {
            crate::panels::right_panel(app, ui);
            if app.right.is_none() {
                app.phone_sheet = None;
            }
        }
    }
    // Picking a tool or a place in the document closes the sheet: the page is what matters now.
    let goto_after = app.active.and_then(|i| app.views.get(i)).map(|v| (v.current, v.goto.is_some()));
    if app.quick_tool != tool_before || (goto_after.is_some_and(|(_, g)| g) && goto_after != goto_before) {
        app.phone_sheet = None;
    }
}

/// The sheet's height: a little over half the window.
pub(crate) fn sheet_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() * 0.55).max(220.0)
}

/// A dialog's width on this screen: `wanted`, or the screen's width less a margin.
pub(crate) fn fit_width(ui: &mut egui::Ui, wanted: f32) {
    let max = (ui.ctx().content_rect().width() - 56.0).max(200.0);
    ui.set_width(wanted.min(max));
}

/// A dialog's body: on a phone it scrolls within the screen, both ways.
pub(crate) fn scroll(ui: &mut egui::Ui, compact: bool, body: &mut dyn FnMut(&mut egui::Ui)) {
    if !compact {
        body(ui);
        return;
    }
    let screen = ui.ctx().content_rect();
    egui::ScrollArea::both()
        .id_salt("phone-dialog")
        .max_width(screen.width() - 40.0)
        .max_height(screen.height() - 96.0)
        .auto_shrink([true, true])
        .show(ui, |ui| body(ui));
}
