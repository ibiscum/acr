#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")" || exit

ALLOW_LOW_SPACE="${ALLOW_LOW_SPACE:-auto}"
HOST_ARCH="${HOST_ARCH:-}"
BUILD_ARCH="${BUILD_ARCH:-$(dpkg --print-architecture)}"

print_usage() {
    cat <<'EOF'
Usage: ./build.sh [options]

Options:
  --allow-low-space        Disable sbuild CHECK_SPACE guard for this run.
  --host <arch>            Cross-build host architecture (for example amd64).
  --build <arch>           Build architecture (defaults to local dpkg arch).
    --force                  Compatibility no-op.
  -h, --help               Show this help.

Environment variables:
  DIST                     Distribution to build for (default: trixie)
  ALLOW_LOW_SPACE          auto|0|1 (default: auto)
  HOST_ARCH                Same as --host
  BUILD_ARCH               Same as --build
  SBUILD_BUILD_DIR         Optional custom sbuild build directory
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --allow-low-space)
            ALLOW_LOW_SPACE=1
            shift
            ;;
        --host)
            HOST_ARCH="$2"
            shift 2
            ;;
        --build)
            BUILD_ARCH="$2"
            shift 2
            ;;
        --force)
            shift
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            print_usage
            exit 2
            ;;
    esac
done

# Enable cross-compile support if configured
_CC_ENV="$(dirname "$0")/../../../scripts/cross-compile-env.sh"
# shellcheck source=/dev/null
if [ -f "$_CC_ENV" ]; then source "$_CC_ENV"; else echo "Not using cross-compilation (${_CC_ENV} does not exist)"; fi

# Check if DIST is set by environment variable
if [ -n "$DIST" ]; then
    echo "Using distribution from DIST environment variable: $DIST"
    DIST_ARG="--dist=$DIST"
else
    echo "No DIST environment variable set, using sbuild default"
    DIST_ARG="--dist=trixie"
fi

SBUILD_EXTRA_ARGS=()

if [ -n "$HOST_ARCH" ]; then
    echo "Cross-build requested: build=${BUILD_ARCH}, host=${HOST_ARCH}"
    SBUILD_EXTRA_ARGS+=("--build=${BUILD_ARCH}" "--host=${HOST_ARCH}")
fi

if [ -n "${SBUILD_BUILD_DIR:-}" ]; then
    mkdir -p "$SBUILD_BUILD_DIR"
    SBUILD_EXTRA_ARGS+=("--build-dir=$SBUILD_BUILD_DIR")
fi

# Remove large local build artifacts so the source package stays small enough
# for sbuild's CHECK_SPACE validation.
if [ -d target ]; then
    echo "Removing previous build directory: target/"
    rm -rf target
fi

for path in build_tmp build_home; do
    if [ -d "$path" ]; then
        echo "Removing previous build directory: ${path}/"
        rm -rf "$path"
    fi
done

# Remove common local-only/generated files that cause lintian source errors
# when sbuild repacks the current working tree.
rm -rf debian/.debhelper
rm -f debian/*.debhelper.log debian/*.postrm.debhelper debian/*.substvars

# Local virtualenvs can contain absolute symlinks (for example .venv/bin/python3)
# that are invalid in source packages.
if [ -d .venv ]; then
    echo "Removing local virtual environment from source tree: .venv/"
    rm -rf .venv
fi

# Normalize tree via debhelper cleanup when available.
if [ -x debian/rules ]; then
    debian/rules clean >/dev/null 2>&1 || true
fi

source_kib=$(du -sk . | awk '{print $1}')
free_kib=$(df -Pk . | awk 'NR==2 {print $4}')
required_kib=$((source_kib * 2))

sbuild_config_tmp=""
cleanup_tmp_config() {
    if [ -n "$sbuild_config_tmp" ] && [ -f "$sbuild_config_tmp" ]; then
        rm -f "$sbuild_config_tmp"
    fi
}
trap cleanup_tmp_config EXIT

if [[ "$ALLOW_LOW_SPACE" == "1" || ( "$ALLOW_LOW_SPACE" == "auto" && "$free_kib" -lt "$required_kib" ) ]]; then
    echo "Enabling temporary sbuild low-space mode (CHECK_SPACE=0)."
    echo "Disk free: ${free_kib} KiB, source size: ${source_kib} KiB, sbuild threshold: ${required_kib} KiB"
    sbuild_config_tmp=$(mktemp)
    cat > "$sbuild_config_tmp" <<'EOF'
$check_space = 0;
EOF
    export SBUILD_CONFIG="$sbuild_config_tmp"
fi

sbuild --chroot-mode=unshare \
       --enable-network \
       --no-clean-source \
       --verbose \
       "${SBUILD_EXTRA_ARGS[@]}" \
       "$DIST_ARG"
