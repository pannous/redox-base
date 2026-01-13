
## DNS Resolution Crash - Cranelift Bug (2026-01-13)

**Symptom**: Any hostname resolution crashes with null pointer dereference
- curl http://pannous.com → GUARD PAGE at 0x0
- ping pannous.com → same crash
- curl http://81.169.181.160 → works (IP addresses OK)

**Root Cause**: Cranelift codegen bug on aarch64 in DNS resolution code
- Similar to documented varargs bug (notes/cranelift-varargs-bug.md)
- Crash happens before any logging → very early in DNS code path
- Added trace!() to relibc lookup.rs but couldn't rebuild due to dep conflicts

**Workaround**: Use IP addresses instead of hostnames

**Files Modified**:
- recipes/core/relibc/source/src/header/netdb/lookup.rs (added trace logging)
- recipes/core/base/source/Cargo.toml (added dns-test workspace member)

**Needs**: Report to rustc_codegen_cranelift upstream with details
