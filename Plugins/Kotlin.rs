use crate::caps::Capability;
use crate::diag::Diagnostic;
use crate::plugin::{unimplemented_diagnostic, Context, Contract, Outcome, Plugin, Version};
use crate::Status;

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
    roadmap_phase: "PHASE 10 — SELF-HOSTING",
};

pub struct KotlinPlugin;

pub static PLUGIN: KotlinPlugin = KotlinPlugin;

impl Plugin for KotlinPlugin {
    fn contract(&self) -> &'static Contract {
        &CONTRACT
    }

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
