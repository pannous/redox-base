#!/bin/bash
# Build curl with DYNAMIC linking for Redox
set -e
cd "$(dirname "$0")"

NIGHTLY="nightly-2026-01-02"
TARGET="/opt/other/redox/recipes/core/base/source/aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"
SYSROOT="/opt/other/redox/mount/usr/lib"

export DYLD_LIBRARY_PATH=~/.rustup/toolchains/${NIGHTLY}-aarch64-apple-darwin/lib
export CARGO_INCREMENTAL=0
export RUSTC_WRAPPER=""

# Dynamic linking flags - use prefer-dynamic and link against sysroot
export RUSTFLAGS="-Zcodegen-backend=${CRANELIFT} \
  -Cprefer-dynamic \
  -Clink-arg=-L${SYSROOT} \
  -Clink-arg=-dynamic-linker -Clink-arg=/usr/lib/ld.so.1 \
  -Clink-arg=-z -Clink-arg=muldefs \
  -Cpanic=abort"

echo "=== Building curl with Cranelift (DYNAMIC) ==="
cargo +${NIGHTLY} build \
    --target ${TARGET} \
    --release \
    -Z build-std=core,alloc,std,panic_abort \
    -Zbuild-std-features=compiler_builtins/no-f16-f128

echo "=== Stripping ==="
llvm-strip -o /opt/other/redox/share/curl.dynamic ../target/aarch64-unknown-redox-clif/release/curl
ls -la /opt/other/redox/share/curl.dynamic

echo "=== Checking if dynamically linked ==="
file /opt/other/redox/share/curl.dynamic
llvm-readelf -d /opt/other/redox/share/curl.dynamic 2>/dev/null | grep -i needed || echo "(no NEEDED entries - static)"

echo "Done - dynamic curl is at /opt/other/redox/share/curl.dynamic"
