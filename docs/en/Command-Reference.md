# Linux Package Query Command Reference

> This document is compiled from man pages (man7.org, manpages.debian.org),
> the official DNF documentation (dnf.readthedocs.io), and hands-on
> verification on UOS Desktop 20 (DEB) and UOS Server 25 (RPM).
> Verification date: 2026-08-25

---

## Table of Contents

1. [RPM Systems](#1-rpm-systems)
   - 1.1 [rpm](#11-rpm)
   - 1.2 [dnf](#12-dnf)
   - 1.3 [dnf repoquery](#13-dnf-repoquery)
   - 1.4 [yum](#14-yum)
   - 1.5 [repoquery (standalone tool)](#15-repoquery-standalone-tool)
   - 1.6 [rpm2cpio / rpm2archive](#16-rpm2cpio--rpm2archive)
   - 1.7 [rpmkeys / rpmdb / rpmverify](#17-rpmkeys--rpmdb--rpmverify)
2. [DEB Systems](#2-deb-systems)
   - 2.1 [dpkg](#21-dpkg)
   - 2.2 [dpkg-query](#22-dpkg-query)
   - 2.3 [dpkg-deb](#23-dpkg-deb)
   - 2.4 [dpkg-architecture](#24-dpkg-architecture)
   - 2.5 [apt](#25-apt)
   - 2.6 [apt-get](#26-apt-get)
   - 2.7 [apt-cache](#27-apt-cache)
   - 2.8 [apt-file](#28-apt-file)
   - 2.9 [apt-mark](#29-apt-mark)
   - 2.10 [aptitude](#210-aptitude)
3. [Cross-System Command Comparison](#3-cross-system-command-comparison)
4. [pkq Command Reference](#4-pkq-command-reference)

---

## 1. RPM Systems

### 1.1 rpm

**Syntax:** `rpm [options] <operation>`

#### Query Operations (`-q` / `--query`)

**Package selection options (filter the query scope):**

| Option | Description | Example |
|--------|-------------|---------|
| `-a, --all` | Query all installed packages | `rpm -qa` |
| `-f, --file FILE` | Query the package owning the installed FILE | `rpm -qf /usr/bin/bash` |
| `--path PATH` | Query the package owning PATH (works for uninstalled files) | `rpm -q --path /usr/bin/bash` |
| `-g, --group GROUP` | Query packages in the given group | `rpm -qg System/Environment` |
| `-p, --package FILE` | Query an uninstalled .rpm file | `rpm -qp package.rpm` |
| `--pkgid` | Query by package ID | `rpm -q --pkgid 0x1234` |
| `--hdrid` | Query by header ID | `rpm -q --hdrid 0x5678` |
| `--triggeredby PKG` | Query packages triggered by PKG | `rpm -q --triggeredby bash` |
| `--whatprovides CAP` | Query packages providing the capability | `rpm -q --whatprovides "libssl.so.3"` |
| `--whatrequires CAP` | Query packages requiring the capability | `rpm -q --whatrequires bash` |
| `--whatconflicts CAP` | Query packages conflicting with the capability | `rpm -q --whatconflicts bash` |
| `--whatobsoletes CAP` | Query packages obsoleting the capability | `rpm -q --whatobsoletes bash` |
| `--whatrecommends CAP` | Query packages recommending the capability | `rpm -q --whatrecommends bash` |
| `--whatsuggests CAP` | Query packages suggesting the capability | `rpm -q --whatsuggests bash` |
| `--whatsupplements CAP` | Query packages supplementing the capability | `rpm -q --whatsupplements bash` |
| `--whatenhances CAP` | Query packages enhancing the capability | `rpm -q --whatenhances bash` |
| `--tid TID` | Query by transaction ID | `rpm -q --tid 1234567890` |
| `--dupes` | List duplicated installed packages | `rpm -qa --dupes` |
| `--nomanifest` | Do not treat non-package files as manifests | |

**Package info options (what information to output):**

| Option | Description | Example |
|--------|-------------|---------|
| `-i, --info` | Show detailed package information | `rpm -qi bash` |
| `-l, --list` | List files in the package | `rpm -ql bash` |
| `-R, --requires` | List package dependencies | `rpm -qR bash` |
| `--provides` | List capabilities provided by the package | `rpm -q --provides bash` |
| `--conflicts` | List package conflicts | `rpm -q --conflicts bash` |
| `--obsoletes` | List obsoletes | `rpm -q --obsoletes bash` |
| `--recommends` | List recommended dependencies | `rpm -q --recommends bash` |
| `--suggests` | List suggested dependencies | `rpm -q --suggests bash` |
| `--supplements` | List supplemental dependencies | `rpm -q --supplements bash` |
| `--enhances` | List enhanced dependencies | `rpm -q --enhances bash` |
| `--changelog` | Show the change log | `rpm -q --changelog bash` |
| `--changes` | Show changes with full timestamps | `rpm -q --changes bash` |
| `--scripts` | Show install/erase scripts | `rpm -q --scripts bash` |
| `--triggers` | Show trigger scripts | `rpm -q --triggers bash` |
| `--filetriggers` | Show file triggers | `rpm -q --filetriggers bash` |
| `--xml` | Dump metadata as XML | `rpm -q --xml bash` |
| `--last` | Sort by install time | `rpm -qa --last` |
| `--filesbypkg` | List all files per package | `rpm -qa --filesbypkg` |
| `--filecolor` | List file colors (0=noarch, 1=32bit, 2=64bit) | `rpm -q --filecolor bash` |
| `--fileclass` | List file classes (libmagic classification) | `rpm -q --fileclass bash` |
| `--filecaps` | List file POSIX1.e capabilities | `rpm -q --filecaps bash` |
| `--fileprovide` | List file provides | `rpm -q --fileprovide bash` |
| `--filerequire` | List file requires | `rpm -q --filerequire bash` |
| `--dump` | Dump basic file info (implies -l) | `rpm -ql --dump bash` |
| `-s, --state` | Show file states (implies -l) | `rpm -qs bash` |
| `--qf, --queryformat FMT` | Custom output format | `rpm -q --qf '%{NAME}-%{VERSION}\n' bash` |

**File filter options (with -l):**

| Option | Description |
|--------|-------------|
| `-c, --configfiles` | Config files only |
| `-d, --docfiles` | Documentation files only |
| `-L, --licensefiles` | License files only |
| `-A, --artifactfiles` | Artifact files only |
| `--noghost` | Exclude %ghost files |
| `--noconfig` | Exclude %config files |
| `--noartifact` | Exclude artifact files |

**Commonly used `--queryformat` tags:**

```
NAME, VERSION, RELEASE, EPOCH, EPOCHNUM, ARCH, ARCHSUFFIX,
SUMMARY, DESCRIPTION, LICENSE, GROUP, URL, VENDOR, PACKAGER,
SIZE, ARCHIVESIZE, INSTALLSIZE, BUILDTIME, BUILDHOST,
SOURCERPM, NEXTRA, P, NVRA, NEVRA, EVR,
BASENAMES, DIRNAMES, DIRINDEXES, FILECOLORS, FILECLASS,
REQUIRENAME, REQUIREFLAGS, REQUIREVERSION,
PROVIDENAME, PROVIDEFLAGS, PROVIDEVERSION,
CONFLICTNAME, CONFLICTFLAGS, CONFLICTVERSION,
OBSOLETENAME, OBSOLETEFLAGS, OBSOLETEVERSION,
CHANGELOGTIME, CHANGELOGNAME, CHANGELOGTEXT,
FILEDIGESTS, FILESIGNATURELENGTH, CLASSDICT,
PAYLOADFORMAT, PAYLOADCOMPRESSOR, PAYLOADFLAGS,
ENCODING, LONGSIZE, LONGFILESIZES, ...
```

Full tag list: `rpm --querytags`

#### Verify Operations (`-V` / `--verify`)

```
rpm {-V|--verify} [select-options] [verify-options] [PACKAGE_NAME ...]
```

| Option | Description |
|--------|-------------|
| `--nofiledigest` | Do not verify file digests |
| `--nofiles` | Do not verify any file attributes |
| `--nodeps` | Do not verify dependencies |
| `--noscripts` | Do not execute verification scripts |
| `--nosignature` | Do not verify signatures |
| `--nodigest` | Do not verify digests |
| `--nolinkto` | Do not verify linkto |
| `--nomd5` (deprecated) | Do not verify file digests |
| `--nosize` | Do not verify file sizes |
| `--nomtime` | Do not verify modification times |
| `--nomode` | Do not verify file modes |
| `--nordev` | Do not verify device numbers |
| `--nouser` | Do not verify owners |
| `--nogroup` | Do not verify groups |
| `--nocaps` | Do not verify capabilities |

Verify result characters: `S`(size) `M`(mode) `5`(digest) `D`(device) `L`(symlink) `U`(user) `G`(group) `T`(time) `P`(capabilities)

#### Install / Upgrade / Erase

| Operation | Description |
|-----------|-------------|
| `-i, --install` | Install a new package |
| `-U, --upgrade` | Upgrade a package |
| `-F, --freshen` | Upgrade only already-installed packages |
| `--reinstall` | Reinstall |
| `--restore` | Restore file metadata |
| `-e, --erase` | Erase an installed package |

#### Common Options

```
-v, --verbose          Verbose output
-D, --define           Define a macro
-E, --eval             Print macro expansion
--macros=FILE          Read macros from FILE
--nodigest             Do not verify digests
--nosignature          Do not verify signatures
--rcfile=FILE          Read rpmrc from FILE
-r, --root=ROOT        Use ROOT as the top-level directory
--dbpath=DIRECTORY     Use the database in DIRECTORY
--querytags            Show known query tags
--showrc               Show rpmrc and macro configuration
--quiet                Less output
--version              Print version
```

---

### 1.2 dnf

**Syntax:** `dnf [options] <command> [<args>...]`

#### Main Command List

| Command | Alias | Description |
|---------|-------|-------------|
| `alias` | | Manage command aliases |
| `autoremove` | | Remove unneeded dependency packages |
| `check` | | Check the local package database |
| `check-update` | `check-upgrade` | Check for available updates |
| `clean` | | Clean temporary files |
| `deplist` | (deprecated) | Alias of `repoquery --deplist` |
| `distro-sync` | `dsync` | Distro sync |
| `downgrade` | `dg` | Downgrade packages |
| `group` | `grp` | Group management |
| `help` | | Help |
| `history` | `hist` | Transaction history |
| `info` | `if` | Package details |
| `install` | `in` | Install packages |
| `list` | `ls` | Package lists |
| `makecache` | `mc` | Make metadata cache |
| `mark` | | Mark packages |
| `module` | | Module management (deprecated) |
| `provides` | `prov`, `whatprovides`, `wp` | Find providers |
| `reinstall` | `rei` | Reinstall packages |
| `remove` | `rm` | Remove packages |
| `repoinfo` | | Repository info |
| `repolist` | | Repository list |
| `repoquery` | | Repository query |
| `repository-packages` | | Act on packages of a repository |
| `search` | | Search |
| `shell` | | Interactive shell |
| `swap` | | Swap packages |
| `updateinfo` | | Update info |
| `upgrade` | | Upgrade |
| `upgrade-minimal` | | Minimal upgrade |

#### Global Options

| Option | Description |
|--------|-------------|
| `-4` | IPv4 only |
| `-6` | IPv6 only |
| `--advisory=ID` | Filter by advisory ID |
| `--allowerasing` | Allow erasing packages to resolve dependencies |
| `--assumeno` | Automatically answer no |
| `-b, --best` | Try the best available version |
| `--bugfix` | Include bugfix releases |
| `--bz=ID` | Filter by Bugzilla ID |
| `-C, --cacheonly` | Use cache only |
| `--color=COLOR` | Control color (always/never/auto) |
| `--comment=TEXT` | Add a transaction history comment |
| `-c, --config=FILE` | Config file location |
| `--cve=ID` | Filter by CVE ID |
| `-d, --debuglevel=N` | Debug level (0-10) |
| `--debugsolver` | Dump dependency solver debug data |
| `--disableexcludes=...` | Disable exclude configuration |
| `--disableplugin=NAMES` | Disable plugins |
| `--disablerepo=ID` | Temporarily disable a repository |
| `--downloaddir=PATH` | Download directory |
| `--downloadonly` | Download only, do not install |
| `-e, --errorlevel=N` | Error level (0-10) |
| `--enablerepo=ID` | Temporarily enable a repository |
| `--enhancement` | Include enhancement packages |
| `-x, --exclude=SPEC` | Exclude packages |
| `--forcearch=ARCH` | Force the given architecture |
| `-h, --help` | Help |
| `--installroot=PATH` | Install root directory |
| `--newpackage` | Include new packages |
| `--noautoremove` | Disable autoremove |
| `--nobest` | Do not limit to the best candidate |
| `--nodocs` | Do not install documentation |
| `--nogpgcheck` | Skip GPG signature checks |
| `--noplugins` | Disable all plugins |
| `--obsoletes` | Enable obsolete processing |
| `-q, --quiet` | Quiet mode |
| `-R, --randomwait=N` | Maximum wait time (minutes) |
| `--refresh` | Mark metadata as expired |
| `--releasever=VER` | Set the release version |
| `--repo=ID` | Use the given repository only |
| `--repofrompath=REPO,PATH` | Add a temporary repository |
| `--rpmverbosity=NAME` | RPM debug level |
| `--sec-severity=SEV` | Security severity filter |
| `--security` | Security fixes filter |
| `--setopt=OPT=VAL` | Override configuration options |
| `--skip-broken` | Skip problem packages |
| `--showduplicates` | Show duplicate packages |
| `-v, --verbose` | Verbose output |
| `--version` | Show version |
| `-y, --assumeyes` | Automatically answer yes |

#### info Subcommand

```
dnf [options] info [<package-file-spec>...]
```
Shows description and summary information for installed and available
packages.

#### list Subcommand

```
dnf [options] list {--all|--installed|--available|--extras|--obsoletes|--recent|--upgrades|--autoremove} [<package-file-spec>...]
```

#### provides Subcommand

```
dnf [options] provides <provide-spec>
```
Finds packages providing the given capability. Lookup order:
1. Match file provides of all packages
2. Match provides of all packages
3. Assume a command and try the `/usr/bin/`, `/usr/sbin/`, `/bin/`, `/sbin/`
   prefixes
4. Return "No Matches found"

#### search Subcommand

```
dnf [options] search <package-file-spec>...
```
Searches package names and descriptions in three tiers: exact name match,
name and summary match, summary match.

---

### 1.3 dnf repoquery

**Syntax:** `dnf repoquery [options] [KEY ...]`

This is the most important reference tool for pkq.

#### Package Filter Options

| Option | Description |
|--------|-------------|
| `-a, --all` | Query all packages |
| `--show-duplicates` | Show all versions |
| `--arch=ARCH` | Given architectures only |
| `--available` | Available packages only |
| `--installed` | Installed packages only |
| `--extras` | Packages not in any repository only |
| `--upgrades` | Upgradable packages only |
| `--unneeded` | Packages that can be auto-removed only |
| `--userinstalled` | User-installed packages only |
| `--recent` | Recently modified packages only |
| `--duplicates` | Duplicate packages only |
| `--installonly` | install-only packages only |
| `--unsatisfied` | Show packages with unsatisfied dependencies |

#### File / Capability Query Options

| Option | Description |
|--------|-------------|
| `-f, --file FILE` | Show packages containing the given file |
| `--whatconflicts REQ` | Show packages conflicting with REQ |
| `--whatdepends REQ` | Show packages depending on REQ (all types) |
| `--whatobsoletes REQ` | Show packages obsoleting REQ |
| `--whatprovides REQ` | Show packages providing REQ |
| `--whatrequires REQ` | Show packages requiring REQ |
| `--whatrecommends REQ` | Show packages recommending REQ |
| `--whatenhances REQ` | Show packages enhancing REQ |
| `--whatsuggests REQ` | Show packages suggesting REQ |
| `--whatsupplements REQ` | Show packages supplementing REQ |

#### Dependency Query Options

| Option | Description |
|--------|-------------|
| `--alldeps` | Check non-explicit dependencies (files and providers), default |
| `--exactdeps` | Check explicit dependencies only |
| `--recursive` | Recursive query (with --whatrequires/--requires --resolve) |
| `--deplist` | List dependencies and their providers |
| `--resolve` | Resolve the source packages of dependencies |
| `--tree` | Show a recursive dependency tree |

#### Output Format Options

| Option | Description |
|--------|-------------|
| `-i, --info` | Show detailed information |
| `-l, --list` | Show the file list |
| `-s, --source` | Show the source RPM name |
| `--changelogs` | Show changelogs |
| `--qf FMT, --queryformat FMT` | Custom format |
| `--querytags` | Show available tags |
| `--nevra` | Output in NEVRA format |
| `--nvr` | Output in NVR format |
| `--envra` | Output in EVRA format |
| `--groupmember` | Show comps group membership |
| `--location` | Show the download location |

#### Relation Query Options

| Option | Description |
|--------|-------------|
| `--conflicts` | List conflicts |
| `--depends` | List dependencies |
| `--enhances` | List enhances |
| `--provides` | List provides |
| `--recommends` | List recommends |
| `--requires` | List requires |
| `--requires-pre` | List Pre-Depends |
| `--suggests` | List suggests |
| `--supplements` | List supplements |
| `--obsoletes` | List obsoletes |
| `--srpm` | Operate on source RPMs |

#### Available repoquery Tags

```
arch, buildtime, conflicts, debug_name, description, downloadsize,
enhances, epoch, evr, from_repo, group, installsize, installtime,
license, name, obsoletes, packager, provides, reason, recommends,
release, repoid, reponame, requires, size, source_debug_name,
source_name, sourcerpm, suggests, supplements, url, vendor, version
```

---

### 1.4 yum

yum is a compatibility layer for dnf; commands and options are essentially
identical.

Distinctive commands:
- `yum deplist <pkg>` — list dependencies and providers (equivalent to
  `dnf repoquery --deplist`)
- `yum provides <spec>` — equivalent to `dnf provides`

---

### 1.5 repoquery (standalone tool)

`/usr/bin/repoquery` is an alias of `dnf repoquery`; the options are exactly
the same.

---

### 1.6 rpm2cpio / rpm2archive

| Command | Syntax | Description |
|---------|--------|-------------|
| `rpm2cpio` | `rpm2cpio file.rpm` | Convert an RPM into a cpio stream |
| `rpm2archive` | `rpm2archive [OPTIONS] <FILES>` | Convert an RPM into a tar archive |
| | `rpm2archive -n, --nocompression` | Create an uncompressed tar |

---

### 1.7 rpmkeys / rpmdb / rpmverify

| Command | Description |
|---------|-------------|
| `rpmkeys --import KEY` | Import a GPG key |
| `rpmkeys -K, --checksig FILE` | Check package signatures |
| `rpmkeys --list` | List imported keys |
| `rpmdb --initdb` | Initialize the database (deprecated, see rpmdb(8)) |
| `rpmdb --rebuilddb` | Rebuild the database (deprecated) |
| `rpmdb --verifydb` | Verify the database (deprecated) |
| `rpmverify` | Equivalent to `rpm -V` |

---

## 2. DEB Systems

### 2.1 dpkg

**Syntax:** `dpkg [option...] <action>`

#### Query Commands

| Command | Description | Example |
|---------|-------------|---------|
| `-s, --status PKG` | Show detailed package status (local) | `dpkg -s bash` |
| `-p, --print-avail PKG` | Show available-version info | `dpkg -p bash` |
| `-L, --listfiles PKG` | List files installed by the package | `dpkg -L bash` |
| `-l, --list [PATTERN]` | Concisely list packages | `dpkg -l 'libc6*'` |
| `-S, --search PATTERN` | Search packages owning files | `dpkg -S /bin/bash` |
| `-V, --verify [PKG]` | Verify package integrity | `dpkg -V bash` |
| `-C, --audit [PKG]` | Check for broken packages | `dpkg -C` |
| `--get-selections` | Get package selections | `dpkg --get-selections` |
| `--set-selections` | Set package selections | `dpkg --set-selections < file` |
| `--yet-to-unpack` | List packages awaiting unpack | |
| `--predep-package` | List pre-dependencies awaiting unpack | |

#### Install / Remove Commands

| Command | Description |
|---------|-------------|
| `-i, --install FILE` | Install a .deb package |
| `--unpack FILE` | Unpack without configuring |
| `--configure PKG` | Configure an unpacked package |
| `--triggers-only PKG` | Process triggers only |
| `-r, --remove PKG` | Remove a package (keep config) |
| `-P, --purge PKG` | Purge a package (including config) |
| `--update-avail FILE` | Replace available info |
| `--merge-avail FILE` | Merge available info |
| `--clear-avail` | Erase available info |

#### Architecture Commands

| Command | Description |
|---------|-------------|
| `--print-architecture` | Print the architecture |
| `--print-foreign-architectures` | Print foreign architectures |
| `--add-architecture ARCH` | Add an architecture |
| `--remove-architecture ARCH` | Remove an architecture |

#### Version Comparison

```
dpkg --compare-versions ver1 op ver2
```
Operators: `lt le eq ne ge gt` (empty version = earlier),
`lt-nl le-nl ge-nl gt-nl` (empty version = later),
`< << <= = >= >> >` (aliases)

#### Options

| Option | Description |
|--------|-------------|
| `--admindir=DIR` | Use DIR instead of /var/lib/dpkg |
| `--root=DIR` | Installation root |
| `--instdir=DIR` | Installation directory (does not affect the admin dir) |
| `--path-exclude=PATTERN` | Do not install matching paths |
| `--path-include=PATTERN` | Re-include an excluded path |
| `-O, --selected-only` | Process selected packages only |
| `-E, --skip-same-version` | Skip already-installed same versions |
| `-G, --refuse-downgrade` | Refuse downgrades |
| `-B, --auto-deconfigure` | Auto-deconfigure |
| `--no-act, --dry-run, --simulate` | Dry run |
| `-R, --recursive` | Recurse into directories |
| `--force-THING` | Force through problems |
| `--no-force-THING, --refuse-THING` | Stop on problems |
| `--status-fd N` | Send status to a file descriptor |
| `--status-logger=CMD` | Send status to a command |
| `--log=FILE` | Log to a file |
| `--no-pager` | Disable the pager |
| `--no-debsig` | Do not verify signatures |
| `--no-triggers` | Do not run triggers |
| `--triggers` | Cancel --no-triggers |

---

### 2.2 dpkg-query

**Syntax:** `dpkg-query [option...] <command>`

#### Commands

| Command | Description | Example |
|---------|-------------|---------|
| `-s, --status PKG` | Show package status | `dpkg-query -s bash` |
| `-p, --print-avail PKG` | Show available-version info | `dpkg-query -p bash` |
| `-L, --listfiles PKG` | List package files | `dpkg-query -L bash` |
| `-l, --list [PATTERN]` | Concisely list packages | `dpkg-query -l 'bash*'` |
| `-W, --show [PATTERN]` | Custom-format listing | `dpkg-query -W -f='${Package}\t${Version}\n' bash` |
| `-S, --search PATTERN` | Search packages owning files | `dpkg-query -S /bin/bash` |
| `--control-list PKG` | List control files | `dpkg-query --control-list bash` |
| `--control-show PKG FILE` | Show a control file | `dpkg-query --control-show bash conffiles` |
| `-c, --control-path PKG [FILE]` | Show a control file path | |

#### Options

| Option | Description |
|--------|-------------|
| `--admindir=DIR` | Use DIR instead of /var/lib/dpkg |
| `--load-avail` | Load the available file |
| `-f, --showformat=FMT` | Set the --show output format |
| `--no-pager` | Disable the pager |

#### Format String Variables

```
Field variables: Package, Version, Architecture, Priority, Section,
  Origin, Maintainer, Homepage, Installed-Size, Depends,
  Pre-Depends, Recommends, Suggests, Conflicts, Breaks,
  Replaces, Enhances, Provides, Description, Essential,
  Protected, Status, Conffiles, Filename, MD5sum, Size,
  Source, Tag, Triggers-Awaited, Triggers-Pending,
  Config-Version, Bugs, Revision

Virtual variables:
  ${binary:Package}       — Architecture-qualified package name
  ${binary:Synopsis}     — Package synopsis
  ${binary:Summary}       — Same as Synopsis
  ${db:Status-Abbrev}     — Status abbreviation (e.g. "ii ")
  ${db:Status-Want}      — Wanted status
  ${db:Status-Status}    — Current status
  ${db:Status-Eflag}      — Error flag
  ${db-fsys:Files}       — File list
  ${db-fsys:Last-Modified} — Last modified timestamp
  ${source:Package}      — Source package name
  ${source:Version}      — Source package version
  ${source:Upstream-Version} — Source upstream version
```

Escapes: `\n`(newline), `\r`(carriage return), `\t`(tab), `\\`(backslash)

---

### 2.3 dpkg-deb

**Syntax:** `dpkg-deb [option...] <command>`

#### Commands

| Command | Description |
|---------|-------------|
| `-b, --build DIR [ARCHIVE]` | Build a .deb package |
| `-c, --contents ARCHIVE` | List package contents |
| `-I, --info ARCHIVE [CFIELD...]` | Show package info |
| `-W, --show ARCHIVE` | Concisely show package info |
| `-f, --field ARCHIVE [CFIELD...]` | Extract control fields |
| `-e, --control ARCHIVE [DIR]` | Extract control info |
| `-x, --extract ARCHIVE DIR` | Extract files |
| `-X, --vextract ARCHIVE DIR` | Extract and list files |
| `-R, --raw-extract ARCHIVE DIR` | Extract control info and files |
| `--ctrl-tarfile ARCHIVE` | Output the control tar |
| `--fsys-tarfile ARCHIVE` | Output the filesystem tar |

#### Options

| Option | Description |
|--------|-------------|
| `--showformat=FMT` | Set the --show format |
| `--deb-format=FMT` | Archive format (0.939000, 2.0) |
| `--nocheck` | Suppress control file checks |
| `--root-owner-group` | Force owner to root |
| `--[no-]uniform-compression` | Uniform compression |
| `-z#` | Compression level (0-9) |
| `-Z TYPE` | Compression type (gzip, xz, zstd, none) |
| `-S STRATEGY` | Compression strategy |
| `-v, --verbose` | Verbose output |
| `-D, --debug` | Debug output |

---

### 2.4 dpkg-architecture

| Command | Description |
|---------|-------------|
| `-l, --list` | List all variables |
| `-L, --list-known` | List known architectures |
| `-e, --equal ARCH` | Compare against the host architecture |
| `-i, --is WILDCARD` | Match the host architecture |
| `-q, --query VAR` | Print a variable value |
| `-s, --print-set` | Print commands that set environment variables |
| `-u, --print-unset` | Print commands that unset environment variables |
| `-c, --command CMD` | Set the environment and run a command |

---

### 2.5 apt

**Syntax:** `apt [options] <command>`

#### Commands

| Command | Description | Equivalent tool |
|---------|-------------|-----------------|
| `list` | List packages (--installed/--upgradable/--all-versions) | dpkg-query -l |
| `search REGEX` | Search packages by regex | apt-cache search |
| `show PKG` | Show package details | apt-cache show |
| `update` | Update the package index | apt-get update |
| `install PKG` | Install a package | apt-get install |
| `reinstall PKG` | Reinstall | apt-get install --reinstall |
| `remove PKG` | Remove a package | apt-get remove |
| `purge PKG` | Purge a package | apt-get purge |
| `upgrade` | Upgrade | apt-get upgrade |
| `full-upgrade` | Full upgrade | apt-get dist-upgrade |
| `autoremove` | Auto-remove | apt-get autoremove |
| `satisfy DEPS` | Satisfy dependencies | apt-get satisfy |
| `edit-sources` | Edit sources | |

---

### 2.6 apt-get

**Syntax:** `apt-get [options] <command>`

#### Commands

| Command | Description |
|---------|-------------|
| `update` | Fetch updated package lists |
| `upgrade` | Upgrade installed packages |
| `dist-upgrade` | Distribution upgrade |
| `dselect-upgrade` | Follow dselect selections |
| `install PKG[=VER]` | Install packages (optional version) |
| `reinstall PKG` | Reinstall (alias install --reinstall) |
| `remove PKG` | Remove packages |
| `purge PKG` | Purge packages |
| `source PKG[=VER]` | Download source packages |
| `build-dep PKG` | Install build dependencies |
| `satisfy DEPS` | Satisfy dependency strings |
| `check` | Check dependency integrity |
| `download PKG` | Download a binary package to the current directory |
| `clean` | Clean the download cache |
| `autoclean` | Clean old download cache |
| `autoremove` | Auto-remove unneeded dependencies |
| `changelog PKG` | Download and show the changelog |
| `indextargets` | Show index target info |

#### Options

| Option | Description |
|--------|-------------|
| `--no-install-recommends` | Do not install recommended packages |
| `--install-suggests` | Install suggested packages |
| `-d, --download-only` | Download only |
| `-f, --fix-broken` | Fix broken dependencies |
| `-m, --ignore-missing` | Ignore missing packages |
| `--no-download` | Disable downloading |
| `-q, --quiet` | Quiet mode |
| `-s, --simulate` | Dry run |
| `-y, --yes` | Automatic yes |
| `--assume-no` | Automatic no |
| `--no-show-upgraded` | Do not show upgraded list |
| `-V, --verbose-versions` | Show full version numbers |
| `-a, --host-architecture ARCH` | Host architecture |
| `-P, --build-profiles PROFILES` | Build profiles |
| `-b, --compile` | Compile source packages |
| `--ignore-hold` | Ignore holds |
| `--with-new-pkgs` | Allow installing new packages |
| `--no-upgrade` | Do not upgrade |
| `--only-upgrade` | Upgrade already-installed only |
| `--allow-downgrades` | Allow downgrades |
| `--allow-remove-essential` | Allow removing essential packages |
| `--allow-change-held-packages` | Allow changing held packages |
| `--print-uris` | Print URIs instead of downloading |
| `--purge` | Use purge instead of remove |
| `--reinstall` | Reinstall |
| `--list-cleanup` | Auto-clean the list |
| `-t, --target-release REL` | Target release |
| `--trivial-only` | Perform trivial operations only |
| `--mark-auto` | Mark as auto-installed |
| `--no-remove` | Refuse removal |
| `--auto-remove` | Auto-remove |
| `--only-source` | Source package names only |
| `--diff-only` | Download diffs only |
| `--dsc-only` | Download dsc only |
| `--tar-only` | Download tar only |
| `--arch-only` | Architecture-dependent dependencies only |
| `--indep-only` | Architecture-independent dependencies only |
| `--allow-unauthenticated` | Allow unauthenticated |
| `--show-progress` | Show progress |
| `--with-source FILE` | Add a metadata source |
| `-e any, --error-on=any` | Fail on any error |

---

### 2.7 apt-cache

**Syntax:** `apt-cache [options] <command>`

#### Commands

| Command | Description | Example |
|---------|-------------|---------|
| `gencaches` | Build the package cache | |
| `showpkg PKG` | Show package info (forward and reverse deps) | `apt-cache showpkg bash` |
| `showsrc PKG` | Show the source package record | `apt-cache showsrc bash` |
| `stats` | Show cache statistics | |
| `dump` | Briefly list all packages | |
| `dumpavail` | Print the available list | |
| `unmet` | Show unmet dependencies | |
| `search REGEX` | Search names and descriptions by regex | `apt-cache search python` |
| `show PKG` | Show the package record (all versions) | `apt-cache show bash` |
| `depends PKG` | Show package dependencies | `apt-cache depends bash` |
| `rdepends PKG` | Show reverse dependencies | `apt-cache rdepends bash` |
| `pkgnames [PREFIX]` | List all package names | `apt-cache pkgnames` |
| `dotty PKG` | Generate GraphViz output | |
| `xvcg PKG` | Generate VCG output | |
| `policy [PKG]` | Show version priorities | `apt-cache policy bash` |
| `madison PKG` | madison-format version table | |

#### Options

| Option | Description |
|--------|-------------|
| `-p, --pkg-cache FILE` | Package cache file |
| `-s, --src-cache FILE` | Source cache file |
| `-q, --quiet` | Quiet |
| `-i, --important` | Important dependencies only (Depends/Pre-Depends) |
| `--no-pre-depends` | Do not show Pre-Depends |
| `--no-depends` | Do not show Depends |
| `--no-recommends` | Do not show Recommends |
| `--no-suggests` | Do not show Suggests |
| `--no-conflicts` | Do not show Conflicts |
| `--no-breaks` | Do not show Breaks |
| `--no-replaces` | Do not show Replaces |
| `--no-enhances` | Do not show Enhances |
| `--implicit` | Show implicit dependencies |
| `-f, --full` | Full records in search output |
| `-a, --all-versions` | Show all versions |
| `-g, --generate` | Auto-generate the cache |
| `--names-only, -n` | Search package names only |
| `--all-names` | Show all names (including virtual) |
| `--recurse` | Recurse depends/rdepends |
| `--installed` | Installed packages only |
| `--with-source FILE` | Add a metadata source |

---

### 2.8 apt-file

**Syntax:** `apt-file [options] <action>`

#### Commands

| Command | Description | Example |
|---------|-------------|---------|
| `search PATTERN` / `find` | Search which package contains the given file | `apt-file search /bin/bash` |
| `show PACKAGE` / `list` | List files in a package | `apt-file list bash` |
| `list-indices` | List known Contents indices | |
| `update` | Update indices (invokes apt update) | |

#### Options

| Option | Description |
|--------|-------------|
| `-a, --architecture ARCH` | Architecture |
| `-c, --config-file FILE` | Config file |
| `-D, --from-deb FILE` | Read search patterns from a .deb file |
| `-f, --from-file FILE` | Read search patterns from a file |
| `--filter-origins ORIGIN` | Filter by origin |
| `--filter-suites SUITE` | Filter by suite |
| `-F, --fixed-string` | Do not expand wildcards |
| `--index-names TYPE` | Filter by index name |
| `-I TYPE` | Same as --index-names |
| `-i, --ignore-case` | Ignore case |
| `-l, --package-only` | Package names only |
| `-o, --option OPT=VAL` | Set a configuration option |
| `--substring-match` | Substring matching (search default) |
| `-v, --verbose` | Verbose mode |
| `-x, --regexp` | Use regular expressions |
| `-h, --help` | Help |

#### Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success with results |
| 1 | Success without results |
| 2 | Error |
| 3 | Empty cache |
| 4 | No matching index in the cache |
| 255 | Internal error |

---

### 2.9 apt-mark

**Syntax:** `apt-mark [options] {auto|manual} PKG...`

| Command | Description |
|---------|-------------|
| `auto PKG` | Mark as auto-installed |
| `manual PKG` | Mark as manually installed |
| `minimize-manual` | Mark metapackage dependencies as auto |
| `hold PKG` | Place a hold |
| `unhold PKG` | Remove a hold |
| `showauto` | List auto-installed packages |
| `showmanual` | List manually installed packages |
| `showhold` | List held packages |

---

### 2.10 aptitude

**Syntax:** `aptitude [options] <action> ...`

#### Query-Related Actions

| Action | Description |
|--------|-------------|
| `search EXPR` | Search packages by name/expression |
| `show PKG` | Show package details |
| `showsrc PKG` | Show source package details |
| `versions PKG` | Show version info |
| `why PKG` | Explain why PKG should be installed |
| `why-not PKG` | Explain why PKG should not be installed |
| `changelog PKG` | Show the changelog |
| `download PKG` | Download the .deb file |
| `source PKG` | Download the source package |

#### Options

| Option | Description |
|--------|-------------|
| `-F FORMAT` | Search result format |
| `-O ORDER` | Search result ordering |
| `-w WIDTH` | Display width |
| `-Z` | Show size changes |
| `-D` | Show dependency relations |
| `-V` | Show versions |
| `-v` | Extra information |
| `-s` | Simulate |
| `-d` | Download only |
| `-P` | Always prompt |
| `-y` | Automatic yes |
| `-f` | Fix broken packages |

---

## 3. Cross-System Command Comparison

### 3.1 Package Details

| Scenario | RPM | DEB | Output differences |
|----------|-----|-----|--------------------|
| Installed package details | `rpm -qi PKG` | `dpkg -s PKG` / `apt show PKG` | RPM includes Install Date/Signature; DEB includes Installed-Size/Download-Size |
| Repo package details | `dnf info PKG` / `dnf repoquery -i PKG` | `apt show PKG` / `apt-cache show PKG` | RPM includes repo origin/size; DEB includes APT-Sources/Download-Size |
| All versions | `dnf repoquery --show-duplicates PKG` | `apt-cache show PKG` (shows all versions by default) | |

### 3.2 File Lists

| Scenario | RPM | DEB |
|----------|-----|-----|
| Installed package files | `rpm -ql PKG` | `dpkg -L PKG` / `dpkg-query -L PKG` |
| Repo package files | `dnf repoquery -l PKG` | `apt-file list PKG` |
| Config files only | `rpm -qc PKG` | `dpkg -L PKG \| grep /etc` (no direct option) |
| Doc files only | `rpm -qd PKG` | `dpkg -L PKG \| grep /usr/share/doc` (no direct option) |
| Files + size + time | `rpm -ql --dump PKG` | No direct equivalent |

### 3.3 File Ownership

| Scenario | RPM | DEB |
|----------|-----|-----|
| Installed file ownership | `rpm -qf /path` | `dpkg -S /path` / `dpkg-query -S /path` |
| Uninstalled file ownership | `dnf provides /path` | `apt-file search /path` |
| Command ownership | `dnf provides CMD` (adds prefixes automatically) | `apt-file search CMD` |
| Capability ownership | `dnf provides "libssl.so.3"` | `apt-file search libssl.so.3` |

### 3.4 Dependencies

| Scenario | RPM | DEB |
|----------|-----|-----|
| Forward deps (local) | `rpm -qR PKG` | `apt-cache depends PKG` |
| Forward deps (repo) | `dnf repoquery --requires PKG` | `apt-cache depends PKG` (from cache) |
| Deps + providers | `dnf repoquery --deplist PKG` / `yum deplist PKG` | No direct equivalent |
| Recursive deps | `dnf repoquery --requires --recursive --resolve PKG` | `apt-cache depends --recurse PKG` |
| Reverse deps (local) | `rpm -q --whatrequires PKG` | `apt-cache rdepends PKG` |
| Reverse deps (repo) | `dnf repoquery --whatrequires PKG` | `apt-cache rdepends PKG` (from cache) |
| Recursive reverse deps | `dnf repoquery --whatrequires --recursive PKG` | `apt-cache rdepends --recurse PKG` / `apt-rdepends -r PKG` |
| Important deps only | None | `apt-cache depends -i PKG` (Depends/Pre-Depends only) |
| Filter by type | None | `--no-recommends`, `--no-suggests`, etc. |

### 3.5 Search

| Scenario | RPM | DEB |
|----------|-----|-----|
| By keyword | `dnf search KW` | `apt-cache search KW` / `apt search KW` |
| By name | `dnf search --name KW` | `apt-cache search --names-only KW` |
| By regex | No direct equivalent | `apt-cache search 'regex'` |
| By file | `dnf provides /path` | `apt-file search /path` |

### 3.6 Provides (capabilities)

| Scenario | RPM | DEB |
|----------|-----|-----|
| What a package provides | `rpm -q --provides PKG` | See the Provides field in `apt-cache show PKG` |
| Who provides a capability | `rpm -q --whatprovides CAP` / `dnf provides CAP` | `apt-cache search CAP` (indirect) |

### 3.7 Source Packages

| Scenario | RPM | DEB |
|----------|-----|-----|
| Source package name of a package | See the Source RPM field in `rpm -qi PKG` | See the Source field in `dpkg -s PKG` |
| Source package name in repo | `dnf repoquery --source PKG` / `dnf repoquery -s PKG` | `apt-cache showsrc PKG` (requires deb-src) |
| Source → binaries | Look up the sourcerpm field in repo metadata | Look up the Source field in Packages |

### 3.8 Change Logs

| Scenario | RPM | DEB |
|----------|-----|-----|
| Local | `rpm -q --changelog PKG` | `apt changelog PKG` / `apt-get changelog PKG` |
| Repo | `dnf repoquery --changelogs PKG` | `apt changelog PKG` |
| Full timestamps | `rpm -q --changes PKG` | |

### 3.9 Package Lists

| Scenario | RPM | DEB |
|----------|-----|-----|
| All installed | `rpm -qa` | `dpkg -l` / `dpkg-query -W` / `apt list --installed` |
| Sorted by time | `rpm -qa --last` | No direct equivalent |
| All available | `dnf repoquery --available` / `dnf list --all` | `apt list` (all) |
| Upgradable | `dnf list --upgrades` | `apt list --upgradable` |
| Packages in a repo | `dnf repoquery --available` | `apt-cache pkgnames` |
| Duplicates | `rpm -qa --dupes` | None |

### 3.10 Repository Management

| Scenario | RPM | DEB |
|----------|-----|-----|
| Repository list | `dnf repolist` | `apt-cache policy` |
| Repository details | `dnf repoinfo` / `dnf repo info` | No direct equivalent |
| Package count per repo | `dnf repolist -v` | No direct equivalent |

### 3.11 Custom Output Formats

| Scenario | RPM | DEB |
|----------|-----|-----|
| Tag list | `rpm --querytags` | None (fixed field list) |
| Custom format | `rpm -q --queryformat='FMT' PKG` | `dpkg-query -W -f='FMT' PKG` |
| Format variables | `%{NAME}`, `%{VERSION}` ... (uppercase) | `${Package}`, `${Version}` ... (mixed case) |
| Tag case | Uppercase | Mixed case |

### 3.12 Verification

| Scenario | RPM | DEB |
|----------|-----|-----|
| Verify an installed package | `rpm -V PKG` | `dpkg -V PKG` |
| Verify all packages | `rpm -Va` | `dpkg -V` |
| Verify digests | `rpm -V --nofiledigest PKG` | `dpkg -V --verify-format=rpm PKG` |

### 3.13 Package File Operations

| Scenario | RPM | DEB |
|----------|-----|-----|
| Query an uninstalled package | `rpm -qp FILE.rpm` | `dpkg-deb -I FILE.deb` / `dpkg-deb -c FILE.deb` |
| Extract files | `rpm2cpio FILE.rpm \| cpio -id` | `dpkg-deb -x FILE.deb DIR` |
| Extract + list | `rpm2cpio FILE.rpm \| cpio -t` | `dpkg-deb -X FILE.deb DIR` |
| Control info | `rpm -qp --scripts FILE.rpm` | `dpkg-deb -e FILE.deb DIR` |
| Filesystem tar | No direct equivalent | `dpkg-deb --fsys-tarfile FILE.deb` |

---

## 4. pkq Command Reference

pkq (Package Query) provides a unified query syntax on top of rpm/dnf and
dpkg/apt. The following documents **every command and option actually
implemented in v0.2.0**.

### Global Options

| Option | Description | Default |
|--------|-------------|---------|
| `-o, --output FORMAT` | Output format (human / json) | human |
| `--refresh` | Force-refresh the repository metadata cache | false |
| `--offline` | Offline mode, local cache only | false |
| `--cache-ttl SECONDS` | Cache TTL | 86400 (24h) |
| `--legacy-exit-code` | Compatibility mode: always exit 0 | false |

Exit-code semantics: `0` hit / `1` not found or partially unsuccessful / `2` error.

### 4.1 info — Package Details

```bash
pkq info <pkg> [--repo]
```

Local-first lookup; uninstalled packages automatically fall back to the
repository and print the install command (`apt-get install` / `dnf install`).

### 4.2 list — Package File List

```bash
pkq list <pkg> [--all] [--repo]
```

Output is sorted by path (files of the same directory are grouped together).
Common directories such as `.`, `/usr`, `/etc` are filtered by default;
`--all` shows the full list. Uninstalled packages automatically trigger a
repository lookup with install guidance.

### 4.3 owns — File Ownership

```bash
pkq owns <path> [--all] [--repo]
```

Supports absolute paths and globs (`*/bin/unzip`). For shared directories
such as `/etc`, ownership lists are truncated at 50 entries by default;
`--all` shows all.

### 4.4 deps — Forward Dependencies

```bash
pkq deps <pkg> [--repo]
```

Output is grouped into `Depends / Recommends / Suggests / Conflicts /
Replaces`. On RPM, sonames and `config(...)` capabilities are resolved to real
package names with version constraints preserved (e.g. `glibc (>= 2.17)`).

### 4.5 rdeps — Reverse Dependencies

```bash
pkq rdeps <pkg> [--all] [--installed-only] [--repo]
```

Semantics aligned with `dnf repoquery --whatrequires`: matching covers the
target package's name, explicit provides and primary binary paths, restricted
to strong dependencies (Requires). Truncated at 50 entries by default;
`--all` shows all. The bottom summary always matches the list exactly.

### 4.6 search — Search

```bash
pkq search <keyword> [--regex] [--names-only] [--files-only]
           [--all] [-i, --installed] [--repo] [--max-files N]
```

- Keyword matching covers package names and summaries (no substring matching
  over description bodies, to avoid noise)
- Word-boundary relevance ranking: exact/prefix/substring name matches >
  summary word-boundary matches > fuzzy hits
- A pattern starting with `/` or containing globs is equivalent to `owns`
  (local misses automatically fall back to the repository)
- Fuzzy summary hits (e.g. sudo→sudoku) are collapsed into a single-line
  "Related" section; `--all` expands it
- `--installed-only` (alias `-i`) searches installed packages only

### 4.7 source — Source/Binary Package Lookup

```bash
pkq source <pkg>
```

Auto-detects whether the input is a binary or source package name: lists all
binary packages generated by the source package, tagged `[installed]` /
`[not installed]` (exact name+version+release+arch matching). Sort order:
main same-name package first > installed first > primary arch > alphabetical.

### 4.8 changelog — Change Log

```bash
pkq changelog <pkg> [--repo]
```

When no local changelog exists, the repository is searched automatically
(RPM fetches repodata other.xml).

### 4.9 cache — Cache Management

```bash
pkq cache status                    # Per-directory cache usage
pkq cache update                    # Force-refresh repository metadata
pkq cache clean [target] [--yes]    # Clean: all | index | repos | contents
```

- `cache update` prints per-source progress: `✓` fetched online,
  `△` fetch failed but fell back to the local cache (with reason, e.g.
  `HTTP 401 (authentication failed)` / `timed out`), `✗` failed outright
- **A source that could not be refreshed counts as a failure**: a ⚠ line is
  printed and the exit code is `1` (script-detectable)
- `cache clean` requires `--yes` unless you want the confirmation prompt

### 4.10 Exit Codes and JSON

| Exit code | Meaning |
|-----------|---------|
| `0` | Query hit / update fully successful |
| `1` | Not found / partially unsuccessful (e.g. some sources failed to refresh) |
| `2` | Error (invalid arguments, network and cache both unavailable, etc.) |

`--output json` provides JSON output for info/deps/rdeps/search/owns;
`--legacy-exit-code` restores the v0.1 behavior of always exiting 0.
