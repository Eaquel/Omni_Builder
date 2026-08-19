//! # Omni_Guard plugin
//!
//! Evaluates build, artifact and signing integrity and produces evidence.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.guard`
//! * **Purpose** — Own the defensive integrity platform: collect evidence,
//!   score integrity, and let policy decide, explainably.
//! * **Inputs** — apk.signed, build.provenance, artifact.digest
//! * **Outputs** — integrity.report, provenance.record
//! * **Capabilities** — FsRead, Crypto, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 26)
//! * **Roadmap** — PHASE 12 — OMNI_GUARD
//!
//! ## Target pipeline (directive section 26)
//!
//! ```text
//! Build Integrity
//!   ↓
//! Artifact Integrity
//!   ↓
//! APK Integrity
//!   ↓
//! Signing Identity
//!   ↓
//! Runtime Integrity
//!   ↓
//! Security Policy
//! ```
//!
//! ## Implementation status
//!
//! None of the pipeline above is implemented. This file declares the contract
//! so that the registry, the capability policy and the user interface can
//! reason about the plugin honestly; [`Plugin::execute`] refuses to run and
//! says why.
//!
//! Directive section 29 draws the boundary: this subsystem does detection,
//! verification, integrity, provenance and policy enforcement, and nothing
//! else. Anything on the forbidden list is out of scope permanently, not
//! merely unbuilt.
//!
//! Directive section 28 forbids a single boolean verdict. The model is
//! IntegrityState, IntegrityScore, EvidenceSet, Confidence, Policy and
//! Response, with states VALID, WARNING, RESTRICTED and INTEGRITY_FAILURE.
//!
//! Every decision must be explainable: a verdict without evidence is not a
//! verdict.
//!
//! The threat model of directive section 27 (T1-T12) is the acceptance
//! criterion for this plugin, not an afterthought.
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

/// The declared contract for the Omni_Guard plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.guard",
    display_name: "Omni_Guard",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Evaluates build, artifact and signing integrity and produces evidence.",
    inputs: &[
        "apk.signed",
        "build.provenance",
        "artifact.digest",
    ],
    outputs: &[
        "integrity.report",
        "provenance.record",
    ],
    required_capabilities: &[
        Capability::FsRead,
        Capability::Crypto,
        Capability::Cache,
    ],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::ProcessExec,
        Capability::KeyAccess,
    ],
    non_responsibilities: &[
        "Bypassing Play Protect or any other security control.",
        "Evading antivirus or hiding malware.",
        "Stealing credentials, escalating privilege, persisting covertly or performing surveillance.",
        "Reducing integrity to a single boolean.",
    ],
    roadmap_phase: "PHASE 12 — OMNI_GUARD",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct GuardPlugin;

/// The single instance the registry holds.
pub static PLUGIN: GuardPlugin = GuardPlugin;

impl Plugin for GuardPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.guard");
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
        assert_eq!(error.origin, "omni.plugin.guard");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
