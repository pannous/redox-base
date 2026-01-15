#!/bin/bash
# Build simple-ls with Cranelift for aarch64
set -e

cd "$(dirname "$0")"

NIGHTLY="nightly-2026-01-02"
TARGET="aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"
RELIBC="/opt/other/redox/recipes/core/relibc/source/target/aarch64-unknown-redox-clif/release"
BASE_SOURCE="$(dirname "$0")"

export DYLD_LIBRARY_PATH=~/.rustup/toolchains/${NIGHTLY}-aarch64-apple-darwin/lib

export RUSTFLAGS="-Zcodegen-backend=${CRANELIFT} \
  -Crelocation-model=static \
  -Clink-arg=-L${RELIBC} \
  -Clink-arg=${BASE_SOURCE}/crt0.o \
  -Clink-arg=${BASE_SOURCE}/crt0_rust.o \
  -Clink-arg=${BASE_SOURCE}/crti.o \
  -Clink-arg=${BASE_SOURCE}/crtn.o \
  -Clink-arg=-lunwind_stubs \
  -Clink-arg=-z -Clink-arg=muldefs \
  -Cpanic=abort"

echo "=== Building simple-ls with Cranelift ==="
cargo +${NIGHTLY} build \
    --target ${TARGET} \
    --release \
    -Z build-std=core,alloc,std,panic_abort \
    -Zbuild-std-features=compiler_builtins/no-f16-f128 \
    -p simple-ls

echo "=== Stripping ==="
llvm-strip -o /tmp/9p-share/ls target/aarch64-unknown-redox-clif/release/simple-ls

echo "=== Done ==="
ls -la /tmp/9p-share/ls
file /tmp/9p-share/ls
