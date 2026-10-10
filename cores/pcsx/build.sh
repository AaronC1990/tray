#!/bin/sh
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT WORKDIR OUT" >&2
	exit 2
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/pcsx_rearmed/tree/$1"
	for p in "$here"/*.patch; do
		[ -e "$p" ] || continue
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" work="$2" out="$3"
	src="$work/pcsx_rearmed"

	mkdir -p "$src"
	[ -d "$src/.git" ] || git init -q "$src"
	git -C "$src" cat-file -e "$commit^{commit}" 2>/dev/null ||
		git -C "$src" fetch -q --depth 1 https://github.com/libretro/pcsx_rearmed "$commit"
	git -C "$src" checkout -q --force --detach "$commit"
	git -C "$src" submodule update -q --init --depth 1 --recursive
	git -C "$src" clean -q -fdx
	for p in "$here"/*.patch; do
		[ -e "$p" ] || continue
		git -C "$src" apply "$p"
	done

	# The H700 is four Cortex-A53s, the same cores as Allwinner's H5, whose
	# profile builds the arm64 dynarec and NEON GPU tuned for them. To
	# cross-build it from another machine:
	#   PLATFORM=h5 CC=tools/cross/aarch64-cc CXX=tools/cross/aarch64-c++ \
	#     AR=tools/cross/ar sh cores/pcsx/build.sh build COMMIT WORKDIR OUT
	platform="${PLATFORM:-unix}"
	if [ "$(uname -s)-$(uname -m)" = "Linux-aarch64" ]; then
		platform=h5
	fi
	make -C "$src" -f Makefile.libretro platform="$platform" \
		-j "$(getconf _NPROCESSORS_ONLN)" >/dev/null
	cp "$src/pcsx_rearmed_libretro.so" "$out"
	stamp "$commit" >"$out.meta"
}

case "${1:-}" in
stamp) [ $# -eq 2 ] || usage; stamp "$2" ;;
build) [ $# -eq 4 ] || usage; build "$2" "$3" "$4" ;;
*) usage ;;
esac
