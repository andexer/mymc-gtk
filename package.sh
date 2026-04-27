#!/usr/bin/env bash
# =============================================================================
# package.sh — Build mymc-gtk and produce AppImage, .deb and .rpm packages
# Usage: ./package.sh [--skip-build] [--deb-only] [--rpm-only] [--appimage-only]
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "${SCRIPT_DIR}")"
DIST_DIR="${SCRIPT_DIR}/dist"
TARGET_RELEASE="${SCRIPT_DIR}/target/release"
BINARY_NAME="mymc-gtk"
VERSION="$(grep '^version' "${SCRIPT_DIR}/Cargo.toml" | head -1 | awk -F'"' '{print $2}')"
ARCH="x86_64"

LINUXDEPLOY="/tmp/linuxdeploy-x86_64.AppImage"
APPIMAGETOOL="/tmp/appimagetool-x86_64.AppImage"

# ── Colour helpers ────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()    { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ── Parse flags ───────────────────────────────────────────────────────────────
SKIP_BUILD=false; DO_DEB=true; DO_RPM=true; DO_APPIMAGE=true
for arg in "$@"; do
    case "$arg" in
        --skip-build)    SKIP_BUILD=true ;;
        --deb-only)      DO_RPM=false; DO_APPIMAGE=false ;;
        --rpm-only)      DO_DEB=false; DO_APPIMAGE=false ;;
        --appimage-only) DO_DEB=false; DO_RPM=false ;;
        *) warn "Unknown flag: $arg" ;;
    esac
done

mkdir -p "${DIST_DIR}"
cd "${SCRIPT_DIR}"

# ── 1. Release build ──────────────────────────────────────────────────────────
if [[ "${SKIP_BUILD}" == false ]]; then
    info "Building ${BINARY_NAME} v${VERSION} in release mode…"
    cargo build --release
    info "Build complete → ${TARGET_RELEASE}/${BINARY_NAME}"
else
    info "Skipping build (--skip-build)"
fi

[[ -f "${TARGET_RELEASE}/${BINARY_NAME}" ]] || error "Binary not found: ${TARGET_RELEASE}/${BINARY_NAME}"

# ── 2. .deb ───────────────────────────────────────────────────────────────────
if [[ "${DO_DEB}" == true ]]; then
    info "Generating .deb package…"
    # cargo-deb does NOT allow license-file to be missing; create stub if needed
    [[ -f LICENSE ]] || echo "GPL-2.0" > LICENSE

    cargo deb --no-build --output "${DIST_DIR}/"
    DEB_FILE=$(ls "${DIST_DIR}"/*.deb | head -1)
    info ".deb ready: ${DEB_FILE}"
    info "  Inspect with: dpkg -I ${DEB_FILE}"
fi

# ── 3. .rpm ───────────────────────────────────────────────────────────────────
if [[ "${DO_RPM}" == true ]]; then
    info "Generating .rpm package…"
    cargo generate-rpm --output "${DIST_DIR}/"
    RPM_FILE=$(ls "${DIST_DIR}"/*.rpm | head -1)
    info ".rpm ready: ${RPM_FILE}"
    info "  Inspect with: rpm -qip ${RPM_FILE}"
fi

# ── 4. AppImage ───────────────────────────────────────────────────────────────
if [[ "${DO_APPIMAGE}" == true ]]; then
    info "Building AppImage…"

    [[ -x "${LINUXDEPLOY}" ]]   || error "linuxdeploy not found at ${LINUXDEPLOY}. Run:\n  wget -P /tmp https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage && chmod +x /tmp/linuxdeploy-x86_64.AppImage"
    [[ -x "${APPIMAGETOOL}" ]]  || error "appimagetool not found at ${APPIMAGETOOL}. Run:\n  wget -P /tmp https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage && chmod +x /tmp/appimagetool-x86_64.AppImage"

    APPDIR="${SCRIPT_DIR}/AppDir"
    rm -rf "${APPDIR}"

    # ── Directory layout ──────────────────────────────────────────────────────
    install -Dm755 "${TARGET_RELEASE}/${BINARY_NAME}" "${APPDIR}/usr/bin/${BINARY_NAME}"
    install -Dm644 "assets/mymc-gtk.desktop"          "${APPDIR}/usr/share/applications/mymc-gtk.desktop"
    install -Dm644 "assets/mymc-gtk.png"              "${APPDIR}/usr/share/icons/hicolor/256x256/apps/mymc-gtk.png"
    # NOTE: appdata is added AFTER linuxdeploy to avoid triggering appstreamcli

    # Copy Python core so PYTHONPATH works from inside the AppImage
    PYCORE_DEST="${APPDIR}/usr/share/${BINARY_NAME}/python_core"
    mkdir -p "${PYCORE_DEST}"
    for py in ps2mc.py ps2save.py ps2mc_dir.py ps2mc_ecc.py lzari.py sjistab.py; do
        [[ -f "${ROOT_DIR}/${py}" ]] && cp "${ROOT_DIR}/${py}" "${PYCORE_DEST}/"
    done

    # Custom AppRun (sets PYTHONPATH, GTK paths)
    install -m755 "assets/AppRun" "${APPDIR}/AppRun"

    # Top-level symlinks expected by AppImage spec
    ln -sf "usr/share/icons/hicolor/256x256/apps/mymc-gtk.png" "${APPDIR}/mymc-gtk.png"
    ln -sf "usr/share/applications/mymc-gtk.desktop"           "${APPDIR}/mymc-gtk.desktop"

    # ── Deploy shared libraries via linuxdeploy ───────────────────────────────
    info "Deploying shared libraries…"
    # DISABLE_STRIP=1: linuxdeploy's bundled strip can't handle Fedora's .relr.dyn ELF sections
    # Suppress copyright/dpkg-query warnings (dpkg not available on Fedora)
    DISABLE_STRIP=1 "${LINUXDEPLOY}" --appimage-extract-and-run \
        --appdir="${APPDIR}" \
        --executable="${TARGET_RELEASE}/${BINARY_NAME}" \
        --desktop-file="assets/mymc-gtk.desktop" \
        --icon-file="assets/mymc-gtk.png" \
        2>&1 | grep -Ev "^$|Could not find copyright|dpkg-query|Calling strip|Strip call failed|Unable to recognise" || true

    # Add appdata AFTER linuxdeploy (avoids triggering its internal appstreamcli call)
    install -Dm644 "assets/mymc-gtk.appdata.xml" "${APPDIR}/usr/share/metainfo/mymc-gtk.appdata.xml"

    # ── Pack into AppImage ────────────────────────────────────────────────────
    APPIMAGE_OUT="${DIST_DIR}/${BINARY_NAME}-${VERSION}-${ARCH}.AppImage"
    # --no-appstream: skip redundant appstreamcli validation (already validated above)
    ARCH="${ARCH}" "${APPIMAGETOOL}" --appimage-extract-and-run \
        --no-appstream \
        "${APPDIR}" "${APPIMAGE_OUT}" 2>&1 | grep -Ev "^$|hint:|consider submitting"
    chmod +x "${APPIMAGE_OUT}"

    info "AppImage ready: ${APPIMAGE_OUT}"
    rm -rf "${APPDIR}"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
info "═══════════════════════════════════════════"
info "  Distribution packages in: ${DIST_DIR}/"
ls -lh "${DIST_DIR}/" 2>/dev/null || true
info "═══════════════════════════════════════════"
