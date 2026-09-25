use std::path::{Component, Path, PathBuf};

/// Lexically removes `.` and `..` components (no file system access).
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Path from directory `from` to `to`, e.g. `relative("/a/b", "/a/c/d") == "../c/d"`.
/// Both paths should be normalized and either both absolute or both relative.
pub fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for component in &to[common..] {
        out.push(component);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), Path::new("/a/c"));
        assert_eq!(normalize(Path::new("/../a")), Path::new("/a"));
        assert_eq!(normalize(Path::new("../a/../../b")), Path::new("../../b"));
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative(Path::new("/a/b"), Path::new("/a/c/d")),
            Path::new("../c/d")
        );
        assert_eq!(
            relative(Path::new("/a"), Path::new("/a/x.png")),
            Path::new("x.png")
        );
        assert_eq!(
            relative(Path::new("/a/b"), Path::new("/a/b")),
            Path::new("")
        );
    }
}
