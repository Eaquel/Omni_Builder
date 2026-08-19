//! # Java plugin
//!
//! Translates Java source into the Omni compiler intermediate representation.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.java`
//! * **Purpose** — Own the Java front end, including the semantic analysis the
//!   Java Language Specification requires.
//! * **Inputs** — java.source, project.model, classpath.index
//! * **Outputs** — compiler.ir, jvm.class, diagnostics
//! * **Capabilities** — FsRead, TempStorage, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 17)
//! * **Roadmap** — PHASE 9 — JAVA
//!
//! ## Target pipeline (directive section 17)
//!
//! ```text
//! Java Source
//!   ↓
//! Lexer
//!   ↓
//! Parser
//!   ↓
//! AST
//!   ↓
//! Symbol Table
//!   ↓
//! Type System
//!   ↓
//! Semantic Analysis
//!   ↓
//! IR
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
//! None of the pipeline above is implemented. This file declares the contract
//! so that the registry, the capability policy and the user interface can
//! reason about the plugin honestly; [`Plugin::execute`] refuses to run and
//! says why.
//!
//! Nothing of the Java front end exists in this tree yet.
//!
//! The pinned JDK is a bootstrap dependency (directive section 15) and must
//! be labelled as such wherever it is used.
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

/// The declared contract for the Java plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.java",
    display_name: "Java",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Translates Java source into the Omni compiler intermediate representation.",
    inputs: &["java.source", "project.model", "classpath.index"],
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
        "Providing a Java runtime or standard library.",
        "Annotation processing, until that is specified separately.",
    ],
    roadmap_phase: "PHASE 9 — JAVA",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct JavaPlugin;

/// The single instance the registry holds.
pub static PLUGIN: JavaPlugin = JavaPlugin;

impl Plugin for JavaPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.java");
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
        assert_eq!(error.origin, "omni.plugin.java");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
