//! # Resources plugin
//!
//! Parses, validates and compiles Android resources into a resource table.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.resources`
//! * **Purpose** — Own the resource pipeline: from resource source files to
//!   compiled resources with stable identifiers and resolved references.
//! * **Inputs** — resource.source, android.manifest
//! * **Outputs** — resource.table, resource.compiled, resource.ids
//! * **Capabilities** — FsRead, TempStorage, Cache; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 22)
//! * **Roadmap** — PHASE 4 — RESOURCE ENGINE
//!
//! ## Target pipeline (directive section 22)
//!
//! ```text
//! Source
//!   ↓
//! Validation
//!   ↓
//! Parse
//!   ↓
//! Resource Model
//!   ↓
//! ID Assignment
//!   ↓
//! Reference Resolution
//!   ↓
//! Table Construction
//!   ↓
//! Compiled Resources
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
//! Planned coverage: values, strings, colors, dimensions, drawables, mipmap,
//! references, manifest resources, resource identifiers and resource tables.
//!
//! Resource identifier assignment must be deterministic, since it feeds the
//! APK's reproducibility (directive section 12).
//!
//! Resource input is untrusted: it comes from the user's project and must be
//! bounded against path explosion and deep nesting (directive section 60).
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

/// The declared contract for the Resources plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.resources",
    display_name: "Resources",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Parses, validates and compiles Android resources into a resource table.",
    inputs: &["resource.source", "android.manifest"],
    outputs: &["resource.table", "resource.compiled", "resource.ids"],
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
        "Packaging resources into an APK.",
        "Rendering or rasterising drawables.",
        "Downloading remote resources.",
    ],
    roadmap_phase: "PHASE 4 — RESOURCE ENGINE",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct ResourcesPlugin;

/// The single instance the registry holds.
pub static PLUGIN: ResourcesPlugin = ResourcesPlugin;

impl Plugin for ResourcesPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.resources");
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
        assert_eq!(error.origin, "omni.plugin.resources");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
