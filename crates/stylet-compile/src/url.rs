//! `url()` rebasing and `?inline` data URIs.

use std::path::{Path, PathBuf};
use stylet_resolve::{normalize, relative};

/// What to do with a URL found in a file.
#[derive(Debug, PartialEq, Eq)]
pub enum Rewrite {
    /// Absolute, external or otherwise not a relative file reference.
    Keep,
    /// Rebased relative URL (query and fragment kept).
    Rebased(String),
    /// Inline the file at this path as a data URI.
    Inline(PathBuf),
}

/// Rewrites `url`, written in a file in `file_dir`, for a stylesheet in `out_dir`.
pub fn rewrite(url: &str, file_dir: &Path, out_dir: &Path) -> Rewrite {
    if url.is_empty() || url.starts_with(['#', '/']) || has_scheme(url) {
        return Rewrite::Keep;
    }
    let split = url.find(['?', '#']).unwrap_or(url.len());
    let (path, suffix) = url.split_at(split);
    let target = normalize(&file_dir.join(path));
    let query = suffix.split('#').next().unwrap_or_default();
    let params: Vec<_> = query
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .collect();
    if params.contains(&"inline") {
        return Rewrite::Inline(target);
    }
    let rebased = relative(out_dir, &target);
    let mut rebased = rebased
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    rebased += suffix;
    Rewrite::Rebased(rebased)
}

fn has_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub fn data_uri(path: &Path, bytes: &[u8]) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let mime = match ext.as_deref() {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    };
    format!("data:{mime};base64,{}", base64(bytes))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites() {
        let file = Path::new("/p/client/components");
        let out = Path::new("/p/public/dist");
        let r = |url| rewrite(url, file, out);
        assert_eq!(
            r("img/a.png"),
            Rewrite::Rebased("../../client/components/img/a.png".into())
        );
        assert_eq!(
            r("../x.svg?v=1#i"),
            Rewrite::Rebased("../../client/x.svg?v=1#i".into())
        );
        assert_eq!(
            r("icon.svg?inline"),
            Rewrite::Inline("/p/client/components/icon.svg".into())
        );
        for keep in [
            "/abs.png",
            "#frag",
            "data:image/png;base64,xx",
            "https://x.y/a.png",
            "//cdn/a.png",
            "",
        ] {
            assert_eq!(r(keep), Rewrite::Keep, "{keep}");
        }
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
