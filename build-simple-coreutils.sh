#!/bin/bash
# Build simple-coreutils with Cranelift for aarch64 Redox
set -e

cd "$(dirname "$0")"

NIGHTLY="nightly-2026-01-02"
TARGET="aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"
RELIBC="/opt/other/redox/recipes/core/relibc/source/target/aarch64-unknown-redox-clif/release"

export DYLD_LIBRARY_PATH=~/.rustup/toolchains/${NIGHTLY}-aarch64-apple-darwin/lib

export RUSTFLAGS="-Zcodegen-backend=${CRANELIFT} \
  -Crelocation-model=static \
  -Clink-arg=-L${RELIBC} \
  -Clink-arg=${RELIBC}/crt0.o \
  -Clink-arg=${RELIBC}/crt0_rust.o \
  -Clink-arg=${RELIBC}/crti.o \
  -Clink-arg=${RELIBC}/crtn.o \
  -Clink-arg=-lunwind_stubs \
  -Clink-arg=-z -Clink-arg=muldefs \
  -Cpanic=abort"

echo "=== Building simple-coreutils with Cranelift ==="
cargo +${NIGHTLY} build \
    --target ${TARGET} \
    --release \
    -Z build-std=core,alloc,std,panic_abort \
    -Zbuild-std-features=compiler_builtins/no-f16-f128 \
    -p simple-coreutils

echo "=== Stripping ==="
OUT=/tmp/simple-coreutils
mkdir -p $OUT
for bin in simple-cat simple-rm simple-mkdir simple-echo simple-cp simple-touch; do
    if [[ -f "target/aarch64-unknown-redox-clif/release/$bin" ]]; then
        llvm-strip -o "$OUT/$bin" "target/aarch64-unknown-redox-clif/release/$bin"
        echo "  $bin -> $OUT/$bin"
    fi
done

echo "=== Done ==="
ls -la $OUT/
