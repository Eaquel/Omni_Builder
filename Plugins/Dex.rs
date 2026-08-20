//! # DEX plugin
//!
//! Reads, models, writes and verifies Dalvik executable files.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.dex`
//! * **Purpose** — Own the DEX binary format as a real model, not a byte
//!   conversion: pools, class definitions, code items, offsets and checksums.
//! * **Inputs** — compiler.ir, jvm.class
//! * **Outputs** — dex.classes, dex.model
//! * **Capabilities** — FsRead, TempStorage, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 21)
//! * **Roadmap** — PHASE 7 — DEX
//!
//! ## Target pipeline (directive section 21)
//!
//! ```text
//! Compiler IR
//!   ↓
//! DEX IR
//!   ↓
//! Register Allocation
//!   ↓
//! Instruction Selection
//!   ↓
//! Code Layout
//!   ↓
//! Pool Construction
//!   ↓
//! Offset Resolution
//!   ↓
//! Validation
//!   ↓
//! Checksum
//!   ↓
//! Signature
//!   ↓
//! classes.dex
//! ```
//!
//! ## Implementation status
//!
//! `PLANNED`. This plugin produces nothing and [`Plugin::execute`] refuses to
//! run and says why. This file declares the contract so that the registry, the
//! capability policy and the user interface can reason about the plugin
//! honestly.
//!
//! A reader exists in the Core, and it belongs to the Core rather than to this
//! plugin. `omni_core::dex` reads a file's header, map list, string, type,
//! prototype, field and method pools and class definitions, decodes the
//! modified UTF-8 the string pool uses, and recomputes the Adler-32 checksum
//! and SHA-1 signature the file records over itself. Its header fields and its
//! class list are checked against `dexdump` on the package this repository's
//! own build produces.
//!
//! It reads no code item, so nothing in this tree can say what a method does,
//! and it writes nothing. Every stage of the pipeline above -- from DEX IR
//! through register allocation, instruction selection, layout, pool
//! construction and offset resolution to the checksum and signature a writer
//! would have to produce -- does not exist in any form. Reading a DEX is not
//! producing one, and nothing here moves this contract off `PLANNED`.
//!
//! Directive section 21 requires that malformed DEX input never crashes the
//! reader, so this plugin is a fuzzing target from its first line of code.
//!
//! The writer's output must be byte-identical for identical input, which
//! makes pool ordering and offset resolution deterministic by contract.
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

/// The declared contract for the DEX plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.dex",
    display_name: "DEX",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Reads, models, writes and verifies Dalvik executable files.",
    inputs: &["compiler.ir", "jvm.class"],
    outputs: &["dex.classes", "dex.model"],
    required_capabilities: &[
        Capability::FsRead,
        Capability::TempStorage,
        Capability::Cache,
    ],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::ProcessExec,
        Capability::KeyAccess,
        Capability::SensitiveOutput,
    ],
    non_responsibilities: &[
        "Compiling source; it consumes an IR that a front end produced.",
        "Packaging or signing the result.",
        "Optimising bytecode beyond what the DEX format requires.",
    ],
    roadmap_phase: "PHASE 7 — DEX",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct DexPlugin;

/// The single instance the registry holds.
pub static PLUGIN: DexPlugin = DexPlugin;

impl Plugin for DexPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.dex");
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
        assert_eq!(error.origin, "omni.plugin.dex");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
