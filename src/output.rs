use crate::cli::OutputFormat;
use crate::i18n::Language;
use crate::model::*;
use colored::*;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_width::UnicodeWidthStr;

const KEY_WIDTH: usize = 16;

/// JSON 输出是否压缩为单行（`--compact` 全局选项，默认美化缩进）。
static JSON_COMPACT: AtomicBool = AtomicBool::new(false);

/// 设置 JSON 压缩模式（进程启动时调用一次，默认 false = 现状）。
pub fn set_json_compact(compact: bool) {
    JSON_COMPACT.store(compact, Ordering::Relaxed);
}

/// 统一的 JSON 序列化入口：按全局开关选择紧凑或美化输出。
fn json_fmt<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    if JSON_COMPACT.load(Ordering::Relaxed) {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
}

fn lang() -> Language {
    Language::current()
}

fn pad_right(s: &str, target_width: usize) -> String {
    let w = s.width();
    if w >= target_width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target_width - w))
    }
}

/// 单行摘要截断（按字符边界，CJK 安全），防止超长描述刷屏
fn truncate_line(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{}...", head.trim_end())
}

pub fn print_package_details(
    metadata: Option<PkgMetadata>,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    match metadata {
        Some(pkg) => match fmt {
            OutputFormat::Json => println!("{}", json_fmt(&pkg)?),
            OutputFormat::Human => {
                let l = lang();
                println!("{} {}", pkg.name.cyan().bold(), pkg.version.dimmed());
                println!();

                print_field(l.field_name(), &pkg.name);
                if let Some(e) = &pkg.epoch {
                    print_field(l.field_epoch(), e);
                }
                print_field(l.field_version(), &pkg.version);
                if !pkg.release.is_empty() {
                    print_field(l.field_release(), &pkg.release);
                }
                print_field(l.field_arch(), &pkg.arch);
                if let Some(v) = &pkg.priority {
                    print_field(l.field_priority(), v);
                }
                if let Some(v) = &pkg.group {
                    print_field(l.field_section(), v);
                }
                if let Some(v) = &pkg.source_pkg {
                    print_field(l.field_source(), &v.green().to_string());
                }
                if let Some(v) = &pkg.packager {
                    print_field(l.field_maintainer(), v);
                }
                if let Some(v) = &pkg.license {
                    print_field(l.field_license(), v);
                }
                if let Some(v) = &pkg.vendor {
                    print_field(l.field_vendor(), v);
                }
                print_field(l.field_summary(), &pkg.summary);
                if !pkg.description.is_empty() {
                    print_field(l.field_description(), "");
                    for line in pkg.description.lines() {
                        if line.is_empty() {
                            println!();
                        } else {
                            println!("    {}", line);
                        }
                    }
                }
                if let Some(v) = &pkg.url {
                    print_field(l.field_homepage(), &v.blue().underline().to_string());
                }
                if let Some(v) = pkg.size {
                    print_field(l.field_download_size(), &format_size(v));
                }
                if let Some(v) = pkg.install_size {
                    print_field(l.field_install_size(), &format_size(v));
                }
                if !pkg.provides.is_empty() {
                    print_field(l.field_provides(), &pkg.provides.join(", "));
                }
            }
        },
        None => match fmt {
            OutputFormat::Json => println!("null"),
            OutputFormat::Human => eprintln!("{}", lang().pkg_not_found()),
        },
    }
    Ok(())
}

fn print_field(key: &str, value: &str) {
    let padded = pad_right(key, KEY_WIDTH);
    if value.is_empty() {
        println!("{}:", padded.bright_cyan().bold());
    } else {
        println!("{}: {}", padded.bright_cyan().bold(), value);
    }
}

pub fn print_file_list(files: &[String], fmt: OutputFormat) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(files)?),
        OutputFormat::Human => {
            for f in files {
                println!("{}", f);
            }
        }
    }
    Ok(())
}

pub fn print_string_list(list: &[String], fmt: OutputFormat) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(list)?),
        OutputFormat::Human => {
            for s in list {
                println!("{}", s);
            }
        }
    }
    Ok(())
}

/// 归属包输出（P0-9）：human 带来源标签，JSON 含 source 字段。
/// items: (包名, source)，source 取值 "installed" / "repo"
pub fn print_file_owners(
    items: &[(String, String)],
    path: &str,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => {
            let arr: Vec<serde_json::Value> = items
                .iter()
                .map(|(name, source)| {
                    serde_json::json!({
                        "package": name,
                        "path": path,
                        "source": source,
                    })
                })
                .collect();
            println!("{}", json_fmt(&arr)?);
        }
        OutputFormat::Human => {
            for (name, source) in items {
                println!("{} {}", name.cyan().bold(), source_tag(l, source));
            }
        }
    }
    Ok(())
}

pub fn print_directory_owners(
    owners: &[(String, String)],
    path: &str,
    total: usize,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => {
            let mut map = serde_json::Map::new();
            map.insert(
                "directory".into(),
                serde_json::Value::String(path.to_string()),
            );
            map.insert("total".into(), serde_json::json!(total));
            let arr: Vec<serde_json::Value> = owners
                .iter()
                .map(|(name, source)| serde_json::json!({"package": name, "source": source}))
                .collect();
            map.insert("packages".into(), serde_json::Value::Array(arr));
            println!("{}", json_fmt(&map)?);
        }
        OutputFormat::Human => {
            println!("{}", l.dir_shared_by(path, total));
            for (owner, source) in owners {
                println!(
                    "  {} {} {}",
                    "•".green(),
                    owner.cyan(),
                    source_tag(l, source)
                );
            }
        }
    }
    Ok(())
}

/// 来源标签渲染（单点维护）
fn source_tag(l: Language, source: &str) -> ColoredString {
    match source {
        "installed" => l.installed_tag().green(),
        "repo" => l.repo_pkg_tag().yellow(),
        other => other.normal(),
    }
}

/// 缓存占用输出（P1-4）
pub fn print_cache_status(
    dir: &str,
    items: &[(String, u64)],
    total: u64,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => {
            let obj = serde_json::json!({
                "dir": dir,
                "items": items.iter().map(|(p, s)| serde_json::json!({"path": p, "bytes": s})).collect::<Vec<_>>(),
                "total_bytes": total,
            });
            println!("{}", json_fmt(&obj)?);
        }
        OutputFormat::Human => {
            println!("{} {}", l.cache_dir_label(), dir);
            for (path, size) in items {
                println!("  {:<40} {:>10}", path, format_size(*size));
            }
            println!("  {:<40} {:>10}", l.cache_total_label(), format_size(total));
        }
    }
    Ok(())
}

pub fn print_dependencies(deps: &[Dependency], fmt: OutputFormat) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(deps)?),
        OutputFormat::Human => {
            for d in deps {
                print_dep_line(d, "  ");
            }
        }
    }
    Ok(())
}

pub fn print_full_dependencies(pkg: &PkgMetadata, fmt: OutputFormat) -> crate::error::Result<()> {
    if fmt == OutputFormat::Json {
        let mut map = serde_json::Map::new();
        map.insert("depends".into(), serde_json::to_value(&pkg.requires)?);
        map.insert("recommends".into(), serde_json::to_value(&pkg.recommends)?);
        map.insert("suggests".into(), serde_json::to_value(&pkg.suggests)?);
        map.insert("conflicts".into(), serde_json::to_value(&pkg.conflicts)?);
        map.insert("breaks".into(), serde_json::to_value(&pkg.obsoletes)?);
        map.insert("replaces".into(), serde_json::to_value(&pkg.replaces)?);
        println!("{}", json_fmt(&map)?);
        return Ok(());
    }
    let is_zh = lang() == Language::Zh;
    let mut printed_any = false;

    let mut print_section = |title_zh: &str, title_en: &str, deps: &[Dependency]| {
        if !deps.is_empty() {
            println!("{}", if is_zh { title_zh } else { title_en });
            for d in deps {
                if let (Some(flag), Some(ver)) = (&d.flags, &d.version) {
                    if !flag.is_empty() && !ver.is_empty() {
                        println!("     {} ({} {})", d.name, flag, ver);
                        continue;
                    }
                }
                if let Some(ver) = &d.version {
                    if !ver.is_empty() {
                        println!("     {} ({})", d.name, ver);
                        continue;
                    }
                }
                println!("     {}", d.name);
            }
            printed_any = true;
        }
    };

    print_section("依赖", "Depends", &pkg.requires);
    print_section("推荐", "Recommends", &pkg.recommends);
    print_section("建议", "Suggests", &pkg.suggests);
    print_section("冲突", "Conflicts", &pkg.conflicts);
    print_section("替换", "Replaces", &pkg.replaces);

    if !printed_any {
        println!(
            "{}",
            if is_zh {
                "无依赖关系"
            } else {
                "No dependencies"
            }
        );
    }
    Ok(())
}

pub fn print_full_dependencies_resolved<F>(
    pkg: &PkgMetadata,
    fmt: OutputFormat,
    resolver: F,
) -> crate::error::Result<()>
where
    F: Fn(&str) -> Option<String>,
{
    let l = lang();
    let resolve_deps = |deps: &[Dependency]| -> Vec<Dependency> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut result = Vec::new();
        for d in deps {
            let resolved = resolver(&d.name).unwrap_or_else(|| d.name.clone());
            if resolved == pkg.name {
                continue;
            }
            if resolved.contains('/') || !resolved.chars().any(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            if !seen.insert(resolved.clone()) {
                continue;
            }
            result.push(Dependency {
                name: resolved,
                version: d.version.clone(),
                flags: d.flags.clone(),
                is_alternative: d.is_alternative,
            });
        }
        result
    };
    let requires = resolve_deps(&pkg.requires);
    let recommends = resolve_deps(&pkg.recommends);
    let suggests = resolve_deps(&pkg.suggests);
    let conflicts = resolve_deps(&pkg.conflicts);
    let obsoletes = resolve_deps(&pkg.obsoletes);
    let replaces = resolve_deps(&pkg.replaces);
    match fmt {
        OutputFormat::Json => {
            let mut map = serde_json::Map::new();
            map.insert("depends".into(), serde_json::to_value(&requires)?);
            map.insert("recommends".into(), serde_json::to_value(&recommends)?);
            map.insert("suggests".into(), serde_json::to_value(&suggests)?);
            map.insert("conflicts".into(), serde_json::to_value(&conflicts)?);
            map.insert("breaks".into(), serde_json::to_value(&obsoletes)?);
            map.insert("replaces".into(), serde_json::to_value(&replaces)?);
            println!("{}", json_fmt(&map)?);
        }
        OutputFormat::Human => {
            print_dep_section(l.dep_depends(), &requires, colored::Color::Cyan);
            print_dep_section(l.dep_recommends(), &recommends, colored::Color::Blue);
            print_dep_section(l.dep_suggests(), &suggests, colored::Color::Blue);
            print_dep_section(l.dep_conflicts(), &conflicts, colored::Color::Red);
            print_dep_section(l.dep_breaks(), &obsoletes, colored::Color::Red);
            print_dep_section(l.dep_replaces(), &replaces, colored::Color::Yellow);
        }
    }
    Ok(())
}

pub fn print_resolved_dependencies(
    deps: &[Dependency],
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => {
            let mut map = serde_json::Map::new();
            map.insert("depends".into(), serde_json::to_value(deps)?);
            println!("{}", json_fmt(&map)?);
        }
        OutputFormat::Human => {
            print_dep_section(l.dep_depends(), deps, colored::Color::Cyan);
        }
    }
    Ok(())
}

fn print_dep_section(name: &str, deps: &[Dependency], color: colored::Color) {
    if deps.is_empty() {
        return;
    }
    println!("{}", name.color(color).bold());
    for d in deps {
        print_dep_line(d, "  ");
    }
}

fn print_dep_line(d: &Dependency, indent: &str) {
    let prefix = if d.is_alternative { " | " } else { "   " };
    let name_str = d.name.cyan();
    let rest: ColoredString = if let Some(v) = &d.version {
        let flag = d.flags.as_deref().unwrap_or("");
        format!(" ({} {})", flag, v).dimmed()
    } else {
        String::new().dimmed()
    };
    println!("{}{}{}{}", indent, prefix, name_str, rest);
}

pub fn print_reverse_dependencies(
    rdeps: &[ReverseDep],
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(rdeps)?),
        OutputFormat::Human => {
            for r in rdeps {
                let arch = r.arch.as_deref().unwrap_or("");
                let tag = if r.source_repo.is_some() {
                    l.repo_tag().yellow()
                } else {
                    l.installed_tag().green()
                };
                println!(
                    "{}-{}.{} {}",
                    r.pkg_name.cyan().bold(),
                    r.version,
                    arch,
                    tag
                );
            }
        }
    }
    Ok(())
}

pub fn print_search_results(
    results: &[PkgMetadata],
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(results)?),
        OutputFormat::Human => {
            for p in results {
                println!("{}: {}", p.name.cyan().bold(), p.summary);
            }
        }
    }
    Ok(())
}

pub fn print_search_partitioned(
    keywords: &[SearchResult],
    related: &[SearchResult],
    files: &[SearchResult],
    file_limit: usize,
    show_all: bool,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match fmt {
        OutputFormat::Json => {
            let mut map = serde_json::Map::new();
            map.insert("packages".into(), serde_json::to_value(keywords)?);
            map.insert("related".into(), serde_json::to_value(related)?);
            map.insert("files".into(), serde_json::to_value(files)?);
            println!("{}", json_fmt(&map)?);
        }
        OutputFormat::Human => {
            if !keywords.is_empty() {
                println!("{}", l.packages_header(keywords.len()).bright_cyan().bold());
                let name_w = keywords
                    .iter()
                    .map(|r| r.pkg_name.len())
                    .max()
                    .unwrap_or(0)
                    .max(10);
                for r in keywords {
                    let tag = match r.source.as_str() {
                        "installed" => l.installed_tag().green(),
                        "repo" => l.repo_tag().yellow(),
                        _ => r.source.normal(),
                    };
                    println!(
                        "  {} {:<12} {}",
                        pad_right(&r.pkg_name, name_w).cyan().bold(),
                        tag,
                        truncate_line(&r.matched_text, 80)
                    );
                }
            }
            // 模糊命中折叠段：默认单行列出包名，--all 展开为完整列表
            if !related.is_empty() {
                if !keywords.is_empty() {
                    println!();
                }
                if show_all {
                    println!("{}", l.related_header(related.len()).bright_cyan().bold());
                    let name_w = related
                        .iter()
                        .map(|r| r.pkg_name.len())
                        .max()
                        .unwrap_or(0)
                        .max(10);
                    for r in related {
                        let tag = match r.source.as_str() {
                            "installed" => l.installed_tag().green(),
                            "repo" => l.repo_tag().yellow(),
                            _ => r.source.normal(),
                        };
                        println!(
                            "  {} {:<12} {}",
                            pad_right(&r.pkg_name, name_w).cyan().bold(),
                            tag,
                            truncate_line(&r.matched_text, 80)
                        );
                    }
                } else {
                    let names: Vec<&str> = related.iter().map(|r| r.pkg_name.as_str()).collect();
                    println!(
                        "{} {}",
                        l.related_header(related.len()).bright_cyan().bold(),
                        truncate_line(&names.join(", "), 100)
                    );
                }
            }
            if !files.is_empty() {
                if !keywords.is_empty() {
                    println!();
                }
                let mut grouped: std::collections::BTreeMap<(String, String), Vec<&str>> =
                    std::collections::BTreeMap::new();
                for r in files {
                    grouped
                        .entry((r.pkg_name.clone(), r.source.clone()))
                        .or_default()
                        .push(r.matched_text.as_str());
                }
                println!("{}", l.files_header(grouped.len()).bright_cyan().bold());
                for ((pkg_name, source), paths) in &grouped {
                    let tag = match source.as_str() {
                        "installed" => l.installed_tag().green(),
                        "repo" => l.repo_tag().yellow(),
                        _ => source.normal(),
                    };
                    println!("  {} {}", pkg_name.cyan().bold(), tag);
                    let display_count = if file_limit > 0 {
                        file_limit
                    } else {
                        paths.len()
                    };
                    for path in paths.iter().take(display_count) {
                        println!("      {}", path);
                    }
                    if file_limit > 0 && paths.len() > file_limit {
                        println!("      {}", l.more_files(paths.len() - file_limit).dimmed());
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn print_file_owners_grouped(
    grouped: &std::collections::BTreeMap<String, Vec<String>>,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => {
            let mut map = serde_json::Map::new();
            for (pkg, files) in grouped {
                map.insert(pkg.clone(), serde_json::to_value(files)?);
            }
            println!("{}", json_fmt(&map)?);
        }
        OutputFormat::Human => {
            for (pkg, files) in grouped {
                println!("{}", pkg.cyan().bold());
                for f in files {
                    println!("    {}", f);
                }
            }
        }
    }
    Ok(())
}

pub fn print_source_package(
    info: Option<SourcePackageInfo>,
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    let l = lang();
    match info {
        Some(src) => match fmt {
            OutputFormat::Json => println!("{}", json_fmt(&src)?),
            OutputFormat::Human => {
                println!("{} {}", src.name.cyan().bold(), src.version.dimmed());
                println!();
                print_field(l.field_source(), &src.name.green().to_string());
                if !src.release.is_empty() {
                    print_field(l.field_release(), &src.release);
                }
                if let Some(v) = &src.license {
                    print_field(l.field_license(), v);
                }
                if let Some(v) = &src.url {
                    print_field(l.field_homepage(), &v.blue().underline().to_string());
                }
                if let Some(v) = &src.maintainer {
                    print_field(l.field_maintainer(), v);
                }
                if !src.binaries.is_empty() {
                    println!();
                    println!("{}", l.binary_packages().bright_cyan().bold());
                    let name_w = src
                        .binaries
                        .iter()
                        .map(|b| b.name.width())
                        .max()
                        .unwrap_or(0)
                        .max(20);
                    for b in &src.binaries {
                        let status = if b.installed {
                            l.installed_tag().green()
                        } else {
                            l.repo_tag().yellow()
                        };
                        let arch_tag = pad_right(&b.arch, 6);
                        println!(
                            "  {} {} {} {}",
                            pad_right(&b.name, name_w).cyan(),
                            status,
                            arch_tag,
                            b.version
                        );
                    }
                }
            }
        },
        None => match fmt {
            OutputFormat::Json => println!("null"),
            OutputFormat::Human => eprintln!("{}", l.source_not_found("")),
        },
    }
    Ok(())
}

pub fn print_changelog(
    changelog: &[ChangelogEntry],
    fmt: OutputFormat,
) -> crate::error::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", json_fmt(changelog)?),
        OutputFormat::Human => {
            for entry in changelog {
                let dt = format_timestamp(entry.timestamp);
                if dt.is_empty() {
                    println!("{} {}", "*".green(), entry.author.cyan());
                } else {
                    println!("{} {} {}", "*".green(), dt.yellow(), entry.author.cyan());
                }
                for line in entry.text.lines() {
                    println!("    {}", line);
                }
                println!();
            }
        }
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// 供 i18n 等模块使用的公开字号格式化
pub fn format_size_public(bytes: u64) -> String {
    format_size(bytes)
}

fn format_timestamp(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    let days_since_epoch = ts / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    let mut y = 1970;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let yd = if leap { 366 } else { 365 };
        if d < 0 {
            y -= 1;
            let leap2 = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            d += if leap2 { 366 } else { 365 };
        } else if d >= yd {
            d -= yd;
            y += 1;
        } else {
            break;
        }
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mdays = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0;
    let mut remaining = d;
    while m < 12 && remaining >= mdays[m] {
        remaining -= mdays[m];
        m += 1;
    }
    (y, (m + 1) as i64, (remaining + 1))
}
