#!/bin/sh
# pkq installer: https://github.com/rtczza/pkq
#
#   curl --proto '=https' --tlsv1.2 -sSfL https://github.com/rtczza/pkq/releases/latest/download/install.sh | sh
#
# Environment:
#   PKQ_VERSION   version to install, e.g. 0.2.0 (default: latest)
#   INSTALL_DIR   target directory   (default: /usr/local/bin,
#                                    falls back to $HOME/.local/bin without write access)
#   BASE_URL      download base      (default: GitHub releases)

set -u

GITHUB_REPO="rtczza/pkq"
BASE_URL="${BASE_URL:-https://github.com/${GITHUB_REPO}/releases}"
INSTALL_DIR="${INSTALL_DIR:-}"

info()  { printf '%s\n' "pkq-installer: $*"; }
error() { printf '%s\n' "pkq-installer: error: $*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

fetch_to() {
    if have curl; then
        curl -sSfL -o "$2" "$1"
    elif have wget; then
        wget -qO "$2" "$1"
    else
        error "need curl or wget to download pkq"
    fi
}

detect_arch() {
    case "$(uname -m)" in
        x86_64 | amd64) echo "x86_64-unknown-linux-gnu" ;;
        aarch64 | arm64) echo "aarch64-unknown-linux-gnu" ;;
        *) error "unsupported architecture: $(uname -m)" ;;
    esac
}

# resolve the latest release tag, e.g. v0.2.0 (follows the /latest redirect)
resolve_latest() {
    if have curl; then
        curl -sSfLI -o /dev/null -w '%{url_effective}' "${BASE_URL}/latest"
    else
        wget -q --max-redirect=10 --server-response -O /dev/null "${BASE_URL}/latest" 2>&1 |
            sed -n 's/^[[:space:]]*[Ll]ocation:[[:space:]]*//p' |
            tail -1 |
            tr -d '\r'
    fi
}

# pick the install target: /usr/local/bin when writable (or root),
# otherwise $HOME/.local/bin so that a plain `curl | sh` needs no sudo
pick_install_dir() {
    if [ "$(id -u)" -eq 0 ] || [ -w /usr/local/bin ] ||
        { [ ! -e /usr/local/bin ] && [ -w /usr/local ]; }; then
        echo "/usr/local/bin"
    else
        echo "${HOME}/.local/bin"
    fi
}

main() {
    [ "$(uname -s)" = "Linux" ] || error "pkq only supports Linux"

    if [ -z "$INSTALL_DIR" ]; then
        INSTALL_DIR=$(pick_install_dir)
    fi
    mkdir -p "$INSTALL_DIR" 2>/dev/null ||
        error "cannot create ${INSTALL_DIR}; re-run with sudo, or set INSTALL_DIR to a writable directory"
    [ -w "$INSTALL_DIR" ] ||
        error "cannot write to ${INSTALL_DIR}; re-run with sudo, or set INSTALL_DIR to a writable directory"

    arch=$(detect_arch)

    if [ -n "${PKQ_VERSION:-}" ]; then
        version="$PKQ_VERSION"
    else
        tag=$(resolve_latest) || error "cannot resolve latest version"
        tag="${tag%/}"
        [ -n "$tag" ] || error "cannot resolve latest version"
        version="${tag##*/}"
        case "$version" in
            v[0-9]*) version="${version#v}" ;;
            *) error "cannot resolve latest version (got: ${tag})" ;;
        esac
    fi

    fname="pkq-${version}-${arch}.tar.gz"
    url="${BASE_URL}/download/v${version}/${fname}"

    tmpdir=$(mktemp -d) || error "mktemp failed"
    trap 'rm -rf "$tmpdir"' EXIT INT TERM

    info "downloading pkq ${version} (${arch}) ..."
    fetch_to "$url" "${tmpdir}/${fname}" || error "download failed: ${url}"
    fetch_to "${url}.sha256" "${tmpdir}/${fname}.sha256" || error "download checksum failed: ${url}.sha256"

    if have sha256sum; then
        (cd "$tmpdir" && sha256sum -c "${fname}.sha256" >/dev/null 2>&1) ||
            error "checksum mismatch, aborting"
        info "checksum OK"
    fi

    tar -xzf "${tmpdir}/${fname}" -C "$tmpdir" || error "extract failed"
    [ -f "${tmpdir}/pkq-${version}-${arch}/pkq" ] || error "archive does not contain pkq binary"

    install -m 0755 "${tmpdir}/pkq-${version}-${arch}/pkq" "${INSTALL_DIR}/pkq" || error "install to ${INSTALL_DIR} failed"
    info "installed pkq to ${INSTALL_DIR}/pkq"
    "${INSTALL_DIR}/pkq" --version || error "installed binary does not run (glibc too old?)"

    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            info "note: ${INSTALL_DIR} is not in your PATH"
            info "add it: export PATH=\"${INSTALL_DIR}:\$PATH\"  (e.g. append to ~/.bashrc or ~/.zshrc)"
            ;;
    esac

    info "done. try: pkq --help"
}

main "$@"
