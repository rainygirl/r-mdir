#!/bin/sh
# Haiku 교차 빌드. haiku/cross-compiler 컨테이너 안에서 실행된다.
#   ARCH=x86_64 (기본, 이미지 x86_64-r1beta4)   -> x86_64-unknown-haiku
#   ARCH=x86    (이미지 x86_gcc2h-r1beta4)      -> i686-unknown-haiku (32비트, x86_gcc2 하이브리드용)
set -eu
ARCH="${ARCH:-x86_64}"
case "$ARCH" in
  x86_64) TARGET=x86_64-unknown-haiku; TOOLS=/tools/cross-tools-x86_64/bin; GCC=x86_64-unknown-haiku ;;
  x86)    TARGET=i686-unknown-haiku;   TOOLS=/tools/cross-tools-x86/bin;    GCC=i586-pc-haiku ;;
  *) echo "unknown ARCH=$ARCH" >&2; exit 1 ;;
esac
TARGET_ENV=$(echo "$TARGET" | tr 'a-z-' 'A-Z_')

export RUSTUP_HOME=/rust/rustup CARGO_HOME=/rust/cargo
export PATH="$CARGO_HOME/bin:$TOOLS:$PATH"

if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain nightly -c rust-src
fi

export "CARGO_TARGET_${TARGET_ENV}_LINKER=$GCC-gcc"
export "CC_$(echo "$TARGET" | tr '-' '_')=$GCC-gcc"
export "CXX_$(echo "$TARGET" | tr '-' '_')=$GCC-g++"
export "AR_$(echo "$TARGET" | tr '-' '_')=$GCC-ar"
export CARGO_TARGET_DIR=/work/target/haiku

cd /work
cargo +nightly build --release --target "$TARGET" -Zbuild-std=std,panic_abort "$@"
