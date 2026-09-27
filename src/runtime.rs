//! Resolve explicit overrides first, then executable-local dependencies.
use std::path::{Path, PathBuf};

pub fn resolve(variable: &str, relative: &Path, fallback: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(variable) {
        return path.into();
    }
    let executable = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    locate(executable.as_deref(), cwd.as_deref(), relative).unwrap_or_else(|| fallback.into())
}

fn locate(executable: Option<&Path>, cwd: Option<&Path>, relative: &Path) -> Option<PathBuf> {
    // The extra ancestors support target/{debug,release}/{deps,examples} in development.
    executable
        .into_iter()
        .flat_map(|exe| exe.ancestors().skip(1).take(4))
        .chain(cwd)
        .map(|root| root.join(relative))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_location_wins_over_working_directory_and_supports_dev_layout() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("portable é");
        let cwd = dir.path().join("unrelated");
        for root in [&package, &cwd] {
            std::fs::create_dir_all(root.join("tools")).unwrap();
            std::fs::write(root.join("tools/runtime"), b"fixture").unwrap();
        }
        let relative = Path::new("tools/runtime");
        for exe in [
            package.join("app.exe"),
            package.join("target/debug/deps/test.exe"),
        ] {
            assert_eq!(
                locate(Some(&exe), Some(&cwd), relative),
                Some(package.join(relative))
            );
        }
        assert_eq!(locate(None, Some(&cwd), relative), Some(cwd.join(relative)));
        assert_eq!(locate(None, None, relative), None);
    }
}
