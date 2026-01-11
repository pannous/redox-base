#!/bin/bash
# Build bootstrap with Cranelift for aarch64
set -e

cd "$(dirname "$0")"

NIGHTLY="nightly-2026-01-02"
TARGET="aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"

echo "=== Building bootstrap with Cranelift ==="

CARGO_INCREMENTAL=1 \
DYLD_LIBRARY_PATH=~/.rustup/toolchains/${NIGHTLY}-aarch64-apple-darwin/lib \
RUSTFLAGS="-Zcodegen-backend=${CRANELIFT}" \
cargo +${NIGHTLY} build \
  --target ${TARGET} \
  --release \
  -Z build-std=core,alloc \
  -Zbuild-std-features=compiler-builtins-mem,compiler_builtins/no-f16-f128

echo "=== Linking bootstrap ELF ==="

ld.lld -o /tmp/bootstrap-cranelift \
  --gc-sections \
  -T src/aarch64.ld \
  -z max-page-size=4096 \
  target/aarch64-unknown-redox-clif/release/libbootstrap.a \
  target/aarch64-unknown-redox-clif/release/deps/*.rlib

echo "=== Stripping ==="

llvm-strip -o /tmp/bootstrap-cranelift-stripped /tmp/bootstrap-cranelift

echo "=== Done ==="
ls -la /tmp/bootstrap-cranelift-stripped
file /tmp/bootstrap-cranelift-stripped
