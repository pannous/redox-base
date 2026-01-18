#!/bin/bash
set -e
NIGHTLY="nightly-2026-01-02"
TARGET="/opt/other/redox/recipes/core/base/source/aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"
RELIBC="/opt/other/redox/build/aarch64/sysroot/lib"

export DYLD_LIBRARY_PATH=~/.rustup/toolchains/${NIGHTLY}-aarch64-apple-darwin/lib
export CARGO_INCREMENTAL=0
export RUSTC_WRAPPER=""
export RUSTFLAGS="-Zcodegen-backend=${CRANELIFT} -Crelocation-model=static -Clink-arg=-L${RELIBC} -Clink-arg=-z -Clink-arg=muldefs -Cpanic=abort"

cargo +${NIGHTLY} build \
    --target ${TARGET} \
    --release \
    -Z build-std=core,alloc,std,panic_abort \
    -Zbuild-std-features=compiler_builtins/no-f16-f128

llvm-strip -o /opt/other/redox/share/readdir-bench target/aarch64-unknown-redox-clif/release/readdir-bench
ls -la /opt/other/redox/share/readdir-bench
