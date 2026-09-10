//! pkq 关键路径基准（criterion）。
//!
//! 全部基准使用公共 API 与合成数据，离线可复现，不依赖真实系统包管理器状态。
//! 覆盖报告 §7.3 关注的解析/缓存热路径：
//! - `deb_control_parse`：DEB control 段落解析（本地库加载主成本）
//! - `pkg_index_cache_load`：postcard 解析缓存反序列化（含 PKQ1 魔数校验）
//! - `rpm_repo_cache_load`：RPM 仓库解析缓存反序列化
//! - `repo_parser_expand_vars`：dnf 变量展开（仓库配置解析热路径）
//! - `path_glob_match`：文件归属 glob 匹配
//!
//! 运行：`cargo bench`（如 `cargo bench --bench pkq_benches`）。

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};

use pkq::backend::deb::local::parse_control_paragraphs;
use pkq::backend::rpm::repo_parser::{expand_vars, parse_mirrorlist_lines};
use pkq::cache::{PkgIndexCache, RpmRepoCache};
use pkq::model::{Dependency, PkgMetadata};
use pkq::path_util::glob_match;

/// 构造一个字段完整的合成包（文件/依赖/provides 均有数据）。
fn sample_package(i: usize) -> PkgMetadata {
    PkgMetadata {
        name: format!("pkg-{}", i),
        version: format!("1.{}.0", i % 50),
        release: "1.uos".into(),
        arch: "x86_64".into(),
        summary: format!("summary for package {} with keywords nginx sudo", i),
        description: "long description line for benchmarking. ".repeat(8),
        requires: (0..8)
            .map(|k| Dependency {
                name: format!("lib{}.so.{}", k, k),
                version: Some("1.0".into()),
                flags: Some("GE".into()),
                is_alternative: false,
            })
            .collect(),
        provides: vec![format!("pkg-{}.so", i), format!("virtual-{}", i)],
        files: (0..40)
            .map(|k| format!("/usr/lib/pkg{}/file{}.so", i, k))
            .collect(),
        ..Default::default()
    }
}

fn bench_deb_control_parse(c: &mut Criterion) {
    // 合成 ~5000 个 control 段落（贴近中大型发行版 status 规模）。
    let mut text = String::with_capacity(5000 * 320);
    for i in 0..5000 {
        text.push_str(&format!(
            "Package: pkg-{i}\nStatus: install ok installed\nVersion: 1.{}.0\nArchitecture: amd64\n\
             Maintainer: Bench <bench@example.com>\nInstalled-Size: {}\nSection: utils\nPriority: optional\n\
             Homepage: https://example.com/pkg-{i}\nDescription: package {i}\n long description line one.\n .\n line two.\n\
             Depends: libc6 (>= 2.31), libssl3\nRecommends: ca-certificates\nProvides: pkg-{i}-abi\n\n",
            i % 50,
            100 + i % 500
        ));
    }
    c.bench_function("deb_control_parse/5000_paragraphs", |b| {
        b.iter(|| black_box(parse_control_paragraphs(black_box(&text))))
    });
}

fn bench_cache_load(c: &mut Criterion) {
    let packages: Vec<PkgMetadata> = (0..5000).map(sample_package).collect();
    let dir = std::env::temp_dir().join("pkq-bench-cache");
    std::fs::create_dir_all(&dir).ok();
    let idx_path = dir.join("index.bin");
    let repo_path = dir.join("repo.bin");
    PkgIndexCache::save(&idx_path, packages.clone()).expect("save index cache");
    RpmRepoCache::save(&repo_path, 1_690_000_000, packages).expect("save repo cache");
    let no_sources: [PathBuf; 0] = [];

    c.bench_function("pkg_index_cache_load/5000_pkgs", |b| {
        b.iter(|| {
            black_box(PkgIndexCache::load(
                black_box(&idx_path),
                0,
                false,
                black_box(&no_sources),
            ))
        })
    });

    c.bench_function("rpm_repo_cache_load/5000_pkgs", |b| {
        b.iter(|| black_box(RpmRepoCache::load(black_box(&repo_path), 1_690_000_000)))
    });
}

fn bench_repo_parser(c: &mut Criterion) {
    use std::collections::HashMap;
    let mut vars = HashMap::new();
    vars.insert("releasever".to_string(), "25".to_string());
    vars.insert("basearch".to_string(), "x86_64".to_string());
    let url = "https://mirror.example.com/uos/$releasever/${basearch}/os/";

    c.bench_function("repo_parser_expand_vars", |b| {
        b.iter(|| black_box(expand_vars(black_box(url), black_box(&vars))))
    });

    let mirrorlist = (0..200)
        .map(|i| format!("https://mirror{}.example.com/repo/", i))
        .collect::<Vec<_>>()
        .join("\n");
    c.bench_function("repo_parser_mirrorlist/200_lines", |b| {
        b.iter(|| black_box(parse_mirrorlist_lines(black_box(&mirrorlist))))
    });
}

fn bench_path_glob(c: &mut Criterion) {
    let pattern = "*/bin/bash";
    let path = "/usr/bin/bash";
    c.bench_function("path_glob_match", |b| {
        b.iter(|| black_box(glob_match(black_box(pattern), black_box(path))))
    });
}

criterion_group!(
    benches,
    bench_deb_control_parse,
    bench_cache_load,
    bench_repo_parser,
    bench_path_glob
);
criterion_main!(benches);
