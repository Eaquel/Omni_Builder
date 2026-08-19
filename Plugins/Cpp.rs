//! # C/C++ plugin
//!
//! Compiles C and C++ translation units into Android native libraries.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.cpp`
//! * **Purpose** — Own the native front end and the path from preprocessed
//!   source to a linked ELF shared object for each supported ABI.
//! * **Inputs** — cpp.source, cpp.header, native.build.config
//! * **Outputs** — native.object, native.library
//! * **Capabilities** — FsRead, TempStorage, Cache, Native; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 18)
//! * **Roadmap** — PHASE 10 — C/C++
//!
//! ## Target pipeline (directive section 18)
//!
//! ```text
//! Source
//!   ↓
//! Preprocessor
//!   ↓
//! Lexer
//!   ↓
//! Parser
//!   ↓
//! AST
//!   ↓
//! Semantic Analysis
//!   ↓
//! IR
//!   ↓
//! Optimization
//!   ↓
//! Code Generation
//!   ↓
//! Assembly
//!   ↓
//! Object
//!   ↓
//! Link
//!   ↓
//! ELF
//!   ↓
//! Android Native Library
//! ```
//!
//! ## Implementation status
//!
//! None of the pipeline above is implemented. This file declares the contract
//! so that the registry, the capability policy and the user interface can
//! reason about the plugin honestly; [`Plugin::execute`] refuses to run and
//! says why.
//!
//! Target ABIs, once implemented: armeabi-v7a, arm64-v8a, x86, x86_64.
//!
//! The bootstrap build uses Clang from the pinned NDK through CMake. That is
//! a bootstrap dependency, not an implementation of this plugin.
//!
//! PROCESS_EXEC is not requested. Driving an external compiler would need
//! that capability, an explicit policy and an audit trail (directive section
//! 61).
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

/// The declared contract for the C/C++ plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.cpp",
    display_name: "C/C++",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Compiles C and C++ translation units into Android native libraries.",
    inputs: &["cpp.source", "cpp.header", "native.build.config"],
    outputs: &["native.object", "native.library"],
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
        "Packaging libraries into an APK; that is the Apk plugin's work.",
        "Providing a C or C++ standard library implementation.",
        "Deciding which ABIs a project targets; that comes from the project model.",
    ],
    roadmap_phase: "PHASE 10 — C/C++",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct CppPlugin;

/// The single instance the registry holds.
pub static PLUGIN: CppPlugin = CppPlugin;

impl Plugin for CppPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.cpp");
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
        assert_eq!(error.origin, "omni.plugin.cpp");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
