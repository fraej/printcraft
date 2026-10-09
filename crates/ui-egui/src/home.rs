//! Home tab: recommended tools, open card, recent files (local only, never another app's list).

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, vec2};
use printcraft_engine::catalog;

use crate::theme::{self, Tokens};
use crate::{LeftPanel, PrintCraftApp, icons, panels::human_size, widgets};

const RECOMMENDED: [&str; 5] = ["organize", "comment", "form", "edit", "protect"];

pub fn show(app: &mut PrintCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let compact = app.compact;
    // A phone: narrow margins, tool cards two to a row.
    let side = if compact { 16 } else { 36 };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::NONE.inner_margin(egui::Margin { left: side, right: side, top: if compact { 18 } else { 28 }, bottom: 28 }).show(ui, |ui| {
            ui.label(egui::RichText::new("Welcome to PrintCraft").font(theme::semibold(24.0)));
            ui.label(
                egui::RichText::new("An open-source PDF workbench — local, private, and scriptable.").color(t.text_muted).font(theme::regular(14.0)),
            );
            ui.add_space(14.0);
            egui::Frame::NONE
                .fill(t.card)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::artcraft_mark(ui, 28.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Join the ArtCraft community").font(theme::semibold(15.0)));
                            ui.label(egui::RichText::new("Get help, share feedback and follow development on Discord.").color(t.text_muted));
                        });
                    });
                    ui.add_space(8.0);
                    if let Some(cmd) = widgets::community_links(ui) {
                        app.execute(cmd);
                    }
                });
            ui.add_space(22.0);

            egui::Frame::NONE
                .fill(t.card)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(if compact { 12 } else { 18 }))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Recommended tools").font(theme::semibold(15.0)));
                    ui.add_space(10.0);
                    let gap = if compact { 10.0 } else { 14.0 };
                    let (tile_w, open_w) = if compact {
                        let w = ((ui.available_width() - gap) / 2.0).floor().max(120.0);
                        (w, w)
                    } else {
                        (190.0, 170.0)
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(gap, gap);
                        for id in RECOMMENDED {
                            let Some(g) = catalog::group(id) else { continue };
                            let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, 104.0), Sense::click());
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, g.label));
                            let fill = if resp.hovered() { t.hover } else { t.card };
                            ui.painter().rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
                            let color = egui::Color32::from_rgb(g.hue[0], g.hue[1], g.hue[2]);
                            icons::paint(ui, Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(22.0, 22.0)), g.icon, 21.0, color);
                            ui.painter().text(rect.min + vec2(44.0, 25.0), Align2::LEFT_CENTER, g.label, theme::semibold(13.5), t.text);
                            let blurb = g
                                .sections
                                .first()
                                .map(|s| s.items.iter().take(if compact { 2 } else { 3 }).map(|i| i.label).collect::<Vec<_>>().join(" · "))
                                .unwrap_or_default();
                            // At most two lines: the card's link sits below.
                            let galley = ui.fonts_mut(|f| {
                                let mut job = egui::text::LayoutJob::simple(blurb, theme::regular(11.5), t.text_muted, rect.width() - 28.0);
                                job.wrap.max_rows = 2;
                                f.layout_job(job)
                            });
                            ui.painter().galley(rect.min + vec2(14.0, 46.0), galley, t.text_muted);
                            ui.painter().text(
                                rect.left_bottom() + vec2(14.0, -14.0),
                                Align2::LEFT_CENTER,
                                "Use now",
                                theme::medium(12.0),
                                t.accent_text,
                            );
                            if resp.clicked() {
                                app.left = LeftPanel::Tool(g.id);
                                app.left_open = true;
                            }
                        }
                        let (rect, resp) = ui.allocate_exact_size(vec2(open_w, 104.0), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Open file"));
                        ui.painter().rect(
                            rect,
                            CornerRadius::same(10),
                            if resp.hovered() { t.hover } else { t.pasteboard },
                            Stroke::new(1.0, t.divider),
                            egui::StrokeKind::Inside,
                        );
                        icons::paint(ui, Rect::from_center_size(rect.center() - vec2(0.0, 16.0), vec2(28.0, 28.0)), "folder-open", 26.0, t.icon);
                        ui.painter().text(rect.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "Open file", theme::semibold(13.0), t.text);
                        if resp.clicked() {
                            app.run_command("file.open");
                        }
                    });
                });

            ui.add_space(26.0);
            ui.label(egui::RichText::new("Recent").font(theme::semibold(17.0)));
            ui.add_space(8.0);
            if app.recent.is_empty() {
                ui.label(
                    egui::RichText::new(if compact {
                        "Files you open in PrintCraft appear here."
                    } else {
                        "Files you open in PrintCraft appear here. Drop a PDF anywhere to open it."
                    })
                    .color(t.text_muted),
                );
            }
            let mut open = None;
            for r in &app.recent {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &r.name));
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), t.hover);
                }
                icons::paint(
                    ui,
                    Rect::from_min_size(rect.min + vec2(10.0, 11.0), vec2(24.0, 24.0)),
                    "file-text",
                    22.0,
                    egui::Color32::from_rgb(0xE0, 0x3E, 0x3E),
                );
                ui.painter().text(rect.min + vec2(46.0, 15.0), Align2::LEFT_CENTER, &r.name, theme::medium(13.5), t.text);
                let facts = format!("{} pages  ·  {}", r.pages, human_size(r.size));
                if compact {
                    ui.painter().text(rect.min + vec2(46.0, 32.0), Align2::LEFT_CENTER, facts, theme::regular(11.5), t.text_faint);
                } else {
                    ui.painter().text(rect.min + vec2(46.0, 32.0), Align2::LEFT_CENTER, &r.path, theme::regular(11.0), t.text_faint);
                    ui.painter().text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, facts, theme::regular(12.0), t.text_muted);
                }
                if resp.clicked() {
                    open = Some(r.path.clone());
                }
            }
            if let Some(p) = open {
                if let Some(i) = app.views.iter().position(|v| app.session.get(v.id).and_then(|d| d.path.as_deref()) == Some(p.as_str())) {
                    app.active = Some(i);
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    app.open_path(&p);
                }
            }
            ui.add_space(20.0);
            widgets::section_title(ui, "Privacy");
            ui.label(
                egui::RichText::new("PrintCraft works offline. No telemetry, no account, and no cloud processing unless you add a provider.")
                    .color(t.text_muted),
            );
        });
    });
}
