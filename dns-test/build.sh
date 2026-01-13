#!/bin/bash
CRANELIFT="/opt/other/redox/rust/compiler/rustc_codegen_cranelift/target/release/librustc_codegen_cranelift.dylib"
TARGET="aarch64-unknown-redox-clif.json"
SYSROOT="/opt/other/redox/build/aarch64/sysroot/lib"

cp /opt/other/redox/tools/$TARGET .

CARGO_INCREMENTAL=0 \
RUSTC_WRAPPER="" \
RUSTFLAGS="-Zcodegen-backend=${CRANELIFT} -Crelocation-model=static -Clto=no -Clink-arg=-L${SYSROOT} -Clink-arg=-z -Clink-arg=muldefs -Cpanic=abort" \
cargo +nightly build --target $TARGET --release \
  -Zbuild-std=core,alloc,std,panic_abort

cp target/aarch64-unknown-redox-clif/release/dns-test /opt/other/redox/share/
ls -lh /opt/other/redox/share/dns-test
