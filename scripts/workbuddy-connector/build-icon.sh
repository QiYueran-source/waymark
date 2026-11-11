#!/usr/bin/env bash
# Render connector/icon.svg to the PNG sizes WorkBuddy asks for.
#
# The Connector package wants `icon.svg` or `icon.png`; the review checklist says
# PNG/JPG should be 64×64. We ship the SVG as the package icon and render both a
# 64×64 and a 400×400 PNG next to it so one file works in the package and the
# other in directory listings that expect a square raster.
#
# Rendering needs a browser or an SVG rasteriser. This environment has neither a
# headless Chrome nor rsvg-convert, so `--check` (the default) only reports what
# is missing and exits non-zero; install one of the tools and re-run without
# --check to produce the files.
#
# Usage:
#   scripts/workbuddy-connector/build-icon.sh            # check the toolchain
#   scripts/workbuddy-connector/build-icon.sh --check    # same
#   scripts/workbuddy-connector/build-icon.sh --render   # write the PNGs

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
svg="$root/connector/icon.svg"
render=0

for arg in "$@"; do
    case "$arg" in
        --render) render=1 ;;
        --check) render=0 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

rasteriser=""
if command -v rsvg-convert >/dev/null 2>&1; then
    rasteriser="rsvg-convert"
elif command -v inkscape >/dev/null 2>&1; then
    rasteriser="inkscape"
elif command -v magick >/dev/null 2>&1 && magick -list format 2>/dev/null | grep -q 'SVG'; then
    rasteriser="magick"
fi

if [[ -z "$rasteriser" ]]; then
    echo "no SVG rasteriser found (rsvg-convert, inkscape, or ImageMagick with SVG)" >&2
    echo "the package ships connector/icon.svg; the PNGs are optional" >&2
    exit 1
fi

if [[ $render -eq 0 ]]; then
    echo "rasteriser: $rasteriser"
    echo "run with --render to write connector/icon-64.png and connector/icon-400.png"
    exit 0
fi

render_one() {
    local size="$1" out="$root/connector/icon-$1.png"
    case "$rasteriser" in
        rsvg-convert) rsvg-convert -w "$size" -h "$size" "$svg" -o "$out" ;;
        inkscape) inkscape "$svg" -w "$size" -h "$size" -o "$out" >/dev/null 2>&1 ;;
        magick) magick -background none -density 1200 "$svg" -resize "${size}x${size}" "$out" ;;
    esac
    echo "wrote $out"
}

render_one 64
render_one 400
