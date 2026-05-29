# Piece Tree

Purely functional (immutable) implementation of Piece Tree, inspired by [fredbuf](https://github.com/cdacamar/fredbuf).

## Vendoring

To keep `piece-tree` highly optimized and tailored for its specific use case, portions of the following third-party crates have been integrated directly into the source code:

* **`cranelift-entity`** (Apache-2.0 with LLVM Exception).
* **`smallvec`** (MIT).
* **`bytecount`** (MIT).

This copy-pasted code remains under its original respective licenses. Full attribution notices are maintained at the top of the relevant source files, and the complete license texts can be found in [THIRD-PARTY-LICENSES.md](./THIRD-PARTY-LICENSES.md).

If you prefer to use the upstream, non-vendored versions of these crates via Cargo, you can enable the `dont_vendor` feature flag.
