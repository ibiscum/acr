#!/bin/bash

cd "$(dirname "$0")" || exit

# Enable cross-compile support if configured
_CC_ENV="$(dirname "$0")/../../../scripts/cross-compile-env.sh"
# shellcheck source=/dev/null
if [ -f "$_CC_ENV" ]; then source "$_CC_ENV"; else echo "Not using cross-compilation (${_CC_ENV} does not exist)"; fi

# Determine target distribution
if [ -n "$DIST" ]; then
    echo "Using distribution from DIST environment variable: $DIST"
else
    DIST="trixie"
    echo "No DIST environment variable set, defaulting to trixie"
fi

# When backporting (e.g. DIST=bookworm-backports), sbuild must build inside the
# base suite's chroot (bookworm) while the upload target distribution is taken
# from debian/changelog. Determine the chroot distribution name first.
if [[ "$DIST" == *-backports ]]; then
    CHROOT_DIST="${DIST%-backports}"
    echo "Backport target: using ${CHROOT_DIST} chroot (upload target from changelog)"
else
    CHROOT_DIST="$DIST"
fi

# Always offer the chroot's backports repository as an extra repository so the
# chroot can install newer toolchain packages (rustc/cargo) when available in
# backports. This is harmless if no newer packages exist there.
# For testing/unstable distributions (like trixie), use sid instead since
# backports are for stable releases.
if [[ "$CHROOT_DIST" == "trixie" || "$CHROOT_DIST" == "testing" ]]; then
    EXTRA_REPO_ARGS=("--extra-repository=deb http://deb.debian.org/debian sid main")
else
    EXTRA_REPO_ARGS=("--extra-repository=deb http://deb.debian.org/debian ${CHROOT_DIST}-backports main")
fi

# Optionally allow installing newer Rust toolchain from the distribution's
# backports (instead of base release packages). This helps CI/backport builds
# that need a newer rustc/cargo. Set KEEP_TESTING_TOOLCHAIN=1 in the environment.
if [ "${KEEP_TESTING_TOOLCHAIN}" = "1" ]; then
    echo "KEEP_TESTING_TOOLCHAIN=1: will install rustc/cargo from ${CHROOT_DIST}-backports"
fi

# First, create the source package (.dsc and .tar.gz) via dpkg-buildpackage
echo "Building source package..."
cd "$(dirname "$0")" || exit 1
dpkg-buildpackage -S -us -uc || exit 1

# Locate the .dsc file created by dpkg-buildpackage (placed in parent directory)
DSC_FILE=$(cd .. && ls -t hifiberry-audiocontrol_*.dsc 2>/dev/null | head -1)
if [ -z "$DSC_FILE" ]; then
    echo "ERROR: No .dsc file found in parent directory"
    exit 1
fi

# --enable-network is required because cargo fetches crate dependencies from
# crates.io during the build. Remove once dependencies are vendored.
echo "Building binary package via sbuild..."
cd .. || exit 1
sbuild --chroot-mode=unshare \
       --enable-network \
       --dist="$CHROOT_DIST" \
       "${EXTRA_REPO_ARGS[@]}" \
       -e KEEP_TESTING_TOOLCHAIN="${KEEP_TESTING_TOOLCHAIN}" \
       -d "$CHROOT_DIST" \
       -b "$DSC_FILE"
