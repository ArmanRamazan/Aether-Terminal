//! BPF program loader and lifecycle management.
//!
//! Loads pre-compiled BPF bytecode into the kernel via aya, attaches
//! tracepoints, and initializes ring buffer maps.

// TODO: implement BpfLoader struct with load/attach/detach methods
