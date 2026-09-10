pub mod common;
pub mod deb;
pub mod rpm;

use crate::error::{PkgError, Result};
use crate::model::*;

pub trait PkgBackend: Send + Sync {
    fn system_type(&self) -> PackageSystem;

    fn get_package_details(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<PkgMetadata>>;

    fn list_files(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<String>>;

    fn find_file_owner(
        &self,
        file_path: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<String>>;

    fn get_dependencies(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<Dependency>>;

    fn get_reverse_dependencies(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<ReverseDep>>;

    fn search_packages(
        &self,
        keyword: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<PkgMetadata>>;

    fn search_by_file(&self, file_path: &str, cfg: &CacheConfig) -> Result<Vec<SearchResult>>;

    fn get_source_package(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<SourcePackageInfo>>;

    fn get_changelog(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<ChangelogEntry>>;

    fn search_by_pattern(
        &self,
        pattern: &str,
        use_regex: bool,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<SearchResult>>;

    fn resolve_dep_name(&self, _dep_name: &str) -> Option<String> {
        None
    }

    /// 强制刷新仓库元数据缓存，返回分源统计与包总数。
    /// RefreshStats.failed + fallback > 0 表示有源未刷新成功（已回退本地缓存）。
    fn refresh_metadata(
        &self,
        _cfg: &CacheConfig,
    ) -> Result<crate::backend::common::RefreshReport> {
        Err(PkgError::NetworkError("refresh not supported".into()))
    }
}
