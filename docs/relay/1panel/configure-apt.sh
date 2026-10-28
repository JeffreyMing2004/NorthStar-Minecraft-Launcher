#!/bin/sh
set -eu
mirror=${1%/}
if [ "$mirror" = official ]; then
    exit 0
fi
case "$mirror" in
    https://*) ;;
    *) echo "DEBIAN_MIRROR must be an HTTPS URL or official" >&2; exit 2 ;;
esac
apt_root=${NSCL_APT_ROOT:-/etc/apt}
for source_file in "$apt_root/sources.list" "$apt_root"/sources.list.d/*.list "$apt_root"/sources.list.d/*.sources; do
    [ -f "$source_file" ] || continue
    sed -i \
        -e "s|https\?://deb\.debian\.org|$mirror|g" \
        -e "s|https\?://security\.debian\.org|$mirror|g" \
        "$source_file"
done
