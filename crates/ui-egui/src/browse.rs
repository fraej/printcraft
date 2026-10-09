//! PrintCraft's own file browser: answers `pick` requests where the platform has no file
//! dialog to wait on (Android). Lists folders and the files a request accepts, from a few
//! starting places, and returns the chosen file(s), folder or save path.

use std::path::{Path, PathBuf};

use egui::{Align, Align2, CornerRadius, Layout, Rect, Sense, vec2};

use crate::pick::{PickKind, PickRequest};
use crate::theme::{self, Tokens};
use crate::{PrintCraftApp, icons, widgets};

/// The most entries one folder lists (a folder with more says so).
const MAX_ENTRIES: usize = 5000;

/// What the host platform adds beyond `std` (the Android app implements it).
pub trait Platform: Send + Sync {
    /// Whether the app may see the user's shared files (Android: All files access).
    fn has_file_access(&self) -> bool {
        true
    }
    /// Ask for that access (Android opens the system setting).
    fn request_file_access(&self) {}
    /// How far the system bars reach into the window, in physical pixels: [left, top, right,
    /// bottom]. The shell keeps its content clear of them.
    fn insets(&self) -> [f32; 4] {
        [0.0; 4]
    }
}

/// A starting place in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub label: String,
    pub icon: &'static str,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    pub size: u64,
}

/// The browser's state while it is open.
#[derive(Clone, Debug)]
pub struct Browser {
    pub request: PickRequest,
    pub dir: PathBuf,
    /// Save: the file name being typed.
    pub name: String,
    /// Files: the files ticked so far.
    pub chosen: Vec<PathBuf>,
    /// Save: the path that already exists and was asked about once.
    pub confirm_replace: Option<PathBuf>,
    /// The user's answer, applied at the start of the next frame: the tap that chose must not
    /// also land on whatever the choice puts under the finger.
    done: Option<Outcome>,
    listing: Option<(PathBuf, Result<Listing, String>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// More entries than were listed.
    pub truncated: bool,
}

/// What the user did in the browser this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Chosen(Vec<PathBuf>),
    Cancelled,
}

impl Browser {
    pub fn new(request: PickRequest, dir: PathBuf) -> Self {
        let name = request.file_name.clone();
        Browser { request, dir, name, chosen: Vec::new(), confirm_replace: None, done: None, listing: None }
    }

    /// The current folder's entries (read once per folder; `refresh` rereads).
    pub fn listing(&mut self) -> &Result<Listing, String> {
        if self.listing.as_ref().is_some_and(|(d, _)| *d != self.dir) {
            self.listing = None;
        }
        let (dir, exts, folders_only) = (&self.dir, self.request.extensions(), self.request.kind == PickKind::Folder);
        &self.listing.get_or_insert_with(|| (dir.clone(), list(dir, &exts, folders_only))).1
    }

    pub fn refresh(&mut self) {
        self.listing = None;
    }

    pub fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.confirm_replace = None;
    }

    pub fn up(&mut self) {
        if let Some(p) = self.dir.parent() {
            let p = p.to_path_buf();
            self.go(p);
        }
    }

    /// Tap a file: Open answers at once, Files ticks it, Save takes its name.
    pub fn tap_file(&mut self, path: &Path) -> Option<Outcome> {
        match self.request.kind {
            PickKind::File => Some(Outcome::Chosen(vec![path.to_path_buf()])),
            PickKind::Files => {
                if let Some(i) = self.chosen.iter().position(|p| p == path) {
                    self.chosen.remove(i);
                } else {
                    self.chosen.push(path.to_path_buf());
                }
                None
            }
            PickKind::Save => {
                self.name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                None
            }
            PickKind::Folder => None,
        }
    }

    /// Save: the path to write, with the filter's extension added when the name has none.
    pub fn save_path(&self) -> Option<PathBuf> {
        let name = self.name.trim();
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return None;
        }
        let has_ext = Path::new(name).extension().is_some();
        let name = match self.request.extensions().first() {
            Some(ext) if !has_ext => format!("{name}.{ext}"),
            _ => name.to_string(),
        };
        Some(self.dir.join(name))
    }

    /// The confirm button: the answer, if the request is complete.
    pub fn confirm(&mut self) -> Option<Outcome> {
        match self.request.kind {
            PickKind::File => None,
            PickKind::Files => (!self.chosen.is_empty()).then(|| Outcome::Chosen(self.chosen.clone())),
            PickKind::Folder => Some(Outcome::Chosen(vec![self.dir.clone()])),
            PickKind::Save => {
                let path = self.save_path()?;
                // Replacing a file asks once (the second tap confirms).
                if path.exists() && self.confirm_replace.as_ref() != Some(&path) {
                    self.confirm_replace = Some(path);
                    return None;
                }
                Some(Outcome::Chosen(vec![path]))
            }
        }
    }
}

/// List `dir`: folders first, then the files with one of `exts` (any file when empty), by name.
pub fn list(dir: &Path, exts: &[String], folders_only: bool) -> Result<Listing, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => "PrintCraft isn't allowed to see this folder.".to_string(),
        std::io::ErrorKind::NotFound => "This folder doesn't exist.".to_string(),
        _ => format!("Couldn't read this folder: {e}"),
    })?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for item in rd.flatten() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Follow links: a link to a folder is a folder.
        let Ok(meta) = std::fs::metadata(item.path()) else { continue };
        let dir = meta.is_dir();
        if !dir {
            if folders_only {
                continue;
            }
            let ext = Path::new(&name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if !exts.is_empty() && !exts.contains(&ext) {
                continue;
            }
        }
        entries.push(Entry { name, path: item.path(), dir, size: if dir { 0 } else { meta.len() } });
    }
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing { entries, truncated })
}

impl PrintCraftApp {
    /// Apply the browser's answer from the last frame, then open the browser for a request
    /// posted by a file dialog, if one is waiting.
    pub(crate) fn poll_pick_requests(&mut self) {
        if let Some(o) = self.browser.as_mut().and_then(|b| b.done.take()) {
            self.finish_browser(o);
        }
        if self.browser.is_some() {
            return;
        }
        if let Some(req) = crate::pick::take_request() {
            self.open_browser(req);
        }
    }

    /// Show the file browser for `req` (also the entry point for tests).
    pub fn open_browser(&mut self, req: PickRequest) {
        let start = self
            .browse_dir
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| self.places.first().map(|p| p.path.clone()))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.browser = Some(Browser::new(req, start));
    }

    /// Finish the browser: hand the answer to the dialog that asked and run its command again.
    pub fn finish_browser(&mut self, outcome: Outcome) {
        let Some(b) = self.browser.take() else { return };
        let Outcome::Chosen(paths) = outcome else { return };
        self.browse_dir = Some(b.dir.clone());
        let replay = b.request.replay.clone();
        crate::pick::answer(b.request, paths);
        match replay {
            Some(cmd) => {
                self.execute(&cmd);
                crate::pick::clear_answer();
            }
            // Asked from a dialog's button: the answer waits for the next tap.
            None => self.notify("Chosen. Tap the button again to continue."),
        }
    }
}

/// Draw the browser (when open).
pub fn show(app: &mut PrintCraftApp, ctx: &egui::Context) {
    let Some(mut b) = app.browser.take() else { return };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let access = app.platform.as_ref().is_none_or(|p| p.has_file_access());
    let mut outcome = None;
    let mut ask_access = false;
    let modal = egui::Modal::new(egui::Id::new("file_browser")).show(ctx, |ui| {
        let w = (screen.width() - 24.0).clamp(240.0, 560.0);
        let h = (screen.height() - 64.0).clamp(240.0, 760.0);
        ui.set_width(w);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(b.request.heading()).font(theme::semibold(17.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, "x", 36.0, false, "Cancel").clicked() {
                    outcome = Some(Outcome::Cancelled);
                }
            });
        });
        // Starting places.
        egui::ScrollArea::horizontal().id_salt("places").show(ui, |ui| {
            ui.horizontal(|ui| {
                // The deepest place holding this folder (Downloads, not also Phone).
                let here =
                    app.places.iter().filter(|p| b.dir.starts_with(&p.path)).max_by_key(|p| p.path.components().count()).map(|p| p.path.clone());
                for p in app.places.clone() {
                    let on = here.as_ref() == Some(&p.path);
                    if widgets::icon_pill(ui, p.icon, &p.label, on).clicked() {
                        b.go(p.path.clone());
                    }
                }
            });
        });
        if !access {
            egui::Frame::NONE.fill(t.accent_soft).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label("PrintCraft can only see its own folder until you allow access to your files.");
                if widgets::pill_button(ui, "Allow access to files", true).clicked() {
                    ask_access = true;
                }
            });
        }
        // The current folder.
        ui.horizontal(|ui| {
            let has_parent = b.dir.parent().is_some();
            if ui.add_enabled_ui(has_parent, |ui| icons::button(ui, "chevron-left", 36.0, false, "Up one folder")).inner.clicked() {
                b.up();
            }
            let name = b.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| b.dir.to_string_lossy().into_owned());
            ui.label(egui::RichText::new(name).font(theme::medium(14.0))).on_hover_text(b.dir.to_string_lossy());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, "rotate-cw", 36.0, false, "Refresh").clicked() {
                    b.refresh();
                }
            });
        });
        ui.separator();
        let footer_h = match b.request.kind {
            PickKind::File => 0.0,
            PickKind::Save => 96.0,
            _ => 52.0,
        };
        let list_h = (h - 170.0 - footer_h).max(120.0);
        let kind = b.request.kind;
        let chosen = b.chosen.clone();
        let mut tapped: Option<(PathBuf, bool)> = None;
        egui::ScrollArea::vertical()
            .id_salt(("browse", b.dir.clone()))
            .max_height(list_h)
            .min_scrolled_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| match b.listing() {
                Err(e) => {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(e.as_str()).color(t.text_muted));
                }
                Ok(l) if l.entries.is_empty() => {
                    ui.add_space(12.0);
                    let what = if kind == PickKind::Folder { "No folders here." } else { "Nothing here that PrintCraft can use." };
                    ui.label(egui::RichText::new(what).color(t.text_muted));
                }
                Ok(l) => {
                    for e in &l.entries {
                        let ticked = chosen.contains(&e.path);
                        if row(ui, &t, e, ticked).clicked() {
                            tapped = Some((e.path.clone(), e.dir));
                        }
                    }
                    if l.truncated {
                        ui.label(egui::RichText::new(format!("Showing the first {MAX_ENTRIES} items.")).color(t.text_faint));
                    }
                }
            });
        match tapped {
            Some((p, true)) => b.go(p),
            Some((p, false)) => {
                if let Some(o) = b.tap_file(&p) {
                    outcome = Some(o);
                }
            }
            None => {}
        }
        if kind != PickKind::File {
            ui.separator();
        }
        if kind == PickKind::Save {
            let edit = egui::TextEdit::singleline(&mut b.name).hint_text("File name").desired_width(f32::INFINITY).margin(vec2(8.0, 8.0));
            if ui.add(edit).changed() {
                b.confirm_replace = None;
            }
            if let Some(p) = &b.confirm_replace {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.label(egui::RichText::new(format!("{name} already exists. Tap Save again to replace it.")).color(t.text_muted));
            }
        }
        if kind != PickKind::File {
            ui.horizontal(|ui| {
                let label = match kind {
                    PickKind::Save => "Save".to_string(),
                    PickKind::Folder => "Use this folder".to_string(),
                    _ => format!("Choose {} file{}", b.chosen.len(), if b.chosen.len() == 1 { "" } else { "s" }),
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::pill_button(ui, &label, true).clicked()
                        && let Some(o) = b.confirm()
                    {
                        outcome = Some(o);
                    }
                    if widgets::pill_button(ui, "Cancel", false).clicked() {
                        outcome = Some(Outcome::Cancelled);
                    }
                });
            });
        }
    });
    if modal.should_close() && outcome.is_none() {
        outcome = Some(Outcome::Cancelled);
    }
    if ask_access && let Some(p) = &app.platform {
        p.request_file_access();
        b.refresh();
    }
    // Answered: it stays up (still blocking taps) until the next frame applies the answer.
    if outcome.is_some() && b.done.is_none() {
        b.done = outcome;
        ctx.request_repaint();
    }
    app.browser = Some(b);
}

fn row(ui: &mut egui::Ui, t: &Tokens, e: &Entry, ticked: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, ticked, &e.name));
    if ticked {
        ui.painter().rect_filled(rect, CornerRadius::same(8), t.accent_soft);
    } else if resp.hovered() || resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), t.hover);
    }
    let icon = if e.dir { "folder" } else { "file-text" };
    let tint = if e.dir { t.icon } else { egui::Color32::from_rgb(0xE0, 0x3E, 0x3E) };
    icons::paint(ui, Rect::from_min_size(rect.min + vec2(8.0, 12.0), vec2(24.0, 24.0)), icon, 22.0, tint);
    let right = if e.dir { String::new() } else { crate::panels::human_size(usize::try_from(e.size).unwrap_or(usize::MAX)) };
    let text_w = rect.width() - 44.0 - if right.is_empty() { 8.0 } else { 80.0 };
    let galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(e.name.clone(), theme::regular(14.0), t.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_w.max(40.0));
        f.layout_job(job)
    });
    ui.painter().galley(rect.left_center() + vec2(42.0, -galley.size().y / 2.0), galley, t.text);
    if ticked {
        icons::paint(ui, Rect::from_center_size(rect.right_center() - vec2(20.0, 0.0), vec2(18.0, 18.0)), "check", 18.0, t.accent);
    } else if !right.is_empty() {
        ui.painter().text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, right, theme::regular(12.0), t.text_faint);
    } else {
        icons::paint(ui, Rect::from_center_size(rect.right_center() - vec2(18.0, 0.0), vec2(16.0, 16.0)), "chevron-right", 16.0, t.text_faint);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("printcraft-browse-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("b.pdf"), b"%PDF").unwrap();
        std::fs::write(d.join("A.PDF"), b"%PDF").unwrap();
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        std::fs::write(d.join(".hidden.pdf"), b"x").unwrap();
        d
    }

    #[test]
    fn lists_folders_first_and_filters_by_extension() {
        let d = temp_dir("list");
        let l = list(&d, &["pdf".into()], false).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["sub", "A.PDF", "b.pdf"]);
        let all = list(&d, &[], false).unwrap();
        assert_eq!(all.entries.len(), 4);
        let folders = list(&d, &[], true).unwrap();
        assert_eq!(folders.entries.len(), 1);
        assert!(list(&d.join("missing"), &[], false).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_adds_the_extension_and_asks_before_replacing() {
        let d = temp_dir("save");
        let mut req = PickRequest::new(PickKind::Save);
        req.filters.push(("PDF".into(), vec!["pdf".into()]));
        let mut b = Browser::new(req, d.clone());
        b.name = "report".into();
        assert_eq!(b.confirm(), Some(Outcome::Chosen(vec![d.join("report.pdf")])));
        b.name = "b.pdf".into();
        assert_eq!(b.confirm(), None, "the first tap asks");
        assert_eq!(b.confirm(), Some(Outcome::Chosen(vec![d.join("b.pdf")])));
        b.name = "../x".into();
        assert_eq!(b.confirm(), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn files_tick_and_untick() {
        let mut b = Browser::new(PickRequest::new(PickKind::Files), PathBuf::from("/"));
        assert_eq!(b.confirm(), None);
        b.tap_file(Path::new("/a.pdf"));
        b.tap_file(Path::new("/b.pdf"));
        b.tap_file(Path::new("/a.pdf"));
        assert_eq!(b.confirm(), Some(Outcome::Chosen(vec![PathBuf::from("/b.pdf")])));
        let mut one = Browser::new(PickRequest::new(PickKind::File), PathBuf::from("/"));
        assert_eq!(one.tap_file(Path::new("/c.pdf")), Some(Outcome::Chosen(vec![PathBuf::from("/c.pdf")])));
    }
}
