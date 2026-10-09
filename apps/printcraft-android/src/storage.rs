//! Where files live on Android (plain paths; tested on every platform).

use std::path::{Component, Path, PathBuf};

use printcraft_ui_egui::browse::Place;

/// Where Android keeps the user's shared files.
pub const SHARED: &str = "/storage/emulated/0";

/// The browser's starting places: shared storage when PrintCraft may see it, its own folder
/// first otherwise.
pub fn places(own: Option<&Path>, access: bool) -> Vec<Place> {
    let shared = Path::new(SHARED);
    let mut v = vec![
        Place { label: "Downloads".into(), icon: "file-down", path: shared.join("Download") },
        Place { label: "Documents".into(), icon: "files", path: shared.join("Documents") },
        Place { label: "Phone".into(), icon: "folder", path: shared.to_path_buf() },
    ];
    if let Some(own) = own {
        let p = Place { label: "PrintCraft".into(), icon: "folder-open", path: own.to_path_buf() };
        if access { v.push(p) } else { v.insert(0, p) }
    }
    v
}

/// Map a storage provider's document id to its path: `primary:Download/a.pdf` (Files) or
/// `raw:/storage/emulated/0/Download/a.pdf` (Downloads).
pub fn document_path(authority: &str, id: &str) -> Option<PathBuf> {
    let path = match authority {
        "com.android.externalstorage.documents" => Path::new(SHARED).join(id.strip_prefix("primary:")?),
        "com.android.providers.downloads.documents" => PathBuf::from(id.strip_prefix("raw:")?),
        _ => return None,
    };
    // Only plain paths inside shared storage.
    let plain = path.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)));
    (plain && path.starts_with(SHARED)).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_put_the_own_folder_first_without_access() {
        let own = Path::new("/data/own");
        assert_eq!(places(Some(own), false)[0].path, own);
        assert_eq!(places(Some(own), true)[0].path, Path::new(SHARED).join("Download"));
        assert_eq!(places(None, false).len(), 3);
    }

    #[test]
    fn provider_ids_map_to_shared_paths_only() {
        let ext = "com.android.externalstorage.documents";
        assert_eq!(document_path(ext, "primary:Download/a.pdf"), Some(PathBuf::from("/storage/emulated/0/Download/a.pdf")));
        assert_eq!(document_path(ext, "primary:../../data/x"), None);
        assert_eq!(document_path(ext, "1234-ABCD:a.pdf"), None);
        let dl = "com.android.providers.downloads.documents";
        assert_eq!(document_path(dl, "raw:/storage/emulated/0/Download/b.pdf"), Some(PathBuf::from("/storage/emulated/0/Download/b.pdf")));
        assert_eq!(document_path(dl, "raw:/data/data/x"), None);
        assert_eq!(document_path(dl, "msf:42"), None);
        assert_eq!(document_path("com.example", "primary:a.pdf"), None);
    }
}
