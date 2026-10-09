//! File dialogs.
//!
//! Desktop builds use the system's dialogs (`rfd`), which block until the user chooses. Android
//! has no dialog a native app can wait on, so there `FileDialog` is [`Picker`]: the same builder,
//! answered by PrintCraft's own file browser (`browse`). The first call posts a request and
//! returns `None`; the browser opens on the next frame. Once the user chooses, the command that
//! asked runs again, and this time the call returns the choice.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[cfg(target_os = "android")]
pub use Picker as FileDialog;
#[cfg(not(target_os = "android"))]
pub use rfd::FileDialog;

/// What the user is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    File,
    Files,
    Folder,
    Save,
}

/// A question for the file browser.
#[derive(Clone, Debug, PartialEq)]
pub struct PickRequest {
    pub kind: PickKind,
    pub title: String,
    /// (name, extensions without the dot); empty: any file.
    pub filters: Vec<(String, Vec<String>)>,
    /// Save: the suggested file name.
    pub file_name: String,
    /// The command that asked; it runs again once the user has chosen.
    pub replay: Option<String>,
}

impl PickRequest {
    pub fn new(kind: PickKind) -> Self {
        PickRequest { kind, title: String::new(), filters: Vec::new(), file_name: String::new(), replay: None }
    }

    /// Whether `other` asks the same question (an answer to one answers the other).
    fn same(&self, other: &PickRequest) -> bool {
        self.kind == other.kind && self.title == other.title && self.filters == other.filters
    }

    /// The extensions any filter accepts (lowercase); empty: any file.
    pub fn extensions(&self) -> Vec<String> {
        self.filters.iter().flat_map(|(_, e)| e.iter().map(|x| x.to_ascii_lowercase())).collect()
    }

    pub fn heading(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        match self.kind {
            PickKind::File => "Open".into(),
            PickKind::Files => "Choose files".into(),
            PickKind::Folder => "Choose a folder".into(),
            PickKind::Save => "Save as".into(),
        }
    }
}

/// An answer is kept this long for the command to come back for it.
const ANSWER_TTL: Duration = Duration::from_secs(120);

struct State {
    /// The command running now (requests made during it replay it).
    context: Option<String>,
    request: Option<PickRequest>,
    answer: Option<(PickRequest, Vec<PathBuf>, Instant)>,
}

static STATE: Mutex<State> = Mutex::new(State { context: None, request: None, answer: None });

fn state() -> MutexGuard<'static, State> {
    // A panic elsewhere while holding the lock leaves plain data behind: keep using it.
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Note the command that is running (`None` when it ends); returns the previous one.
pub(crate) fn set_context(command: Option<String>) -> Option<String> {
    std::mem::replace(&mut state().context, command)
}

/// The answer to `req` if the user has given one; otherwise post `req` for the browser.
pub fn ask(mut req: PickRequest) -> Option<Vec<PathBuf>> {
    let mut s = state();
    if let Some((asked, paths, at)) = s.answer.take()
        && asked.same(&req)
        && at.elapsed() < ANSWER_TTL
    {
        return Some(paths);
    }
    req.replay = s.context.clone();
    s.request = Some(req);
    None
}

/// The question waiting for the browser, if any.
pub fn take_request() -> Option<PickRequest> {
    state().request.take()
}

/// The user chose `paths` for `req`.
pub fn answer(req: PickRequest, paths: Vec<PathBuf>) {
    state().answer = Some((req, paths, Instant::now()));
}

/// Forget an answer nobody came back for.
pub fn clear_answer() {
    state().answer = None;
}

/// `rfd::FileDialog`'s builder, answered by the in-app file browser.
#[derive(Clone, Debug)]
pub struct Picker {
    req: PickRequest,
}

impl Default for Picker {
    fn default() -> Self {
        Self::new()
    }
}

impl Picker {
    pub fn new() -> Self {
        Picker { req: PickRequest::new(PickKind::File) }
    }

    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.req.title = title.into();
        self
    }

    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        self.req.filters.push((name.into(), extensions.iter().map(|e| e.to_string()).collect()));
        self
    }

    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        self.req.file_name = name.into();
        self
    }

    fn ask(mut self, kind: PickKind) -> Option<Vec<PathBuf>> {
        self.req.kind = kind;
        ask(self.req)
    }

    pub fn pick_file(self) -> Option<PathBuf> {
        self.ask(PickKind::File)?.into_iter().next()
    }

    pub fn pick_files(self) -> Option<Vec<PathBuf>> {
        self.ask(PickKind::Files).filter(|p| !p.is_empty())
    }

    pub fn pick_folder(self) -> Option<PathBuf> {
        self.ask(PickKind::Folder)?.into_iter().next()
    }

    pub fn save_file(self) -> Option<PathBuf> {
        self.ask(PickKind::Save)?.into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test: the state is process-wide.
    #[test]
    fn picker_asks_then_returns_the_answer() {
        let prev = set_context(Some("file.open".into()));
        assert_eq!(Picker::new().add_filter("PDF", &["pdf"]).pick_file(), None);
        let req = take_request().expect("a request was posted");
        assert_eq!(req.kind, PickKind::File);
        assert_eq!(req.replay.as_deref(), Some("file.open"));
        assert_eq!(req.extensions(), vec!["pdf".to_string()]);
        set_context(prev);
        // An answer to a different question is not used.
        answer(req.clone(), vec![PathBuf::from("/x/a.pdf")]);
        assert_eq!(Picker::new().set_title("Other").pick_file(), None);
        assert!(take_request().is_some());
        // The same question gets the answer, once.
        answer(req, vec![PathBuf::from("/x/a.pdf")]);
        assert_eq!(Picker::new().add_filter("PDF", &["pdf"]).pick_file(), Some(PathBuf::from("/x/a.pdf")));
        assert_eq!(Picker::new().add_filter("PDF", &["pdf"]).pick_file(), None);
        let _ = take_request();
        clear_answer();
    }
}
