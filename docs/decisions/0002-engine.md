# 0002. The engine: a pinned git dependency, used unmodified

Status: accepted (2026-10-25)

`hfcast` comes from the owner's mirror (code.alvaroedo.io/alvaroemtnez/hfcast-engine),
pinned to commit f9782529 (tag v2.14.0), with `embedded-coefficients`. A git
dependency is required: the coefficient maps are excluded from the crates.io
package (licensing caution by the author; see the engine's NOTICE and
docs/licence.md), so only a checkout has them. The Flatpak vendor step copies
them back into the vendored crate for the same reason.

It is called only through `hfcast::service::run` (JSON), the interface its author
keeps stable for HFcast. It is not patched; anything we need is built around it
in `core/`. The engine is thoroughly tested upstream (unit tests and a parity
check against the Fortran), so we test only our own code.

To move the pin: update the mirror, change `rev` in the root `Cargo.toml`, run
`tools/rust.sh cargo test --workspace` and compare a few screenshots.
