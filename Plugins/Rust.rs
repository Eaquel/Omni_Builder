//! # Rust plugin
//!
//! Compiles Rust crates for Android targets.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.rust`
//! * **Purpose** — Own the Rust front end, including the ownership and borrow
//!   analysis that makes Rust what it is.
//! * **Inputs** — rust.source, cargo.manifest, native.build.config
//! * **Outputs** — native.object, native.library
//! * **Capabilities** — FsRead, TempStorage, Cache, Native; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 19)
//! * **Roadmap** — PHASE 11 — RUST
//!
//! ## Target pipeline (directive section 19)
//!
//! ```text
//! Rust
//!   ↓
//! Lexer
//!   ↓
//! Parser
//!   ↓
//! AST
//!   ↓
//! HIR
//!   ↓
//! Type System
//!   ↓
//! Borrow Checking
//!   ↓
//! MIR
//!   ↓
//! Optimization
//!   ↓
//! Code Generation
//!   ↓
//! Object
//!   ↓
//! Link
//! ```
//!
//! ## Implementation status
//!
//! None of the pipeline above is implemented. This file declares the contract
//! so that the registry, the capability policy and the user interface can
//! reason about the plugin honestly; [`Plugin::execute`] refuses to run and
//! says why.
//!
//! Directive section 19 is explicit: without a production-quality type system
//! and borrow checker, no self-hosted Rust compiler may be claimed.
//!
//! The Core itself is compiled by the pinned upstream rustc. That is a
//! bootstrap dependency (directive section 15).
//!
//! ## Acceptance criteria before the status may change
//!
//! Directive section 51 applies in full: specification, stable contract,
//! real implementation, unit and integration and regression tests, fuzzing
//! where input is untrusted, security review, determinism verification,
//! measured performance, diagnostics, documentation and compatibility.

use crate::caps::Capability;
use crate::diag::Diagnostic;
use crate::plugin::{unimplemented_diagnostic, Context, Contract, Outcome, Plugin, Version};
use crate::Status;

/// The declared contract for the Rust plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.rust",
    display_name: "Rust",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Compiles Rust crates for Android targets.",
    inputs: &[
        "rust.source",
        "cargo.manifest",
        "native.build.config",
    ],
    outputs: &[
        "native.object",
        "native.library",
    ],
    required_capabilities: &[
        Capability::FsRead,
        Capability::TempStorage,
        Capability::Cache,
        Capability::Native,
    ],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::KeyAccess,
        Capability::SensitiveOutput,
    ],
    non_responsibilities: &[
        "Resolving crates from a registry; that requires a reviewed dependency resolution design (directive section 62).",
        "Packaging libraries into an APK.",
        "Replacing rustc for the Core's own build until self-hosting is real.",
    ],
    roadmap_phase: "PHASE 11 — RUST",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct RustPlugin;

/// The single instance the registry holds.
pub static PLUGIN: RustPlugin = RustPlugin;

impl Plugin for RustPlugin {
    fn contract(&self) -> &'static Contract {
        &CONTRACT
    }

    /// Refuses to run.
    ///
    /// Directive section 1 forbids a fabricated success. While the contract
    /// status is [`Status::Planned`], the only correct behaviour is to return
    /// the diagnostic that explains what is missing and when it is scheduled.
    fn execute(&self, _ctx: &mut Context<'_>) -> Result<Outcome, Diagnostic> {
        Err(unimplemented_diagnostic(&CONTRACT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::Policy;
    use crate::diag::Sink;

    #[test]
    fn the_contract_is_self_consistent() {
        assert_eq!(CONTRACT.id, "omni.plugin.rust");
        assert_eq!(CONTRACT.status, Status::Planned);
        assert!(!CONTRACT.status.may_produce_artifacts());
        assert!(!CONTRACT.inputs.is_empty());
        assert!(!CONTRACT.outputs.is_empty());
        assert!(!CONTRACT.non_responsibilities.is_empty());
        for required in CONTRACT.required_capabilities {
            assert!(!CONTRACT.forbidden_capabilities.contains(required));
        }
    }

    #[test]
    fn executing_reports_the_truth_instead_of_a_result() {
        let mut policy = Policy::new("test");
        let mut sink = Sink::new();
        let mut ctx = Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let error = PLUGIN
            .execute(&mut ctx)
            .expect_err("a PLANNED plugin must never report success");

        assert_eq!(error.code, "E0001");
        assert_eq!(error.origin, "omni.plugin.rust");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
