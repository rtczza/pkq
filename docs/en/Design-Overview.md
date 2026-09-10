# pkq Design Overview

> This document describes the post-refactoring high-level architecture of pkq,
> including the native network IO module and metadata cache TTL management that
> solve the stale-local-cache problem. Audience: system architects and Rust
> engineers.

## 1. Background

In multi-distro operations and image building, local package manager caches
(`/var/cache/apt`, `/var/cache/dnf`) often go stale when users have not run
`apt update` / `dnf makecache` for a long time. A query tool that only reads
local disk caches will miss newly published packages, report mismatched
versions, or produce broken dependency information. Therefore pkq must have
native real-time network synchronization: when needed it fetches the latest
`repomd.xml` or `Packages` index from official/mirror sources, keeping query
results strictly consistent with the live repository.

## 2. Goals and Non-Goals

Goals:

- [x] Clear cache strategy: TTL-based local metadata cache plus `--refresh`
  forced refresh.
- [x] Unified commands: one command set behaves identically on DEB and RPM.
- [x] Read-only: never installs, removes, or mutates system state.

Non-goals (out of scope):

- Install/remove/upgrade: pkq is a read-only query tool
- Verify: per-file checksum verification is complex and rarely needed
- Package building: a developer tool, not an ops scenario
- Transaction history: owned by the package manager itself
- Module/group management: distro-specific, not uniform across distros
- Repository management: package manager configuration territory
- Downloading package payloads: never downloads `.rpm`/`.deb` files
- Subprocess invocation: strictly no `apt`/`dnf`/`curl`/`wget` subprocesses

## 3. User Profile

Ops engineers and sysadmins working in mixed RPM/DEB environments who want one
uniform query tool. Core pain points: inconsistent query commands across
distros + stale local caches requiring network refresh.

## 4. Command Set (v0.2.0)

| # | Subcommand | Description | Native equivalent |
|---|------------|-------------|-------------------|
| 1 | `info <pkg> [--repo]` | Package details (name, version, description, size, arch, license…) | rpm -qi / dpkg -s |
| 2 | `list <pkg> [--all] [--repo]` | File list (path-sorted) | rpm -ql / dpkg -L |
| 3 | `owns <path> [--all] [--repo]` | File ownership (globs; shared dirs truncated) | rpm -qf / dpkg -S |
| 4 | `deps <pkg> [--repo]` | Forward dependencies (RPM resolves capabilities to real packages) | rpm -qR / apt-cache depends |
| 5 | `rdeps <pkg> [--all] [--installed-only] [--repo]` | Reverse dependencies (strong requires + capability set, aligned with dnf repoquery --whatrequires) | rpm --whatrequires / apt-cache rdepends |
| 6 | `search <pattern> [--regex] [--all] [-i] [--repo]` | Keyword (name+summary) + file-path dual index; word-boundary relevance ranking | dnf search / apt-file search |
| 7 | `source <pkg>` | Source/binary package lookup (exact-version install status) | dnf repoquery --source / dpkg -s Source |
| 8 | `changelog <pkg> [--repo]` | Change log | rpm -q --changelog / apt changelog |
| 9 | `cache status\|update\|clean` | Cache management (usage / force refresh / classified clean) | dnf makecache (partial) |

> Since v0.2.0 the truncation escape hatch is unified as `--all` (lifts the
> default 50-entry / 5-files-per-package folding). Exit-code semantics:
> `0` hit / `1` not found or partially unsuccessful / `2` error.

Out-of-scope items remain: no install/remove/upgrade, no verify, no package
building, no transaction history, no module/group management, no repository
management, no payload downloads, no subprocesses.

## 5. System Architecture

A Network & Cache layer at the bottom gives the tool a
"local database + online metadata" dual-drive model.

```
+-----------------------------------------------------------------------------------+
|                                  CLI layer (clap)                                 |
+-----------------------------------------------------------------------------------+
                                          |
                                          v
+-----------------------------------------------------------------------------------+
|                     Command dispatch & orchestration (engine)                     |
|        engine/mod.rs (Ctx + dispatch) · engine/commands/* (one file per command)  |
|        engine/support.rs (banner / word-boundary relevance / file query helpers)  |
+-----------------------------------------------------------------------------------+
                                          |
                                          v
+-----------------------------------------------------------------------------------+
|                     Unified backend abstraction (PkgBackend Trait)                |
|       backend/common.rs (page limit / network-error compression / refresh stats)  |
+-----------------------------------------------------------------------------------+
                                /                   \
                               /                     \
                              v                       v
+------------------------------------+ +------------------------------------+
|           RPM adapter              | |            DEB adapter             |
| - Local: rpmdb (NDB/BDB/SQLite)    | | - Local: /var/lib/dpkg/status      |
| - Remote: repomd.xml + primary.xml | | - Remote: Packages.gz / Sources    |
+------------------------------------+ +------------------------------------+
                   \                             /
                    v                           v
+-----------------------------------------------------------------------------------+
|                       Network sync & cache layer (Network & Cache)                |
|  - Sync HTTP client (ureq)                                                        |
|  - Local storage (~/.cache/pkq/)                                                  |
|  - TTL validation & HTTP ETag / Last-Modified check                               |
|  - Three-level fallback: stale reuse → parsed cache (postcard) → load_any         |
+-----------------------------------------------------------------------------------+
                                          |
                                          v
+-----------------------------------------------------------------------------------+
|                          Unified data model (PkgMetadata)                         |
+-----------------------------------------------------------------------------------+
                                          |
                                          v
+-----------------------------------------------------------------------------------+
|                            Rendering layer (Human / JSON)                         |
+-----------------------------------------------------------------------------------+
```

## 6. Query Lifecycle Example

`pkq info nginx --repo`:

1. CLI parses arguments, builds `CacheConfig`.
2. Engine dispatches to the RPM backend; `NetworkCacheManager` is engaged.
3. The manager validates the TTL (24h default) and HTTP headers
   (ETag / If-Modified-Since) for the repo index under `~/.cache/pkq/`.
4. If expired or `--refresh` was passed, the native HTTP client (ureq) fetches
   the fresh metadata archive into the cache.
5. The index is parsed into the unified `PkgMetadata` model and rendered.

## 7. Constraints

- Outbound HTTP/HTTPS (80/443) must be reachable; when the network is down and
  a cache exists, the tool degrades to stale data (silent for queries).
- HTTP timeout defaults to 10 seconds with automatic stale-cache fallback.
