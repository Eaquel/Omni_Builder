//! # Kotlin plugin
//!
//! Translates Kotlin source into the Omni compiler intermediate
//! representation.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.kotlin`
//! * **Purpose** — Own the Kotlin front end: everything from source text to a
//!   typed, lowered IR that a backend can consume.
//! * **Inputs** — kotlin.source, project.model, classpath.index
//! * **Outputs** — compiler.ir, jvm.class, diagnostics
//! * **Capabilities** — FsRead, TempStorage, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 16)
//! * **Roadmap** — PHASE 8 — KOTLIN
//!
//! ## Target pipeline (directive section 16)
//!
//! ```text
//! Source
//!   ↓
//! Lexer
//!   ↓
//! Parser
//!   ↓
//! AST
//!   ↓
//! Symbol Resolution
//!   ↓
//! Type Analysis
//!   ↓
//! HIR
//!   ↓
//! MIR
//!   ↓
//! Optimization
//!   ↓
//! Backend
//!   ↓
//! JVM / DEX
//! ```
//!
//! ## Implementation status
//!
//! `PLANNED`. Not one stage of the pipeline above is implemented:
//! **no lexer, no parser, no symbol table, no type checker, no HIR, no MIR and
//! no backend exists in this tree.** [`Plugin::execute`] refuses to run and
//! says why. This file declares the contract so that the registry, the
//! capability policy and the user interface can reason about the plugin
//! honestly.
//!
//! One thing this plugin's contract names does exist, and it belongs to the
//! Core rather than to this plugin: `jvm.class`, the last of the declared
//! outputs, is a published format, so `omni_core::jvm` reads one. It covers the
//! constant pool, access flags, superclass, interfaces, fields, methods,
//! attributes and the `kotlin.Metadata` annotation, and it is checked against
//! `javap` on the classes this repository's own build produces.
//!
//! That is the *shape of the artifact* a front end will have to emit, defined
//! and verified in advance. It is not a front end and it is not progress
//! towards one: a reader for a format says nothing about the ability to produce
//! its content.
//!
//! The bootstrap build uses the upstream Kotlin compiler through Gradle. That
//! compiler is not part of Omni_Builder and directive section 16 forbids
//! treating its output as evidence that this plugin works. Reading that output
//! does not change what it is evidence of.
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

/// The declared contract for the Kotlin plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.kotlin",
    display_name: "Kotlin",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Translates Kotlin source into the Omni compiler intermediate representation.",
    inputs: &["kotlin.source", "project.model", "classpath.index"],
    outputs: &["compiler.ir", "jvm.class", "diagnostics"],
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
        "Converting to DEX; that is the Dex plugin's work.",
        "Packaging or signing anything.",
        "Resolving external dependencies from a network.",
    ],
    roadmap_phase: "PHASE 8 — KOTLIN",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct KotlinPlugin;

/// The single instance the registry holds.
pub static PLUGIN: KotlinPlugin = KotlinPlugin;

impl Plugin for KotlinPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.kotlin");
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
        assert_eq!(error.origin, "omni.plugin.kotlin");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
