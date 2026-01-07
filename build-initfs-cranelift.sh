#!/bin/bash
# Build initfs binaries with Cranelift for aarch64
set -e

cd "$(dirname "$0")"

NIGHTLY="nightly-2026-01-02"
TARGET="aarch64-unknown-redox-clif.json"
CRANELIFT="/opt/other/rustc_codegen_cranelift/dist/lib/librustc_codegen_cranelift.dylib"
RELIBC="/opt/other/redox/recipes/core/relibc/source/target/aarch64-unknown-redox-clif/release"
REDOXFS="/opt/other/redox/recipes/core/redoxfs/source/target/aarch64-unknown-redox-clif/release/redoxfs"

# Core initfs binaries
BINS="init logd ramfs randd zerod"
# Driver binaries
BINS="$BINS acpid fbbootlogd fbcond hwd inputd lived nvmed pcid pcid-spawner rtcd vesad"
# Virtio for QEMU
BINS="$BINS virtio-blkd virtio-gpud virtio-9pd"
# Test binaries
BINS="$BINS test-9p simple-ls"

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

echo "=== Building initfs binaries ==="
cargo +${NIGHTLY} build \
    --target ${TARGET} \
    --release \
    -Z build-std=core,alloc,std,panic_abort \
    -Zbuild-std-features=compiler_builtins/no-f16-f128 \
    $(for bin in $BINS; do echo "-p $bin"; done)

echo "=== Creating initfs directory ==="
rm -rf /tmp/initfs-cranelift
mkdir -p /tmp/initfs-cranelift/bin /tmp/initfs-cranelift/lib/drivers /tmp/initfs-cranelift/etc/pcid

# Strip and copy binaries
for bin in init logd ramfs randd zerod pcid pcid-spawner acpid fbbootlogd fbcond hwd inputd lived rtcd vesad test-9p simple-ls; do
    llvm-strip -o /tmp/initfs-cranelift/bin/$bin target/aarch64-unknown-redox-clif/release/$bin
done
# Create 'ls' symlink/copy for convenience
cp /tmp/initfs-cranelift/bin/simple-ls /tmp/initfs-cranelift/bin/ls
for bin in nvmed virtio-blkd virtio-gpud virtio-9pd; do
    llvm-strip -o /tmp/initfs-cranelift/lib/drivers/$bin target/aarch64-unknown-redox-clif/release/$bin
done

# nulld is a copy of zerod
cp /tmp/initfs-cranelift/bin/zerod /tmp/initfs-cranelift/bin/nulld

# Copy redoxfs (should be pre-built with Cranelift)
if [ -f "$REDOXFS" ]; then
    llvm-strip -o /tmp/initfs-cranelift/bin/redoxfs "$REDOXFS"
else
    echo "WARNING: redoxfs not found at $REDOXFS"
fi

# Copy config files
cp init.rc /tmp/initfs-cranelift/etc/init.rc
cp init_drivers.rc /tmp/initfs-cranelift/etc/init_drivers.rc
cp drivers/initfs.toml /tmp/initfs-cranelift/etc/pcid/initfs.toml

echo "=== Building bootstrap ==="
cd bootstrap
./build-cranelift.sh
cd ..

echo "=== Creating initfs archive ==="
# Clear RUSTFLAGS for host tool build (not cross-compiled)
unset RUSTFLAGS
cargo run --manifest-path initfs/tools/Cargo.toml --bin redox-initfs-ar -- \
    /tmp/initfs-cranelift /tmp/bootstrap-cranelift-stripped -o /tmp/initfs-cranelift.img

echo "=== Done ==="
ls -la /tmp/initfs-cranelift.img
echo "To test: inject into a Redox ISO and boot with QEMU"
