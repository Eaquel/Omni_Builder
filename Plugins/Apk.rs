//! # APK plugin
//!
//! Lays out and packages an APK deterministically.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.apk`
//! * **Purpose** — Own the archive: entry layout, ordering, alignment and
//!   structural validation of the package before it is handed to signing.
//! * **Inputs** — dex.classes, resource.compiled, native.library, asset.file, android.manifest
//! * **Outputs** — apk.unsigned
//! * **Capabilities** — FsRead, FsWrite, TempStorage, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 23)
//! * **Roadmap** — PHASE 5 — APK ENGINE
//!
//! ## Target pipeline (directive section 23)
//!
//! ```text
//! Project
//!   ↓
//! Artifact Graph
//!   ↓
//! Manifest
//!   ↓
//! Resources
//!   ↓
//! DEX
//!   ↓
//! Native Libraries
//!   ↓
//! Assets
//!   ↓
//! APK Layout
//!   ↓
//! Deterministic Ordering
//!   ↓
//! Alignment
//!   ↓
//! Signing
//!   ↓
//! Verification
//! ```
//!
//! ## Implementation status
//!
//! None of the pipeline above is implemented. This file declares the contract
//! so that the registry, the capability policy and the user interface can
//! reason about the plugin honestly; [`Plugin::execute`] refuses to run and
//! says why.
//!
//! Mandatory checks before an artifact may leave this plugin: duplicate
//! entries, invalid offsets, malformed metadata, path traversal, invalid
//! names, alignment, deterministic ordering and archive integrity.
//!
//! Timestamps, entry order and metadata must be normalised, or the build is
//! not reproducible (directive section 12).
//!
//! Archive input is untrusted and must be bounded against archive bombs
//! (directive section 60).
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

/// The declared contract for the APK plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.apk",
    display_name: "APK",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Lays out and packages an APK deterministically.",
    inputs: &[
        "dex.classes",
        "resource.compiled",
        "native.library",
        "asset.file",
        "android.manifest",
    ],
    outputs: &[
        "apk.unsigned",
    ],
    required_capabilities: &[
        Capability::FsRead,
        Capability::FsWrite,
        Capability::TempStorage,
        Capability::Cache,
    ],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::ProcessExec,
        Capability::KeyAccess,
    ],
    non_responsibilities: &[
        "Signing; that is the Sign plugin's work, behind a separate capability.",
        "Compiling anything.",
        "Inventing an archive format. Directive section 24 requires the published specification to be implemented, not replaced.",
    ],
    roadmap_phase: "PHASE 5 — APK ENGINE",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct ApkPlugin;

/// The single instance the registry holds.
pub static PLUGIN: ApkPlugin = ApkPlugin;

impl Plugin for ApkPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.apk");
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
        assert_eq!(error.origin, "omni.plugin.apk");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
