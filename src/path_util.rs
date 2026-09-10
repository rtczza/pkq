use std::path::PathBuf;

const USRMERGE_MAP: &[(&str, &str)] = &[
    ("/bin/", "/usr/bin/"),
    ("/sbin/", "/usr/sbin/"),
    ("/lib/", "/usr/lib/"),
    ("/lib32/", "/usr/lib32/"),
    ("/lib64/", "/usr/lib64/"),
    ("/libx32/", "/usr/libx32/"),
];

fn canonicalize_path(path: &str) -> String {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return resolved.to_string_lossy().to_string();
    }
    path.to_string()
}

fn apply_usrmerge(path: &str) -> String {
    for (old, new) in USRMERGE_MAP {
        if path == &old[..old.len() - 1] {
            return new[..new.len() - 1].to_string();
        }
        if let Some(rest) = path.strip_prefix(old) {
            return format!("{}{}", new, rest);
        }
    }
    path.to_string()
}

pub fn normalize_path(path: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let abs = if path.starts_with('/') {
        path.to_string()
    } else {
        match std::fs::canonicalize(path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                format!("{}/{}", cwd.to_string_lossy(), path)
            }
        }
    };
    let canonical = canonicalize_path(&abs);
    candidates.push(canonical.clone());
    let usrmerged = apply_usrmerge(&canonical);
    if usrmerged != canonical {
        candidates.push(usrmerged);
    }
    let reverse_usrmerge = reverse_usrmerge(&canonical);
    if let Some(r) = reverse_usrmerge {
        if r != canonical && !candidates.contains(&r) {
            candidates.push(r);
        }
    }
    candidates
}

fn reverse_usrmerge(path: &str) -> Option<String> {
    for (old, new) in USRMERGE_MAP {
        if let Some(rest) = path.strip_prefix(new) {
            return Some(format!("{}{}", old, rest));
        }
    }
    None
}

pub fn contains_glob(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

pub fn glob_to_regex(glob: &str) -> String {
    let mut result = String::with_capacity(glob.len() * 2);
    for c in glob.chars() {
        match c {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_impl(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_impl(pattern: &[u8], text: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = 0;

    while ti < text.len() {
        if pi < pattern.len() {
            match pattern[pi] {
                b'*' => {
                    star_pi = Some(pi);
                    star_ti = ti;
                    pi += 1;
                    continue;
                }
                b'?' => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                c if c == text[ti] => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ => {}
            }
        }
        if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_basic() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*/bin/unzip", "/usr/bin/unzip"));
        assert!(glob_match("*/bin/unzip", "/bin/unzip"));
        assert!(!glob_match("*/bin/zip", "/usr/bin/unzip"));
        assert!(glob_match("/usr/bin/?nzip", "/usr/bin/unzip"));
        assert!(glob_match("/usr/bin/un*", "/usr/bin/unzip"));
        assert!(!glob_match("/usr/bin/un*", "/usr/bin/zip"));
    }

    #[test]
    fn test_usrmerge() {
        assert_eq!(apply_usrmerge("/bin/unzip"), "/usr/bin/unzip");
        assert_eq!(apply_usrmerge("/sbin/ip"), "/usr/sbin/ip");
        assert_eq!(
            apply_usrmerge("/lib/x86_64-linux-gnu/libc.so.6"),
            "/usr/lib/x86_64-linux-gnu/libc.so.6"
        );
        assert_eq!(apply_usrmerge("/usr/bin/unzip"), "/usr/bin/unzip");
    }

    #[test]
    fn test_contains_glob() {
        assert!(contains_glob("*/bin/unzip"));
        assert!(contains_glob("/usr/bin/?nzip"));
        assert!(!contains_glob("/usr/bin/unzip"));
    }

    #[test]
    fn test_glob_to_regex_escapes() {
        assert_eq!(glob_to_regex("a.b"), "a\\.b");
        assert_eq!(glob_to_regex("*"), ".*");
        assert_eq!(glob_to_regex("?"), ".");
        assert_eq!(glob_to_regex("/usr/bin/un*"), "/usr/bin/un.*");
    }
}
