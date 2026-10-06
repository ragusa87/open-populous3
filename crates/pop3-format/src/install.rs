//! Locating the user's original install (the folder holding `levels/`, `data/`, `objects/`).
//! `POP3_INSTALL` wins; otherwise the usual locations are searched: Wine prefixes on Unix,
//! `C:\Program Files*` on Windows. Callers skip this entirely with `--no-original`.

use std::path::{Path, PathBuf};

pub const INSTALL_ENV: &str = "POP3_INSTALL";

/// Folders that contain game installs (`<base>/<game dir>/levels`) or are one (`<base>/levels`).
pub fn default_bases(home: Option<&Path>, wine_prefix: Option<&Path>, windows: bool) -> Vec<PathBuf> {
    let program_files = ["Program Files (x86)", "Program Files"];
    let mut drives: Vec<PathBuf> = Vec::new();
    if windows {
        drives.push(PathBuf::from(r"C:\"));
    } else {
        drives.extend(wine_prefix.map(|p| p.join("drive_c")));
        drives.extend(home.map(|h| h.join(".wine").join("drive_c")));
    }
    let mut bases = Vec::new();
    for drive in &drives {
        for pf in program_files {
            bases.push(drive.join(pf).join("Bullfrog"));
        }
        bases.push(drive.join("GOG Games"));
    }
    bases
}

/// `dir` is an install when it has a `levels` sub-directory (any case).
pub fn is_install(dir: &Path) -> bool {
    crate::find_file(dir, "levels").is_some_and(|p| p.is_dir())
}

/// First install found in `bases`: the base itself, else its sub-directories in name order.
pub fn find_install_in(bases: &[PathBuf]) -> Option<PathBuf> {
    bases.iter().find_map(|base| {
        if is_install(base) {
            return Some(base.clone());
        }
        let mut subdirs: Vec<PathBuf> = std::fs::read_dir(base).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subdirs.sort();
        subdirs.into_iter().find(|d| is_install(d))
    })
}

/// `$POP3_INSTALL`, else the first install in the usual locations for this platform.
pub fn find_install() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(INSTALL_ENV) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let prefix = std::env::var_os("WINEPREFIX").map(PathBuf::from);
    find_install_in(&default_bases(home.as_deref(), prefix.as_deref(), cfg!(windows)))
}

/// A sub-folder of the install (`levels`, `data`, `objects`), matching case-insensitively.
pub fn subdir(install: &Path, name: &str) -> PathBuf {
    crate::find_file(install, name).unwrap_or_else(|| install.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("pop3-install-test-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            TempTree(root)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            if self.0.starts_with(std::env::temp_dir()) && self.0.ends_with(format!("{}", std::process::id())) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn unix_bases_cover_wine_prefixes() {
        let bases = default_bases(Some(Path::new("/home/u")), Some(Path::new("/p")), false);
        assert_eq!(bases[0], PathBuf::from("/p/drive_c/Program Files (x86)/Bullfrog"));
        assert!(bases.contains(&PathBuf::from("/home/u/.wine/drive_c/Program Files/Bullfrog")));
    }

    #[test]
    fn windows_bases_are_on_c() {
        let bases = default_bases(Some(Path::new("/ignored")), None, true);
        assert!(bases.iter().all(|b| b.starts_with(r"C:\")));
        assert!(bases.iter().any(|b| b.ends_with("Bullfrog")));
    }

    #[test]
    fn finds_any_language_install_under_bullfrog() {
        let t = TempTree::new("find");
        let bullfrog = t.0.join("Bullfrog");
        std::fs::create_dir_all(bullfrog.join("Other Game")).unwrap();
        std::fs::create_dir_all(bullfrog.join("Populous - The Beginning").join("LEVELS")).unwrap();
        let missing = t.0.join("missing");
        assert_eq!(find_install_in(&[missing.clone(), bullfrog]), Some(t.0.join("Bullfrog/Populous - The Beginning")));
        assert_eq!(find_install_in(&[missing]), None);
    }

    #[test]
    fn subdir_matches_case() {
        let t = TempTree::new("subdir");
        std::fs::create_dir_all(t.0.join("DATA")).unwrap();
        assert_eq!(subdir(&t.0, "data"), t.0.join("DATA"));
        assert_eq!(subdir(&t.0, "objects"), t.0.join("objects"));
    }
}
