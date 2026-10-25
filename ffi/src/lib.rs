//! The core, as one shared library per Android ABI. Nothing lives here:
//! every type and function is the core's own, exported through UniFFI.

hfpredict_core::uniffi_reexport_scaffolding!();
