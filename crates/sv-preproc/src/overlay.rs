//! Editor buffers: text that stands in for files on disk.
//!
//! A language server holds the unsaved text of the files open in the editor.
//! While a path has an overlay, [`read`] returns that text instead of the
//! file's, so the preprocessor (sources and `` `include `` files alike) and
//! whatever reads source lines afterwards (inline waivers, baselines, fixes)
//! see what the user sees. With no overlay set, [`read`] is the file system.
//!
//! The table belongs to the process, as the open buffers belong to the one
//! editor the process serves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

static TEXTS: RwLock<Option<HashMap<PathBuf, Arc<str>>>> = RwLock::new(None);

/// The path overlays are keyed by: the file's canonical path when it
/// exists, else the path as given.
fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Makes `text` stand in for the file at `path`.
pub fn set(path: &Path, text: &str) {
    if let Ok(mut t) = TEXTS.write() {
        t.get_or_insert_with(HashMap::new).insert(key(path), text.into());
    }
}

/// Drops the overlay of `path`: the file on disk is read again.
pub fn clear(path: &Path) {
    if let Ok(mut t) = TEXTS.write() {
        if let Some(m) = t.as_mut() {
            m.remove(&key(path));
            if m.is_empty() {
                *t = None;
            }
        }
    }
}

/// The overlay of `path`, if it has one.
pub fn get(path: &Path) -> Option<Arc<str>> {
    let texts = TEXTS.read().ok()?;
    let m = texts.as_ref()?;
    m.get(path).or_else(|| m.get(&key(path))).cloned()
}

/// The text of `path`: its overlay, else the file (bytes that are not UTF-8
/// are replaced, as the preprocessor reads them).
pub fn read(path: &Path) -> std::io::Result<String> {
    if let Some(text) = get(path) {
        return Ok(text.to_string());
    }
    std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_overlay_stands_in_until_it_is_cleared() {
        let dir = std::env::temp_dir().join(format!("sv-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.sv");
        std::fs::write(&file, "module on_disk; endmodule\n").unwrap();
        assert_eq!(read(&file).unwrap(), "module on_disk; endmodule\n");
        set(&file, "module in_editor; endmodule\n");
        assert_eq!(read(&file).unwrap(), "module in_editor; endmodule\n");
        // A file that is not on disk yet (a new, unsaved buffer).
        let fresh = dir.join("new.sv");
        assert!(read(&fresh).is_err());
        set(&fresh, "package p; endpackage\n");
        assert_eq!(read(&fresh).unwrap(), "package p; endpackage\n");
        clear(&file);
        clear(&fresh);
        assert_eq!(read(&file).unwrap(), "module on_disk; endmodule\n");
        assert!(read(&fresh).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
