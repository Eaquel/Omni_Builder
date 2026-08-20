//! # Sign plugin
//!
//! Signs an APK and verifies the resulting signature.
//!
//! ## Contract (directive section 2)
//!
//! * **Module** — `omni.plugin.sign`
//! * **Purpose** — Own key handling, digesting, signature generation,
//!   certificate validation and final verification of the signed artifact.
//! * **Inputs** — apk.unsigned, signing.identity
//! * **Outputs** — apk.signed, signature.verification
//! * **Capabilities** — FsRead, FsWrite, Crypto, KeyAccess, SensitiveOutput, TempStorage; everything else denied
//! * **State** — stateless
//! * **Determinism** — required for every artifact this plugin emits
//! * **Status** — `PLANNED` (directive section 25)
//! * **Roadmap** — PHASE 6 — SIGNING
//!
//! ## Target pipeline (directive section 25)
//!
//! ```text
//! Key Management
//!   ↓
//! Certificate Parsing
//!   ↓
//! Digest
//!   ↓
//! Signature
//!   ↓
//! APK Signing
//!   ↓
//! Certificate Validation
//!   ↓
//! Final Verification
//! ```
//!
//! ## Implementation status
//!
//! `PLANNED`. This plugin signs nothing and [`Plugin::execute`] refuses to run
//! and says why. This file declares the contract so that the registry, the
//! capability policy and the user interface can reason about the plugin
//! honestly.
//!
//! Two stages of the pipeline above now exist in the Core, on the reading side
//! only, and they belong to the Core rather than to this plugin:
//!
//! * **Certificate Parsing** — `omni_core::x509` reads a certificate's names,
//!   validity, serial, key size and algorithms and fingerprints it. It never
//!   checks the certificate's own signature, so it identifies a certificate
//!   and never validates one.
//! * **Final Verification**, partly — `omni_core::signing` finds the APK
//!   signing block, reads its v2 signers and recomputes the chunked SHA-256
//!   content digest, matched against `apksigner`. It never checks the
//!   signature over the signed data, because there is no RSA or
//!   elliptic-curve arithmetic in this tree. A digest match proves a package
//!   is unchanged; it does not prove who signed it.
//!
//! Key Management, Digest-for-signing, Signature and APK Signing do not exist
//! in any form. Reading a signature is not producing one, and nothing here
//! moves this contract off `PLANNED`.
//!
//! This is the only plugin that may ever hold KEY_ACCESS, and it holds it
//! under an explicit grant with an audit record.
//!
//! Directive section 25 is absolute: a private key must never reach an
//! artifact, a log, a diagnostic or a crash dump. Every diagnostic this
//! plugin emits must be written with that in mind.
//!
//! Every primitive must be implemented against an official specification and
//! validated with official test vectors before its status may leave PLANNED.
//!
//! The bootstrap APK is signed today by the Android Gradle Plugin, configured in
//! Builder/build.gradle.kts from a keystore that lives outside the repository
//! (ADR-0007 in Omni.rs). That is a bootstrap dependency in the sense of
//! directive section 15, not an implementation of this plugin, and it does not
//! move this contract off PLANNED.
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

/// The declared contract for the Sign plugin.
pub static CONTRACT: Contract = Contract {
    id: "omni.plugin.sign",
    display_name: "Sign",
    version: Version::new(0, 1, 0),
    status: Status::Planned,
    summary: "Signs an APK and verifies the resulting signature.",
    inputs: &[
        "apk.unsigned",
        "signing.identity",
    ],
    outputs: &[
        "apk.signed",
        "signature.verification",
    ],
    required_capabilities: &[
        Capability::FsRead,
        Capability::FsWrite,
        Capability::Crypto,
        Capability::KeyAccess,
        Capability::SensitiveOutput,
        Capability::TempStorage,
    ],
    forbidden_capabilities: &[
        Capability::Network,
        Capability::Internet,
        Capability::ProcessExec,
    ],
    non_responsibilities: &[
        "Inventing cryptographic primitives. Directive section 30 permits only established standards with official test vectors.",
        "Packaging; it receives a finished archive.",
        "Storing private keys. Key custody is a separate, reviewed design.",
    ],
    roadmap_phase: "PHASE 6 — SIGNING",
};

/// Zero-sized handle registered in [`crate::plugin::Registry`].
pub struct SignPlugin;

/// The single instance the registry holds.
pub static PLUGIN: SignPlugin = SignPlugin;

impl Plugin for SignPlugin {
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
        assert_eq!(CONTRACT.id, "omni.plugin.sign");
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
        assert_eq!(error.origin, "omni.plugin.sign");
        assert!(error.message.contains("not implemented"));
        assert!(error.suggestion.is_some());
    }
}
