use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RepoConfig {
    pub id: String,
    pub name: String,
    pub baseurl: Option<String>,
    pub mirrorlist: Option<String>,
    pub metalink: Option<String>,
    pub enabled: bool,
    pub gpgcheck: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn parse_repo_files() -> Vec<RepoConfig> {
    let mut repos = Vec::new();
    // 系统级 + 用户级（~/.config/pkq/repos/，无需 root 即可定义仓库）
    let mut repo_dirs = vec![PathBuf::from("/etc/yum.repos.d")];
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            repo_dirs.push(PathBuf::from(home).join(".config/pkq/repos"));
        }
    }

    for repos_dir in repo_dirs {
        if let Ok(entries) = std::fs::read_dir(&repos_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "repo" {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            repos.extend(parse_repo_content(&content));
                        }
                    }
                }
            }
        }
    }

    repos
}

fn parse_repo_content(content: &str) -> Vec<RepoConfig> {
    let mut repos = Vec::new();
    let mut current: Option<RepoConfig> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(r) = current.take() {
                repos.push(r);
            }
            let id = &line[1..line.len() - 1];
            current = Some(RepoConfig {
                id: id.to_string(),
                name: id.to_string(),
                baseurl: None,
                mirrorlist: None,
                metalink: None,
                enabled: true,
                gpgcheck: false,
                username: None,
                password: None,
            });
        } else if let Some(ref mut repo) = current {
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim();

                match key {
                    "name" => repo.name = val.to_string(),
                    "baseurl" => repo.baseurl = Some(val.to_string()),
                    "mirrorlist" => repo.mirrorlist = Some(val.to_string()),
                    "metalink" => repo.metalink = Some(val.to_string()),
                    "enabled" => repo.enabled = val != "0",
                    "gpgcheck" => repo.gpgcheck = val != "0",
                    "username" if !val.starts_with('$') => {
                        repo.username = Some(val.to_string());
                    }
                    "password" if !val.starts_with('$') => {
                        repo.password = Some(val.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(r) = current {
        repos.push(r);
    }

    repos
}

pub fn resolve_url(baseurl: &str) -> String {
    let vars = load_dnf_vars();
    expand_vars(baseurl, &vars)
        .trim_end_matches('/')
        .to_string()
}

/// 汇总 dnf/yum 变量：/etc/dnf/vars/*、/etc/yum/vars/*、dnf.conf releasever、
/// os-release VERSION_ID 兜底，外加运行时 arch/basearch（P0-5）。
pub fn load_dnf_vars() -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;
    let mut vars: HashMap<String, String> = HashMap::new();

    let arch = std::env::consts::ARCH;
    vars.insert("arch".into(), arch.into());
    vars.insert("basearch".into(), arch.into());

    for dir in ["/etc/dnf/vars", "/etc/yum/vars"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.is_empty() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    let val = content.trim().to_string();
                    if !val.is_empty() {
                        vars.entry(name).or_insert(val);
                    }
                }
            }
        }
    }

    if !vars.contains_key("releasever") {
        if let Some(rv) = std::fs::read_to_string("/etc/dnf/dnf.conf")
            .ok()
            .and_then(|c| releasever_from_conf(&c))
        {
            vars.insert("releasever".into(), rv);
        }
    }
    if !vars.contains_key("releasever") {
        if let Some(rv) = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|c| releasever_from_os_release(&c))
        {
            vars.insert("releasever".into(), rv);
        }
    }

    vars
}

/// dnf.conf [main] 段的 releasever= 取值
pub fn releasever_from_conf(conf: &str) -> Option<String> {
    let mut in_main = false;
    for line in conf.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_main = line.eq_ignore_ascii_case("[main]");
            continue;
        }
        if in_main {
            if let Some(v) = line.strip_prefix("releasever=") {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// os-release 的 VERSION_ID 取值
pub fn releasever_from_os_release(os_release: &str) -> Option<String> {
    for line in os_release.lines() {
        if let Some(v) = line.strip_prefix("VERSION_ID=") {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// 展开 ${var} 与 $var（$var 后不跟字母数字才算完整变量）；未知变量保留原文
pub fn expand_vars(input: &str, vars: &std::collections::HashMap<String, String>) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            // 非 ASCII 按字符拷贝
            let ch_len = utf8_len(bytes[i]);
            out.push_str(&input[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        // ${name}
        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find('}') {
                let name = &input[i + 2..i + 2 + end];
                if let Some(v) = vars.get(name) {
                    out.push_str(v);
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            out.push('$');
            i += 1;
            continue;
        }
        // $name（遇到非字母数字下划线结束）
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > i + 1 {
            let name = &input[i + 1..j];
            if let Some(v) = vars.get(name) {
                out.push_str(v);
                i = j;
                continue;
            }
        }
        out.push('$');
        i += 1;
    }
    out
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// 解析 metalink XML，返回候选镜像 URL（按 preference 降序）
pub fn parse_metalink_urls(content: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(content);
    let mut buf = Vec::new();
    let mut candidates: Vec<(i64, String)> = Vec::new();
    let mut in_url = false;
    let mut preference: i64 = 0;
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                if e.name().as_ref() == b"url" {
                    in_url = true;
                    text.clear();
                    preference = 0;
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"preference" {
                            preference = String::from_utf8_lossy(&attr.value)
                                .trim()
                                .parse()
                                .unwrap_or(0);
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if in_url {
                    text.push_str(&t.decode().unwrap_or_default());
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                if e.name().as_ref() == b"url" {
                    in_url = false;
                    let url = text.trim().to_string();
                    if url.starts_with("https://") || url.starts_with("http://") {
                        candidates.push((preference, url));
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    candidates.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
    candidates.into_iter().map(|(_, u)| u).collect()
}

/// 解析纯文本 mirrorlist，返回候选镜像 URL（保持文件顺序）
pub fn parse_mirrorlist_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| l.starts_with("https://") || l.starts_with("http://"))
        .map(|l| l.to_string())
        .collect()
}

/// 镜像 URL 归一为仓库 baseurl：剥掉末尾 repodata/repomd.xml（metalink 指向实际文件）
pub fn baseurl_from_mirror_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    match trimmed.strip_suffix("repodata/repomd.xml") {
        Some(base) => base.trim_end_matches('/').to_string(),
        None => trimmed.to_string(),
    }
}

/// 解析出的镜像 URL 列表是否为 metalink XML（而非纯文本 mirrorlist）
pub fn looks_like_metalink_xml(content: &str) -> bool {
    let head = content.trim_start();
    head.starts_with('<') || head.starts_with("<?xml") || head.contains("<metalink")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_repo() {
        let content = r#"
[example]
name = Example Repo
baseurl = https://example.com/repo/$basearch/
enabled = 1
gpgcheck = 0

[disabled-repo]
name = Disabled
baseurl = https://disabled.com/repo/
enabled = 0
"#;
        let repos = parse_repo_content(content);
        assert_eq!(repos.len(), 2);
        assert!(repos[0].enabled);
        assert!(!repos[1].enabled);
        assert_eq!(
            repos[0].baseurl.as_deref(),
            Some("https://example.com/repo/$basearch/")
        );
    }

    #[test]
    fn test_resolve_url() {
        let url = resolve_url("https://example.com/repo/$basearch/");
        assert!(url.contains("x86_64") || url.contains("aarch64"));
        assert!(!url.ends_with('/'));
    }

    #[test]
    fn test_expand_vars() {
        let mut vars = HashMap::new();
        vars.insert("releasever".to_string(), "25".to_string());
        vars.insert("basearch".to_string(), "x86_64".to_string());
        assert_eq!(
            expand_vars("https://m.com/$releasever/${basearch}/os", &vars),
            "https://m.com/25/x86_64/os"
        );
        // 未知变量保留原文
        assert_eq!(
            expand_vars("https://m.com/$unknown", &vars),
            "https://m.com/$unknown"
        );
        // $ 后跟非变量字符保留
        assert_eq!(expand_vars("https://m.com/a$b", &vars), "https://m.com/a$b");
        // 变量名边界：$releaseverX 不误读为 $releasever
        assert_eq!(expand_vars("$releaseverX", &vars), "$releaseverX");
    }

    #[test]
    fn test_releasever_parsers() {
        assert_eq!(
            releasever_from_conf("[main]\nreleasever=25\ngpgcheck=1\n"),
            Some("25".to_string())
        );
        assert_eq!(releasever_from_conf("[other]\nreleasever=9\n"), None);
        assert_eq!(
            releasever_from_os_release("NAME=\"UOS\"\nVERSION_ID=\"25\"\n"),
            Some("25".to_string())
        );
    }

    #[test]
    fn test_baseurl_from_mirror_url() {
        assert_eq!(
            baseurl_from_mirror_url("https://m.com/pub/fedora/40/x86_64/os/repodata/repomd.xml"),
            "https://m.com/pub/fedora/40/x86_64/os"
        );
        assert_eq!(
            baseurl_from_mirror_url("https://m.com/repo/"),
            "https://m.com/repo"
        );
    }

    #[test]
    fn test_parse_mirrorlist_lines() {
        let content = "# comment\nhttps://m1.com/repo/\nhttp://m2.com/repo/\nftp://skip.com\n";
        let urls = parse_mirrorlist_lines(content);
        assert_eq!(urls, vec!["https://m1.com/repo/", "http://m2.com/repo/"]);
    }

    #[test]
    fn test_parse_metalink_urls() {
        let xml = r#"<?xml version="1.0"?>
<metalink>
  <files>
    <file name="repomd.xml">
      <url type="https" preference="100" location="cn">https://fast.com/os/repodata/repomd.xml</url>
      <url type="https" preference="50" location="us">https://slow.com/os/repodata/repomd.xml</url>
      <url type="rsync" preference="90">rsync://skip.com/</url>
    </file>
  </files>
</metalink>"#;
        assert!(looks_like_metalink_xml(xml));
        let urls = parse_metalink_urls(xml);
        assert_eq!(urls[0], "https://fast.com/os/repodata/repomd.xml");
        assert_eq!(urls.len(), 2);
    }
}
