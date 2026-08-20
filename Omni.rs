//! # Omni Core
//!
//! Crate root of the Omni_Builder Rust Core.
//!
//! ## Module contract (directive section 2)
//!
//! | Field                | Value                                                                    |
//! |----------------------|--------------------------------------------------------------------------|
//! | Module               | `omni_core`                                                              |
//! | Purpose              | System infrastructure for the Omni_Builder toolchain ecosystem.          |
//! | Scope                | Status model, diagnostics, capability security, plugin contracts,        |
//! |                      | toolchain lock verification, C ABI boundary.                             |
//! | Responsibilities     | Own the vocabulary every subsystem shares; decide capability grants;     |
//! |                      | verify the pinned toolchain against an observed environment; expose a    |
//! |                      | stable C ABI to the JNI layer.                                           |
//! | Non-Responsibilities | Compilation, parsing, code generation, DEX, APK, resources, signing,     |
//! |                      | filesystem access, networking, process execution. Those belong to        |
//! |                      | plugins (invariants I1, I2).                                             |
//! | Inputs               | An observed-environment key/value string supplied by the host UI.        |
//! | Outputs              | Deterministic JSON reports; diagnostics.                                 |
//! | Interfaces           | Rust API (this crate) and the `omni_*` C ABI in [`ffi`].                 |
//! | Dependencies         | Rust standard library only. Zero third-party crates.                     |
//! | State                | Stateless. Every report is a pure function of its inputs plus compiled-in |
//! |                      | constants. No global mutable state (directive section 64).               |
//! | Security             | Default-deny capability model. The Core requests no capability itself.   |
//! | Performance          | All contracts are `&'static` constants; report generation allocates only |
//! |                      | the output string.                                                       |
//! | Failure Modes        | See [`FailureClass`]. FFI never unwinds across the ABI boundary.         |
//! | Diagnostics          | See [`diag`].                                                            |
//! | Tests                | Unit tests at the bottom of this file and in every `Plugins/*.rs` file.  |
//! | Compatibility        | Rust 1.97.1, edition 2021, `no_std` not supported.                       |
//! | Determinism          | Report ordering is declaration order; no time, locale or RNG is read.    |
//! | Status               | [`Status::Foundation`]                                                   |
//!
//! ### Acceptance criteria for this phase
//!
//! 1. The crate compiles with zero third-party dependencies.
//! 2. `cargo test` passes on the host.
//! 3. Every plugin listed in directive section 6 exposes a real, inspectable
//!    contract, and none of them claims to be implemented.
//! 4. The toolchain lock of directive section 14 is encoded verbatim and can be
//!    verified against an observed environment, reporting mismatches honestly.
//! 5. The C ABI never panics across the boundary and never leaks the allocator.
//!
//! ## Architectural Decision Records (directive section 47)
//!
//! ### ADR-0001 — Capitalised Gradle and CMake file names cannot be used
//!
//! * **Context.** Directive section 46 spells the build files `Build.gradle.kts`,
//!   `Settings.gradle.kts`, `Gradle.properties` and `CMakelist.txt`.
//! * **Alternatives.** (a) Keep the capitalised names. (b) Use the names the
//!   tools require. (c) Add shim files under both names.
//! * **Decision.** (b).
//! * **Reason.** Gradle resolves a settings file only as `settings.gradle`,
//!   `settings.gradle.kts` or `settings.gradle.dcl`; this was verified empirically
//!   against Gradle 8.14.3, which rejected `Settings.gradle.kts` outright. The
//!   `-c/--settings-file` escape hatch was removed in Gradle 9. CMake likewise
//!   resolves only `CMakeLists.txt`. Capitalised names would produce a repository
//!   that cannot build, which directive section 1 forbids more strongly than
//!   section 46 requires the spelling.
//! * **Tradeoffs.** The tree deviates from section 46 in letter case only. Every
//!   directory, every file and every nesting level is otherwise preserved.
//! * **Security impact.** None.
//! * **Performance impact.** None.
//! * **Migration plan.** None required; renaming back would break the build.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0002 — The Core lives in one root-level crate file
//!
//! * **Context.** Section 46 lists no location for the Omni Core, but Rust
//!   requires a manifest and a crate-root file.
//! * **Alternatives.** (a) A `Core/` directory. (b) Place the Core under
//!   `Builder/Source/Main/Native/`. (c) Two root-level files only.
//! * **Decision.** (c): `Cargo.toml` and `Omni.rs` at the repository root.
//! * **Reason.** It is the theoretical minimum — no new directory, no module
//!   explosion (section 46), and the root already hosts build-system files.
//! * **Tradeoffs.** A single large Core file. Acceptable while the Core is a
//!   foundation; a split requires a new ADR and an explicit justification.
//! * **Security impact.** None.
//! * **Performance impact.** None.
//! * **Migration plan.** Splitting the Core changes only `[lib] path`.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0003 — The Core carries zero third-party dependencies
//!
//! * **Context.** Sections 31 and 63 demand pinned, verified, inventoried
//!   dependencies for anything that reaches a production artifact.
//! * **Alternatives.** (a) Use `serde`/`serde_json` for reports. (b) Hand-write a
//!   minimal JSON writer.
//! * **Decision.** (b).
//! * **Reason.** The Core emits JSON but never parses untrusted JSON, so the
//!   surface is a few hundred bytes of escaping logic. Zero dependencies means an
//!   empty supply-chain attack surface and a trivially auditable SBOM.
//! * **Tradeoffs.** The writer is write-only and deliberately not a general JSON
//!   library. Parsing untrusted input will require a separate, reviewed decision.
//! * **Security impact.** Positive: no transitive code in the Core.
//! * **Performance impact.** Negligible.
//! * **Migration plan.** Adding a dependency requires a new ADR plus a provenance
//!   record in the toolchain lock.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0005 — CMake is provisioned from upstream, not from the Android SDK
//!
//! * **Context.** Directive section 14 pins CMake 4.x, and the chosen point
//!   release is 4.4.2. `sdkmanager` publishes CMake only up to 4.1.2.
//! * **Alternatives.** (a) Accept the newest CMake the SDK offers. (b) Let AGP
//!   choose, which silently installs 3.22.1. (c) Provision 4.4.2 from Kitware and
//!   point AGP at it with `cmake.dir`.
//! * **Decision.** (c).
//! * **Reason.** (b) is an unpinned toolchain, which section 14 forbids outright.
//!   (a) would quietly rewrite the pin to whatever Google happens to ship. (c)
//!   keeps the pin exact and the provenance explicit.
//! * **Tradeoffs.** The build depends on a `cmake.dir` entry in
//!   `local.properties`, which is host-specific and not committed. The
//!   `verifyCmakeToolchain` task turns a missing or mismatched entry into a
//!   precise failure instead of a confusing one. The Kitware archive ships no
//!   Ninja, so the generator is taken from the SDK's own CMake package.
//! * **Security impact.** The archive is verified against a recorded SHA-256
//!   before use (directive section 31); nothing is trusted because it downloaded
//!   successfully.
//! * **Performance impact.** None.
//! * **Migration plan.** When the SDK publishes 4.4.2 or later, the provisioning
//!   step can be replaced by an `sdkmanager` package and this ADR superseded.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0006 — Kotlin is pinned through the Build Tools API
//!
//! * **Context.** From AGP 9.0 the Android plugin carries its own Kotlin and
//!   offers no DSL to select the version; `android.builtInKotlin` only turns the
//!   feature on or off. AGP 9.3.0 supplies Kotlin 2.2.10. Directive section 14
//!   pins 2.4.10, and the pin is not negotiable.
//! * **Alternatives.** (a) Accept 2.2.10 and record the drift. (b) Set
//!   `android.builtInKotlin=false`, which also requires `android.newDsl=false`,
//!   and apply the standalone Kotlin plugin. (c) Force every
//!   `org.jetbrains.kotlin` module to 2.4.10 with a resolution rule alone.
//!   (d) Compile through the Kotlin Build Tools API, which is the mechanism
//!   Kotlin provides for driving a compiler other than the plugin's own, and set
//!   `compilerVersion` to the pinned value.
//! * **Decision.** (d), with (c) kept alongside so the standard library that
//!   reaches the APK matches the compiler that produced the bytecode.
//! * **Reason.** (a) leaves the toolchain lock unmet, which section 14 does not
//!   permit. (b) works today but is removed in AGP 10 and would force this
//!   project back onto a deprecated DSL. (c) was tried first and is **not
//!   sufficient on its own**: the Build Tools API refuses a
//!   `kotlin-build-tools-impl` whose version differs from the plugin's unless
//!   `kotlin.compiler.runViaBuildToolsApi` is enabled. That refusal stayed
//!   hidden for a while because no Kotlin source was reaching the compiler at
//!   all, so the compile task never ran; see ADR-0008.
//! * **Tradeoffs.** `compilerVersion` is marked experimental by the Kotlin
//!   Gradle Plugin, so the opt-in is explicit in the build script. AGP 9.3.0 was
//!   not tested by Google against Kotlin 2.4.10, which makes the combination
//!   this project's responsibility. `verifyKotlinToolchain` fails the build if
//!   the pin stops taking effect.
//! * **Security impact.** None. Both versions come from the same pinned
//!   repository.
//! * **Performance impact.** None measured.
//! * **Migration plan.** When AGP ships 2.4.10 or later, the compilerVersion
//!   setting and the resolution rule both become no-ops and can be removed.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0008 — The build proves the APK contains the code it declares
//!
//! * **Context.** The Kotlin source directory was redirected on the `java`
//!   source set but not on the `kotlin` one. AGP's built-in Kotlin compiles what
//!   `kotlin.directories` names, so `compileDebugKotlin` reported `NO-SOURCE`.
//!   The build succeeded. The APK was well formed, correctly aligned, signed and
//!   installable, and it contained none of the module's code — only the
//!   generated resource classes. The application died at launch with
//!   `ClassNotFoundException` for its own activity.
//! * **Alternatives.** (a) Fix the source set and move on. (b) Fix it, and make
//!   the build check that every class the manifest names is actually present.
//! * **Decision.** (b).
//! * **Reason.** Every existing gate passed on that APK, because every gate was
//!   asking about packaging and none was asking whether the application was in
//!   there. A missing-source failure is silent by construction: an empty source
//!   set is indistinguishable from a module with nothing to compile. Directive
//!   section 55 requires a regression test for every defect, and the only
//!   meaningful one here inspects the finished artifact.
//! * **Tradeoffs.** The check unpacks each APK and reads its dex files, costing
//!   about a second per build.
//! * **Security impact.** None directly, though an artifact that does not
//!   contain the code it claims to is an integrity problem in the sense of
//!   directive section 58.
//! * **Performance impact.** Negligible against a full build.
//! * **Migration plan.** When the Omni build engine replaces AGP, this check
//!   moves with the packaging step rather than being dropped.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0007 — The bootstrap APK is signed by the build, not by a repacker
//!
//! * **Context.** An unsigned release APK cannot be installed, and signing it
//!   with a third-party tool that repacks the archive produced "App not
//!   installed" with no further explanation. Two independent causes were
//!   measured. First, an application targeting API 36 is refused at install time
//!   unless it carries an APK Signature Scheme v2 or later signature, and a
//!   v1-only JAR signature is still the default in some tools. Second, the
//!   manifest declares `extractNativeLibs="false"`, so the platform maps each
//!   native library straight out of the APK; a repacker re-zips the archive,
//!   turning stored entries into deflated ones, and a deflated library cannot be
//!   mapped.
//! * **Alternatives.** (a) Set `extractNativeLibs="true"` so repacking stops
//!   mattering. (b) Commit a keystore so the build can always sign. (c) Sign from
//!   the build using a keystore referenced from outside the repository, and check
//!   the finished APK for both failure modes.
//! * **Decision.** (c).
//! * **Reason.** (a) trades a real improvement — smaller installs and libraries
//!   the platform can map directly, which 16 KB page devices need — for tolerance
//!   of a tool that should not be in the pipeline at all. (b) is precisely what
//!   directive section 25 forbids. (c) removes the need for an external signer
//!   and turns both failures into build errors that say what is wrong.
//! * **Tradeoffs.** Signing a release requires four settings in
//!   `local.properties` or the environment. Without them the release artifact
//!   stays unsigned, which is honest rather than convenient. The debug APK is
//!   signed by the standard debug key and installs as it always did.
//! * **Security impact.** No key material enters the repository, the build log or
//!   any diagnostic (directive sections 25 and 57). A partly configured identity
//!   fails the build instead of silently producing an unsigned APK.
//! * **Performance impact.** None.
//! * **Migration plan.** When the Omni signing subsystem is real (roadmap phase
//!   12), it replaces the AGP signing config. `Plugins/Sign.rs` stays PLANNED
//!   until then; this ADR covers bootstrap signing only.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0009 — The Core stays one file, and the trigger for splitting it
//!
//! * **Context.** ADR-0002 put the Core in a single root-level file and said a
//!   split would need its own decision record. Phase 2 took that file past eight
//!   thousand lines across ten modules.
//! * **Alternatives.** (a) Split now, one file per module. (b) Keep one file.
//!   (c) Keep one file and write down what would make (a) the right answer.
//! * **Decision.** (c).
//! * **Reason.** The file is large but not tangled: every module is an inner
//!   `mod` with its own contract, its dependencies point one way, and the tests
//!   sit at the end where they can reach everything. Splitting has a real cost
//!   here — directive section 46 fixes the repository layout, and each new file
//!   is a deviation from it needing its own justification, which section 46 also
//!   demands. Size alone is not a reason; difficulty working in it would be.
//! * **Tradeoffs.** Navigating one long file is slower than opening the right
//!   short one, and two people editing different subsystems touch the same file.
//! * **Trigger for revisiting.** Any of: a module needing a dependency the rest
//!   of the Core must not have; compile times making the edit-test loop painful;
//!   or a second contributor working in the Core regularly. Any one of those is
//!   enough — none of them is true today.
//! * **Security impact.** None.
//! * **Performance impact.** None at runtime.
//! * **Migration plan.** A split moves each inner `mod` to its own file and
//!   leaves `Omni.rs` as the crate root that declares them. `[lib] path` does not
//!   change.
//! * **Status.** ACCEPTED.
//!
//! ### ADR-0004 — C++ owns JNI; Rust exposes a plain C ABI
//!
//! * **Context.** Section 46 mandates `Builder.cpp` / `Builder.hpp` next to the
//!   CMake file, and the Kotlin UI needs to reach the Core.
//! * **Alternatives.** (a) Rust exports `Java_*` symbols directly. (b) C++ holds
//!   the JNI layer and calls a C ABI exported by Rust.
//! * **Decision.** (b).
//! * **Reason.** It keeps JVM specifics out of the Core, matches the file layout
//!   the directive mandates, and keeps the Rust surface testable from plain C.
//! * **Tradeoffs.** One extra language on the boundary.
//! * **Security impact.** The Core never sees a `JNIEnv`, so it cannot reach JVM
//!   state; `Capability::Jni` stays outside the Core.
//! * **Performance impact.** One extra call per bridge invocation.
//! * **Migration plan.** The C ABI is versioned by [`ffi::OMNI_ABI_VERSION`].
//! * **Status.** ACCEPTED.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
// A `Diagnostic` is around 200 bytes, which Clippy flags whenever one is the
// error half of a `Result`. It is the error half of nearly every `Result` here,
// deliberately: the diagnostic *is* what the caller needs, and boxing it would
// add an allocation to every failure path in the Core in exchange for nothing
// measurable. Revisit if profiling ever says otherwise (directive section 10).
#![allow(clippy::result_large_err)]

// ---------------------------------------------------------------------------
// Plugin modules (directive section 6).
//
// Section 46 fixes these nine files. They are modules of this crate, so the
// repository grows no new Rust files and no new directories.
// ---------------------------------------------------------------------------

#[path = "Plugins/Apk.rs"]
pub mod apk;
#[path = "Plugins/Cpp.rs"]
pub mod cpp;
#[path = "Plugins/Dex.rs"]
pub mod dex;
#[path = "Plugins/Guard.rs"]
pub mod guard;
#[path = "Plugins/Java.rs"]
pub mod java;
#[path = "Plugins/Kotlin.rs"]
pub mod kotlin;
#[path = "Plugins/Resources.rs"]
pub mod resources;
#[path = "Plugins/Rust.rs"]
pub mod rust;
#[path = "Plugins/Sign.rs"]
pub mod sign;

// ===========================================================================
// Core identity
// ===========================================================================

/// Semantic version of the Core, taken from `Cargo.toml` at compile time.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Phase of the roadmap in directive section 52 that this tree implements.
pub const CORE_PHASE: &str = "PHASE 2 — OMNI CORE";

/// Maturity of the Core as a whole. Never raise this without the quality gates
/// of directive section 51.
pub const CORE_STATUS: Status = Status::Foundation;

/// Honest statement of what still runs on borrowed infrastructure.
///
/// Directive section 15 requires bootstrap dependencies to be reported rather
/// than hidden, and section 53 forbids any self-hosting claim while they exist.
pub const BOOTSTRAP_DEPENDENCIES: &[&str] = &[
    "Gradle — drives the Android application build",
    "Android Gradle Plugin — packages and signs the bootstrap APK",
    "Kotlin compiler — compiles the builder user interface",
    "Android NDK / Clang — compiles the JNI bridge and links the Core",
    "CMake — configures the native build",
    "JDK — hosts Gradle and the Kotlin compiler",
    "rustc / cargo — compiles the Core itself",
];

// ===========================================================================
// Status model (directive section 1)
// ===========================================================================

/// Maturity of a subsystem.
///
/// Directive section 1 forbids presenting an unfinished subsystem as finished.
/// Every contract in this tree carries one of these values, and the user
/// interface renders it verbatim.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Status {
    /// Specified, not implemented. No code path produces an artifact.
    Planned,
    /// Contracts and scaffolding are real; the subsystem does no useful work yet.
    Foundation,
    /// Some of the specified behaviour is implemented; the rest is absent.
    Partial,
    /// Implemented, but the contract may still change without notice.
    Experimental,
    /// Contract frozen, implementation complete, hardening still in progress.
    Beta,
    /// Passed every gate in directive section 51.
    Production,
    /// Superseded. Retained only for compatibility.
    Deprecated,
}

impl Status {
    /// Stable machine-readable name. Used in reports and in the user interface.
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::Planned => "PLANNED",
            Status::Foundation => "FOUNDATION",
            Status::Partial => "PARTIAL",
            Status::Experimental => "EXPERIMENTAL",
            Status::Beta => "BETA",
            Status::Production => "PRODUCTION",
            Status::Deprecated => "DEPRECATED",
        }
    }

    /// Whether a subsystem in this state may produce an artifact that is
    /// published (directive section 58) rather than merely inspected.
    pub const fn may_produce_artifacts(self) -> bool {
        matches!(
            self,
            Status::Experimental | Status::Beta | Status::Production
        )
    }
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ===========================================================================
// Subsystem inventory (directive section 1)
// ===========================================================================

/// What one subsystem of the Core is, and honestly is not.
///
/// Directive section 1 forbids presenting an unfinished subsystem as finished.
/// A comment saying so is easy to write and easy to forget; this table is
/// rendered by the user interface, so what it says is what a person reads.
#[derive(Clone, Copy, Debug)]
pub struct Subsystem {
    /// Human-facing name.
    pub name: &'static str,
    /// Maturity.
    pub status: Status,
    /// Section of the directive that specifies it.
    pub directive_section: u16,
    /// One sentence on what it does today.
    pub summary: &'static str,
    /// What the specification asks for that is not built.
    ///
    /// An empty list means nothing specified is missing. It does not mean the
    /// subsystem has reached [`Status::Production`]; the gates of directive
    /// section 51 decide that.
    pub missing: &'static [&'static str],
}

/// Every subsystem of the Core, in the order the directive introduces them.
pub const SUBSYSTEMS: &[Subsystem] = &[
    Subsystem {
        name: "Diagnostics",
        status: Status::Partial,
        directive_section: 33,
        summary: "Coded, located, actionable diagnostics with severity and origin.",
        missing: &["Related-diagnostic graphs are modelled but never populated."],
    },
    Subsystem {
        name: "Capability security",
        status: Status::Partial,
        directive_section: 7,
        summary: "Default-deny capability policy with an audit record per request.",
        missing: &[
            "Policies are built in code; nothing loads one from a project.",
            "Plugins are not isolated at runtime, only by contract.",
        ],
    },
    Subsystem {
        name: "SHA-256",
        status: Status::Beta,
        directive_section: 30,
        summary: "FIPS 180-4 SHA-256, verified against the published NIST vectors.",
        missing: &["Not fuzzed yet, which directive section 41 asks for."],
    },
    Subsystem {
        name: "Virtual filesystem",
        status: Status::Partial,
        directive_section: 8,
        summary: "Named mounts, normalised paths, traversal and symlink refusal, \
                  quotas, atomic writes.",
        missing: &[
            "No locking.",
            "No snapshot or rollback.",
            "No temporary-file lifetime management beyond a single write.",
        ],
    },
    Subsystem {
        name: "Project model",
        status: Status::Partial,
        directive_section: 44,
        summary: "Omni.toml parsed, validated and reduced to a configuration digest.",
        missing: &[
            "No dependency declarations; nothing resolves dependencies yet.",
            "The manifest is read from text, not from the virtual filesystem.",
        ],
    },
    Subsystem {
        name: "Artifact lifecycle",
        status: Status::Partial,
        directive_section: 58,
        summary: "CREATED to PUBLISHED as a state machine that refuses illegal steps.",
        missing: &[
            "Nothing signs an artifact, so SIGNED is reachable and unused.",
            "No artifact store; artifacts are described, not kept.",
        ],
    },
    Subsystem {
        name: "Incremental cache",
        status: Status::Foundation,
        directive_section: 11,
        summary: "Cache keys over every input directive section 11 names, and four \
                  distinguishable lookup outcomes.",
        missing: &[
            "In-memory only; nothing survives a restart.",
            "No eviction policy.",
            "No stored bytes, so a hit proves a key matched, not that a result exists.",
        ],
    },
    Subsystem {
        name: "Build graph",
        status: Status::Partial,
        directive_section: 9,
        summary: "A real DAG with deterministic ordering and cycle detection.",
        missing: &[
            "Nothing constructs a graph from a project yet.",
            "No graph is persisted between builds, so nothing can be compared.",
        ],
    },
    Subsystem {
        name: "Scheduler",
        status: Status::Partial,
        directive_section: 10,
        summary: "Executes a graph in dependency order, propagates failure, honours \
                  cancellation.",
        missing: &[
            "Sequential only; nothing runs in parallel.",
            "Not memory, battery or thermal aware, which directive section 36 requires.",
            "No checkpoint or resume inside a node.",
        ],
    },
    Subsystem {
        name: "Toolchain lock",
        status: Status::Partial,
        directive_section: 14,
        summary: "Pinned versions with provenance, verified against an observed \
                  environment.",
        missing: &["Only two components can be observed from a device."],
    },
];

// ===========================================================================
// Failure model (directive section 34)
// ===========================================================================

/// Classification every subsystem must be able to distinguish.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FailureClass {
    /// The operation completed as specified.
    Success,
    /// The operation failed but the build may continue or be retried.
    Recoverable,
    /// The project or the request is wrong.
    UserError,
    /// The configuration or manifest is wrong.
    ConfigurationError,
    /// A required tool is missing, mismatched or misbehaving.
    ToolchainError,
    /// A security policy denied the operation.
    SecurityFailure,
    /// Data that was expected to be intact is not.
    Corruption,
    /// Memory, storage, CPU or time budget was exhausted.
    ResourceExhaustion,
    /// A defect in Omni_Builder itself.
    InternalError,
    /// The operation was cancelled (directive section 35).
    Cancellation,
}

impl FailureClass {
    /// Stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            FailureClass::Success => "SUCCESS",
            FailureClass::Recoverable => "RECOVERABLE_FAILURE",
            FailureClass::UserError => "USER_ERROR",
            FailureClass::ConfigurationError => "CONFIGURATION_ERROR",
            FailureClass::ToolchainError => "TOOLCHAIN_ERROR",
            FailureClass::SecurityFailure => "SECURITY_FAILURE",
            FailureClass::Corruption => "CORRUPTION",
            FailureClass::ResourceExhaustion => "RESOURCE_EXHAUSTION",
            FailureClass::InternalError => "INTERNAL_ERROR",
            FailureClass::Cancellation => "CANCELLATION",
        }
    }
}

impl core::fmt::Display for FailureClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ===========================================================================
// json — minimal, write-only JSON emitter (ADR-0003)
// ===========================================================================

/// Deterministic, allocation-light JSON writer.
///
/// This is intentionally *not* a general JSON library: it can only write, it
/// never parses, and it exposes no way to emit a malformed document from safe
/// code. Every report the Core hands to the host goes through it.
pub mod json {
    /// Accumulates a JSON document.
    #[derive(Debug, Default)]
    pub struct Writer {
        buf: String,
        /// `true` when at least one member has been written at the current depth.
        needs_comma: Vec<bool>,
    }

    impl Writer {
        /// Creates an empty writer.
        pub fn new() -> Self {
            Writer {
                buf: String::with_capacity(1024),
                needs_comma: Vec::with_capacity(8),
            }
        }

        fn separate(&mut self) {
            if let Some(last) = self.needs_comma.last_mut() {
                if *last {
                    self.buf.push(',');
                } else {
                    *last = true;
                }
            }
        }

        /// Opens an object, optionally as the value of `key`.
        pub fn begin_object(&mut self, key: Option<&str>) {
            self.separate();
            if let Some(k) = key {
                self.write_escaped(k);
                self.buf.push(':');
            }
            self.buf.push('{');
            self.needs_comma.push(false);
        }

        /// Closes the innermost object.
        pub fn end_object(&mut self) {
            self.needs_comma.pop();
            self.buf.push('}');
        }

        /// Opens an array, optionally as the value of `key`.
        pub fn begin_array(&mut self, key: Option<&str>) {
            self.separate();
            if let Some(k) = key {
                self.write_escaped(k);
                self.buf.push(':');
            }
            self.buf.push('[');
            self.needs_comma.push(false);
        }

        /// Closes the innermost array.
        pub fn end_array(&mut self) {
            self.needs_comma.pop();
            self.buf.push(']');
        }

        /// Writes a string member.
        pub fn field_str(&mut self, key: &str, value: &str) {
            self.separate();
            self.write_escaped(key);
            self.buf.push(':');
            self.write_escaped(value);
        }

        /// Writes an unsigned integer member.
        pub fn field_u64(&mut self, key: &str, value: u64) {
            self.separate();
            self.write_escaped(key);
            self.buf.push(':');
            self.buf.push_str(&value.to_string());
        }

        /// Writes a boolean member.
        pub fn field_bool(&mut self, key: &str, value: bool) {
            self.separate();
            self.write_escaped(key);
            self.buf.push(':');
            self.buf.push_str(if value { "true" } else { "false" });
        }

        /// Writes a bare string element inside an array.
        pub fn element_str(&mut self, value: &str) {
            self.separate();
            self.write_escaped(value);
        }

        /// Consumes the writer and returns the document.
        ///
        /// # Panics
        ///
        /// Panics if a container was left open. Callers inside the Core always
        /// balance their containers; the FFI layer additionally catches panics
        /// so this can never cross the ABI boundary.
        pub fn finish(self) -> String {
            assert!(
                self.needs_comma.is_empty(),
                "omni_core::json::Writer::finish called with {} unclosed container(s)",
                self.needs_comma.len()
            );
            self.buf
        }

        /// Escapes per RFC 8259 section 7, including the C0 control range.
        fn write_escaped(&mut self, value: &str) {
            self.buf.push('"');
            for ch in value.chars() {
                match ch {
                    '"' => self.buf.push_str("\\\""),
                    '\\' => self.buf.push_str("\\\\"),
                    '\n' => self.buf.push_str("\\n"),
                    '\r' => self.buf.push_str("\\r"),
                    '\t' => self.buf.push_str("\\t"),
                    '\u{08}' => self.buf.push_str("\\b"),
                    '\u{0C}' => self.buf.push_str("\\f"),
                    c if (c as u32) < 0x20 => {
                        self.buf.push_str(&format!("\\u{:04x}", c as u32));
                    }
                    c => self.buf.push(c),
                }
            }
            self.buf.push('"');
        }
    }
}

// ===========================================================================
// diag — unified diagnostics (directive section 33)
// ===========================================================================

/// Diagnostic model shared by the Core and every plugin.
pub mod diag {
    use crate::{json::Writer, FailureClass};

    /// How much a diagnostic matters.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub enum Severity {
        /// Machine-oriented detail, off by default.
        Trace,
        /// Progress and state, useful when investigating.
        Info,
        /// Something is suspicious but the operation continues.
        Warning,
        /// The operation failed.
        Error,
        /// The operation failed and the build cannot continue.
        Fatal,
    }

    impl Severity {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Severity::Trace => "TRACE",
                Severity::Info => "INFO",
                Severity::Warning => "WARNING",
                Severity::Error => "ERROR",
                Severity::Fatal => "FATAL",
            }
        }

        /// Whether a diagnostic of this severity stops the build.
        pub const fn is_blocking(self) -> bool {
            matches!(self, Severity::Error | Severity::Fatal)
        }
    }

    impl core::fmt::Display for Severity {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    /// Where in a source file a diagnostic points.
    ///
    /// Lines and columns are 1-based, matching every editor the diagnostics are
    /// rendered in. A location with no line refers to the file as a whole.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Location {
        /// Path as the user wrote it, never an absolute host path.
        pub file: String,
        /// 1-based line, or `0` when the diagnostic covers the whole file.
        pub line: u32,
        /// 1-based column, or `0` when the diagnostic covers the whole line.
        pub column: u32,
    }

    impl Location {
        /// A location covering an entire file.
        pub fn file(path: impl Into<String>) -> Self {
            Location {
                file: path.into(),
                line: 0,
                column: 0,
            }
        }

        /// A precise location.
        pub fn at(path: impl Into<String>, line: u32, column: u32) -> Self {
            Location {
                file: path.into(),
                line,
                column,
            }
        }
    }

    impl core::fmt::Display for Location {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match (self.line, self.column) {
                (0, _) => write!(f, "{}", self.file),
                (l, 0) => write!(f, "{}:{}", self.file, l),
                (l, c) => write!(f, "{}:{}:{}", self.file, l, c),
            }
        }
    }

    /// A single actionable message.
    ///
    /// Directive section 33 requires diagnostics to be actionable, not merely
    /// technically correct, which is why [`Diagnostic::suggestion`] exists and
    /// why [`Diagnostic::origin`] names the subsystem that produced it.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Diagnostic {
        /// Stable code such as `E1004`. Never reused for a different meaning.
        pub code: String,
        /// Severity of this message.
        pub severity: Severity,
        /// Failure classification (directive section 34).
        pub class: FailureClass,
        /// Subsystem that emitted the diagnostic, for example `core.toolchain`.
        pub origin: String,
        /// One-sentence statement of the problem.
        pub message: String,
        /// Optional source position.
        pub location: Option<Location>,
        /// Optional detail: what was expected, what was found.
        pub context: Vec<String>,
        /// Optional remedy the user can act on.
        pub suggestion: Option<String>,
        /// Codes of diagnostics that explain this one.
        pub related: Vec<String>,
    }

    impl Diagnostic {
        /// Creates a diagnostic with the mandatory fields set.
        pub fn new(
            code: impl Into<String>,
            severity: Severity,
            class: FailureClass,
            origin: impl Into<String>,
            message: impl Into<String>,
        ) -> Self {
            Diagnostic {
                code: code.into(),
                severity,
                class,
                origin: origin.into(),
                message: message.into(),
                location: None,
                context: Vec::new(),
                suggestion: None,
                related: Vec::new(),
            }
        }

        /// Attaches a source position.
        pub fn with_location(mut self, location: Location) -> Self {
            self.location = Some(location);
            self
        }

        /// Appends a line of context.
        pub fn with_context(mut self, line: impl Into<String>) -> Self {
            self.context.push(line.into());
            self
        }

        /// Attaches an actionable remedy.
        pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
            self.suggestion = Some(suggestion.into());
            self
        }

        /// Links a related diagnostic code.
        pub fn with_related(mut self, code: impl Into<String>) -> Self {
            self.related.push(code.into());
            self
        }

        /// Serialises this diagnostic into an open JSON array.
        pub fn write_json(&self, w: &mut Writer) {
            w.begin_object(None);
            w.field_str("code", &self.code);
            w.field_str("severity", self.severity.as_str());
            w.field_str("class", self.class.as_str());
            w.field_str("origin", &self.origin);
            w.field_str("message", &self.message);
            if let Some(loc) = &self.location {
                w.begin_object(Some("location"));
                w.field_str("file", &loc.file);
                w.field_u64("line", loc.line as u64);
                w.field_u64("column", loc.column as u64);
                w.end_object();
            }
            w.begin_array(Some("context"));
            for line in &self.context {
                w.element_str(line);
            }
            w.end_array();
            if let Some(suggestion) = &self.suggestion {
                w.field_str("suggestion", suggestion);
            }
            w.begin_array(Some("related"));
            for code in &self.related {
                w.element_str(code);
            }
            w.end_array();
            w.end_object();
        }
    }

    impl core::fmt::Display for Diagnostic {
        /// Renders the human-facing form shown in directive section 33.
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{} [{}]", self.code, self.severity)?;
            if let Some(loc) = &self.location {
                write!(f, " {}", loc)?;
            }
            write!(f, "\n{}", self.message)?;
            for line in &self.context {
                write!(f, "\n  {}", line)?;
            }
            if let Some(s) = &self.suggestion {
                write!(f, "\nSuggestion: {}", s)?;
            }
            Ok(())
        }
    }

    /// Ordered collection of diagnostics.
    ///
    /// Insertion order is preserved so that reports stay byte-identical across
    /// runs with identical input (directive section 12).
    #[derive(Clone, Default, Debug)]
    pub struct Sink {
        entries: Vec<Diagnostic>,
    }

    impl Sink {
        /// Creates an empty sink.
        pub fn new() -> Self {
            Sink {
                entries: Vec::new(),
            }
        }

        /// Records a diagnostic.
        pub fn emit(&mut self, diagnostic: Diagnostic) {
            self.entries.push(diagnostic);
        }

        /// All recorded diagnostics, in emission order.
        pub fn entries(&self) -> &[Diagnostic] {
            &self.entries
        }

        /// Number of recorded diagnostics.
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether nothing has been recorded.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Whether any recorded diagnostic stops the build.
        ///
        /// This is the single question the scheduler asks before treating a node
        /// as successful; directive section 10 forbids ignoring a failure.
        pub fn has_blocking(&self) -> bool {
            self.entries.iter().any(|d| d.severity.is_blocking())
        }

        /// Highest severity recorded, if any.
        pub fn max_severity(&self) -> Option<Severity> {
            self.entries.iter().map(|d| d.severity).max()
        }

        /// Serialises every diagnostic as a JSON array member of `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_array(Some(key));
            for d in &self.entries {
                d.write_json(w);
            }
            w.end_array();
        }
    }
}

// ===========================================================================
// caps — capability security (directive sections 3-I3 and 7)
// ===========================================================================

/// Default-deny capability model.
///
/// Directive section 7 states that no plugin holds any privilege implicitly.
/// The pipeline is `Request -> Policy -> Grant/Deny -> Audit -> Execution`, and
/// this module implements all five stages. Nothing here performs the privileged
/// operation itself; it only decides and records.
pub mod caps {
    use crate::json::Writer;

    /// A privilege a plugin may request.
    ///
    /// The list is closed on purpose: adding a variant is an architectural
    /// change that needs an ADR, so privilege cannot creep in silently.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub enum Capability {
        /// Read through the virtual filesystem.
        FsRead,
        /// Write through the virtual filesystem.
        FsWrite,
        /// Spawn a process.
        ProcessExec,
        /// Reach the local network.
        Network,
        /// Reach the public internet.
        Internet,
        /// Use cryptographic primitives.
        Crypto,
        /// Touch private key material.
        KeyAccess,
        /// Call into the JVM.
        Jni,
        /// Load or run native code.
        Native,
        /// Use scratch storage that does not survive the build.
        TempStorage,
        /// Read or write the incremental build cache.
        Cache,
        /// Query device state.
        Device,
        /// Emit an artifact that carries identity or secrets.
        SensitiveOutput,
    }

    impl Capability {
        /// Stable machine-readable name, matching directive section 7.
        pub const fn as_str(self) -> &'static str {
            match self {
                Capability::FsRead => "FS_READ",
                Capability::FsWrite => "FS_WRITE",
                Capability::ProcessExec => "PROCESS_EXEC",
                Capability::Network => "NETWORK",
                Capability::Internet => "INTERNET",
                Capability::Crypto => "CRYPTO",
                Capability::KeyAccess => "KEY_ACCESS",
                Capability::Jni => "JNI",
                Capability::Native => "NATIVE",
                Capability::TempStorage => "TEMP_STORAGE",
                Capability::Cache => "CACHE",
                Capability::Device => "DEVICE",
                Capability::SensitiveOutput => "SENSITIVE_OUTPUT",
            }
        }

        /// Every capability, in declaration order.
        pub const ALL: &'static [Capability] = &[
            Capability::FsRead,
            Capability::FsWrite,
            Capability::ProcessExec,
            Capability::Network,
            Capability::Internet,
            Capability::Crypto,
            Capability::KeyAccess,
            Capability::Jni,
            Capability::Native,
            Capability::TempStorage,
            Capability::Cache,
            Capability::Device,
            Capability::SensitiveOutput,
        ];

        /// Whether misuse of this capability can leak identity or secrets.
        ///
        /// Directive sections 25, 56 and 57 forbid key material from reaching a
        /// log, a diagnostic or an artifact, so grants of these capabilities are
        /// always audited even when the policy allows them.
        pub const fn is_sensitive(self) -> bool {
            matches!(
                self,
                Capability::KeyAccess
                    | Capability::Crypto
                    | Capability::SensitiveOutput
                    | Capability::ProcessExec
                    | Capability::Network
                    | Capability::Internet
            )
        }
    }

    impl core::fmt::Display for Capability {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    /// Outcome of a capability request.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Decision {
        /// The policy allows the request.
        Grant,
        /// The policy refuses the request.
        Deny,
    }

    impl Decision {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Decision::Grant => "GRANT",
                Decision::Deny => "DENY",
            }
        }

        /// Whether the caller may proceed.
        pub const fn is_granted(self) -> bool {
            matches!(self, Decision::Grant)
        }
    }

    /// One entry in the immutable audit trail (directive section 57).
    ///
    /// The record deliberately holds no payload: only who asked for what, and
    /// what the policy answered. It can never contain a key or a credential.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct AuditRecord {
        /// Plugin identifier that made the request.
        pub subject: String,
        /// Capability that was requested.
        pub capability: Capability,
        /// What the policy answered.
        pub decision: Decision,
        /// Why, in one sentence.
        pub reason: String,
    }

    /// A set of granted capabilities plus the audit trail of every request.
    ///
    /// Construction is default-deny: [`Policy::new`] grants nothing. A grant is
    /// only ever added explicitly, which makes the privilege surface of a build
    /// readable at the call site.
    #[derive(Clone, Debug)]
    pub struct Policy {
        /// Name of the policy, recorded in reports.
        name: String,
        granted: Vec<Capability>,
        audit: Vec<AuditRecord>,
    }

    impl Policy {
        /// Creates a policy that grants nothing.
        pub fn new(name: impl Into<String>) -> Self {
            Policy {
                name: name.into(),
                granted: Vec::new(),
                audit: Vec::new(),
            }
        }

        /// Name of this policy.
        pub fn name(&self) -> &str {
            &self.name
        }

        /// Adds a capability to the granted set. Idempotent.
        pub fn grant(&mut self, capability: Capability) -> &mut Self {
            if !self.granted.contains(&capability) {
                self.granted.push(capability);
                self.granted.sort();
            }
            self
        }

        /// Removes a capability from the granted set. Idempotent.
        pub fn revoke(&mut self, capability: Capability) -> &mut Self {
            self.granted.retain(|c| *c != capability);
            self
        }

        /// Capabilities currently granted, in a deterministic order.
        pub fn granted(&self) -> &[Capability] {
            &self.granted
        }

        /// Evaluates a request and records it in the audit trail.
        ///
        /// This is the only way to obtain a [`Decision`]; there is no path that
        /// checks a capability without leaving an audit record behind.
        pub fn request(&mut self, subject: &str, capability: Capability) -> Decision {
            let decision = if self.granted.contains(&capability) {
                Decision::Grant
            } else {
                Decision::Deny
            };
            let reason = match decision {
                Decision::Grant => format!("granted by policy '{}'", self.name),
                Decision::Deny => format!("not granted by policy '{}' (default deny)", self.name),
            };
            self.audit.push(AuditRecord {
                subject: subject.to_string(),
                capability,
                decision,
                reason,
            });
            decision
        }

        /// The audit trail, in request order.
        pub fn audit(&self) -> &[AuditRecord] {
            &self.audit
        }

        /// Serialises the policy and its audit trail.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_str("name", &self.name);
            w.field_str("default", "DENY");
            w.begin_array(Some("granted"));
            for c in &self.granted {
                w.element_str(c.as_str());
            }
            w.end_array();
            w.begin_array(Some("audit"));
            for record in &self.audit {
                w.begin_object(None);
                w.field_str("subject", &record.subject);
                w.field_str("capability", record.capability.as_str());
                w.field_str("decision", record.decision.as_str());
                w.field_str("reason", &record.reason);
                w.field_bool("sensitive", record.capability.is_sensitive());
                w.end_object();
            }
            w.end_array();
            w.end_object();
        }
    }
}

// ===========================================================================
// plugin — plugin contracts and registry (directive sections 6 and 66)
// ===========================================================================

/// Plugin contract surface.
///
/// Directive section 66 requires that adding a compiler needs no Core change.
/// A plugin therefore contributes a [`Contract`] plus an [`Plugin::execute`]
/// implementation, and the Core never names a specific language.
pub mod plugin {
    use crate::caps::{Capability, Decision, Policy};
    use crate::diag::{Diagnostic, Severity};
    use crate::json::Writer;
    use crate::{FailureClass, Status};

    /// Three-component plugin version.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct Version {
        /// Incremented on a breaking contract change (directive section 65).
        pub major: u16,
        /// Incremented when behaviour is added compatibly.
        pub minor: u16,
        /// Incremented for fixes that do not change the contract.
        pub patch: u16,
    }

    impl Version {
        /// Creates a version.
        pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
            Version {
                major,
                minor,
                patch,
            }
        }

        /// Whether a consumer written against `required` can use this version.
        pub const fn is_compatible_with(self, required: Version) -> bool {
            self.major == required.major
                && (self.minor > required.minor
                    || (self.minor == required.minor && self.patch >= required.patch))
        }
    }

    impl core::fmt::Display for Version {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }

    /// Everything the Core knows about a plugin without running it.
    ///
    /// The whole contract is `&'static`, so it costs nothing to inspect and can
    /// be rendered by the user interface without a build being in progress.
    #[derive(Clone, Copy, Debug)]
    pub struct Contract {
        /// Stable identifier, for example `omni.plugin.dex`.
        pub id: &'static str,
        /// Human-facing name.
        pub display_name: &'static str,
        /// Contract version.
        pub version: Version,
        /// Maturity. Directive section 1 forbids overstating this.
        pub status: Status,
        /// One sentence describing the purpose.
        pub summary: &'static str,
        /// Artifact kinds this plugin consumes.
        pub inputs: &'static [&'static str],
        /// Artifact kinds this plugin produces.
        pub outputs: &'static [&'static str],
        /// Capabilities the plugin needs in order to do its job.
        pub required_capabilities: &'static [Capability],
        /// Capabilities that must never be granted to this plugin.
        pub forbidden_capabilities: &'static [Capability],
        /// Work this plugin explicitly does not do (directive section 2).
        pub non_responsibilities: &'static [&'static str],
        /// Roadmap phase in which this plugin becomes real.
        pub roadmap_phase: &'static str,
    }

    impl Contract {
        /// Serialises the contract as an object inside an open array.
        pub fn write_json(&self, w: &mut Writer) {
            w.begin_object(None);
            w.field_str("id", self.id);
            w.field_str("displayName", self.display_name);
            w.field_str("version", &self.version.to_string());
            w.field_str("status", self.status.as_str());
            w.field_bool("mayProduceArtifacts", self.status.may_produce_artifacts());
            w.field_str("summary", self.summary);
            w.field_str("roadmapPhase", self.roadmap_phase);
            w.begin_array(Some("inputs"));
            for i in self.inputs {
                w.element_str(i);
            }
            w.end_array();
            w.begin_array(Some("outputs"));
            for o in self.outputs {
                w.element_str(o);
            }
            w.end_array();
            w.begin_array(Some("requiredCapabilities"));
            for c in self.required_capabilities {
                w.element_str(c.as_str());
            }
            w.end_array();
            w.begin_array(Some("forbiddenCapabilities"));
            for c in self.forbidden_capabilities {
                w.element_str(c.as_str());
            }
            w.end_array();
            w.begin_array(Some("nonResponsibilities"));
            for n in self.non_responsibilities {
                w.element_str(n);
            }
            w.end_array();
            w.end_object();
        }
    }

    /// What a plugin produced.
    ///
    /// Empty in this phase: no plugin in this tree produces an artifact yet, and
    /// directive section 1 forbids pretending otherwise.
    #[derive(Clone, Debug, Default)]
    pub struct Outcome {
        /// Identifiers of artifacts the plugin created.
        pub artifacts: Vec<String>,
    }

    /// Execution environment handed to a plugin.
    ///
    /// The plugin reaches privileged operations only through this context, and
    /// every capability question it asks is audited.
    #[derive(Debug)]
    pub struct Context<'a> {
        /// Policy governing this execution.
        pub policy: &'a mut Policy,
        /// Sink the plugin writes diagnostics into.
        pub diagnostics: &'a mut crate::diag::Sink,
    }

    impl Context<'_> {
        /// Asks the policy for a capability on behalf of `subject`.
        pub fn require(&mut self, subject: &str, capability: Capability) -> Decision {
            self.policy.request(subject, capability)
        }
    }

    /// A unit of build work the Core can schedule.
    ///
    #[allow(unreachable_pub)]
    pub trait Plugin: Sync {
        /// The contract, available without executing anything.
        fn contract(&self) -> &'static Contract;

        /// Performs the plugin's work.
        ///
        /// A plugin whose contract status is [`Status::Planned`] must return the
        /// diagnostic produced by [`unimplemented_diagnostic`] rather than a
        /// fabricated success (directive section 1).
        fn execute(&self, ctx: &mut Context<'_>) -> Result<Outcome, Diagnostic>;
    }

    /// The single, honest failure a not-yet-implemented plugin returns.
    ///
    /// Code `E0001` is reserved permanently for this meaning.
    pub fn unimplemented_diagnostic(contract: &Contract) -> Diagnostic {
        Diagnostic::new(
            "E0001",
            Severity::Error,
            FailureClass::InternalError,
            contract.id,
            format!(
                "{} is not implemented; its status is {}.",
                contract.display_name, contract.status
            ),
        )
        .with_context(format!("Contract version: {}", contract.version))
        .with_context(format!("Scheduled for: {}", contract.roadmap_phase))
        .with_context(format!("Declared outputs: {}", contract.outputs.join(", ")))
        .with_suggestion(
            "This subsystem is declared, not built. No artifact was produced and \
             none should be expected until its status reaches EXPERIMENTAL.",
        )
    }

    /// The nine plugins fixed by directive section 6, in a fixed order.
    ///
    /// Declaration order is part of the contract: it is what makes every report
    /// byte-stable across runs (directive section 12).
    static BUILTIN: &[&'static dyn Plugin] = &[
        &crate::kotlin::PLUGIN,
        &crate::java::PLUGIN,
        &crate::cpp::PLUGIN,
        &crate::rust::PLUGIN,
        &crate::resources::PLUGIN,
        &crate::dex::PLUGIN,
        &crate::apk::PLUGIN,
        &crate::sign::PLUGIN,
        &crate::guard::PLUGIN,
    ];

    /// Ordered, read-only view of every plugin compiled into this build.
    ///
    /// Order is declaration order, so reports are byte-stable across runs
    /// (directive section 12).
    #[derive(Clone, Copy)]
    pub struct Registry {
        plugins: &'static [&'static dyn Plugin],
    }

    impl Registry {
        /// Every plugin listed in directive section 6.
        pub fn builtin() -> Self {
            Registry { plugins: BUILTIN }
        }

        /// Number of registered plugins.
        pub fn len(&self) -> usize {
            self.plugins.len()
        }

        /// Whether the registry is empty.
        pub fn is_empty(&self) -> bool {
            self.plugins.is_empty()
        }

        /// Every registered plugin, in declaration order.
        pub fn all(&self) -> &'static [&'static dyn Plugin] {
            self.plugins
        }

        /// Looks a plugin up by its contract identifier.
        pub fn find(&self, id: &str) -> Option<&'static dyn Plugin> {
            self.plugins.iter().copied().find(|p| p.contract().id == id)
        }

        /// Serialises every contract as the array member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_array(Some(key));
            for p in self.plugins {
                p.contract().write_json(w);
            }
            w.end_array();
        }
    }
}

// ===========================================================================
// toolchain — the version lock and its verification (sections 14, 31, 15)
// ===========================================================================

/// Encoding and verification of the pinned toolchain.
///
/// Directive section 14 forbids `latest`, `9.+`, `*` and any other dynamic
/// version. Every component below is therefore pinned literally, and section 31
/// requires each one to carry its source and, where known, a checksum.
///
/// This module never *installs* or *invokes* a tool. It compares what the
/// directive demands against what the host reports observing, and says plainly
/// where the two disagree. On an Android device most of these components are not
/// observable at all, which the report states rather than guesses.
pub mod toolchain {
    use crate::diag::{Diagnostic, Severity, Sink};
    use crate::json::Writer;
    use crate::FailureClass;

    /// How strictly an observed version must match the pin.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Requirement {
        /// The observed version must equal the pin exactly.
        Exact,
        /// The observed version must share the pin's leading component.
        ///
        /// Used only where the directive itself pins a series, such as
        /// "CMake 4.x stable".
        Series,
    }

    impl Requirement {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Requirement::Exact => "EXACT",
                Requirement::Series => "SERIES",
            }
        }

        /// Evaluates `observed` against `pinned`.
        pub fn satisfied_by(self, pinned: &str, observed: &str) -> bool {
            let pinned = pinned.trim();
            let observed = observed.trim();
            match self {
                Requirement::Exact => pinned == observed,
                Requirement::Series => {
                    let head = |v: &str| v.split('.').next().unwrap_or("").to_string();
                    !observed.is_empty() && head(pinned) == head(observed)
                }
            }
        }
    }

    /// One pinned component of the toolchain.
    #[derive(Clone, Copy, Debug)]
    pub struct Pin {
        /// Key the host uses when reporting an observation.
        pub id: &'static str,
        /// Human-facing name.
        pub display_name: &'static str,
        /// The pinned version, verbatim from directive section 14.
        pub pinned: &'static str,
        /// Match strictness.
        pub requirement: Requirement,
        /// Where the component comes from (directive section 31 provenance).
        pub source: &'static str,
        /// Checksum of the distribution, when one has been verified.
        pub checksum: Option<&'static str>,
        /// Whether an Android device can observe this component at runtime.
        ///
        /// Build-host tools such as Gradle and the JDK cannot be observed from
        /// inside the builder application, and the report says so instead of
        /// inventing a value.
        pub observable_on_device: bool,
        /// Why this component is pinned, or what is still missing.
        pub note: &'static str,
    }

    /// The toolchain lock of directive section 14, encoded verbatim.
    ///
    /// Every entry was checked against its upstream index on 2026-08-19 and
    /// exists. Checksums are recorded only where the artifact was actually
    /// fetched and hashed; `None` means "not yet verified", never "trusted".
    pub const LOCK: &[Pin] = &[
        Pin {
            id: "jdk",
            display_name: "JDK",
            pinned: "25",
            requirement: Requirement::Series,
            source: "Eclipse Temurin",
            checksum: None,
            observable_on_device: false,
            note: "Hosts Gradle and the Kotlin compiler. Build host only.",
        },
        Pin {
            id: "gradle",
            display_name: "Gradle",
            pinned: "9.7.0",
            requirement: Requirement::Exact,
            source: "https://services.gradle.org/distributions/gradle-9.7.0-bin.zip",
            checksum: Some("84fbba45c7f4c64abc77460e1c00f541e9f960e3c7ed2538f1ede19eacd873ae"),
            observable_on_device: false,
            note: "Bootstrap build driver. SHA-256 published by Gradle and pinned \
                   in Gradle/gradle-wrapper.properties.",
        },
        Pin {
            id: "agp",
            display_name: "Android Gradle Plugin",
            pinned: "9.3.0",
            requirement: Requirement::Exact,
            source: "https://dl.google.com/dl/android/maven2",
            checksum: None,
            observable_on_device: false,
            note: "Bootstrap packaging and signing. Resolved and verified by Gradle \
                   dependency verification, which is not yet enabled.",
        },
        Pin {
            id: "kotlin",
            display_name: "Kotlin",
            pinned: "2.4.10",
            requirement: Requirement::Exact,
            source: "https://repo.maven.apache.org/maven2",
            checksum: None,
            observable_on_device: false,
            note: "Compiles the builder user interface. AGP 9.3.0 would otherwise \
                   supply its own Kotlin (2.2.10) and exposes no DSL to change \
                   that, so the version is pinned where Gradle does have \
                   authority: a resolution rule forces every org.jetbrains.kotlin \
                   module to this version, and the Kotlin Build Tools API runs \
                   the pinned compiler. The `verifyKotlinToolchain` Gradle task \
                   proves the result rather than assuming it.",
        },
        Pin {
            id: "rust",
            display_name: "Rust",
            pinned: "1.97.1",
            requirement: Requirement::Exact,
            source: "https://static.rust-lang.org (rustup channel: stable)",
            checksum: None,
            observable_on_device: false,
            note: "Compiles the Core. Enforced by `rust-version` in Cargo.toml, \
                   which cargo refuses to build below, and installed at this exact \
                   version by the build workflow.",
        },
        Pin {
            id: "ndk",
            display_name: "Android NDK",
            pinned: "29.0.14206865",
            requirement: Requirement::Exact,
            source: "Android SDK package ndk;29.0.14206865",
            checksum: None,
            observable_on_device: false,
            note: "Provides the Clang toolchain that links the Core into \
                   libomni_builder.so.",
        },
        Pin {
            id: "androidApi",
            display_name: "Android API (compileSdk)",
            pinned: "36",
            requirement: Requirement::Exact,
            source: "Android SDK package platforms;android-36",
            checksum: None,
            observable_on_device: false,
            note: "Compile-time platform. Distinct from the device API level.",
        },
        Pin {
            id: "buildTools",
            display_name: "Android Build Tools",
            pinned: "36.0.0",
            requirement: Requirement::Exact,
            source: "Android SDK package build-tools;36.0.0",
            checksum: None,
            observable_on_device: false,
            note: "Supplies aapt2, d8 and zipalign for the bootstrap build.",
        },
        Pin {
            id: "cmake",
            display_name: "CMake",
            pinned: "4.4.2",
            requirement: Requirement::Exact,
            source: "https://github.com/Kitware/CMake/releases/download/v4.4.2/\
                     cmake-4.4.2-linux-x86_64.tar.gz",
            checksum: Some("3ada9a3f5d8a85413579bdd0ea6aa8e8da86efdd6d15c91a1afa517f2021956c"),
            observable_on_device: false,
            note: "Provisioned from upstream Kitware, not from the Android SDK: \
                   sdkmanager publishes CMake only up to 4.1.2. The Kitware archive \
                   ships no Ninja, so the generator is taken from the SDK's own \
                   CMake package. Pointed at through `cmake.dir` in \
                   local.properties and checked by the `verifyCmakeToolchain` \
                   Gradle task, which refuses to build against any other version.",
        },
        Pin {
            id: "minSdk",
            display_name: "minSdk",
            pinned: "28",
            requirement: Requirement::Exact,
            source: "Omni_Builder build configuration",
            checksum: None,
            observable_on_device: true,
            note: "Lowest Android release the builder itself runs on.",
        },
        Pin {
            id: "targetSdk",
            display_name: "targetSdk",
            pinned: "36",
            requirement: Requirement::Exact,
            source: "Omni_Builder build configuration",
            checksum: None,
            observable_on_device: true,
            note: "Behavioural contract the builder opts into.",
        },
    ];

    /// Result of comparing one pin against the environment.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum State {
        /// Observed and equal to the pin.
        Match,
        /// Observed and different from the pin.
        Mismatch,
        /// Not observed, and not observable from this host.
        NotObservable,
        /// Observable in principle, but the host did not report it.
        Missing,
    }

    impl State {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                State::Match => "MATCH",
                State::Mismatch => "MISMATCH",
                State::NotObservable => "NOT_OBSERVABLE",
                State::Missing => "MISSING",
            }
        }
    }

    /// One line of the verification report.
    #[derive(Clone, Debug)]
    pub struct Finding {
        /// The pin that was evaluated.
        pub pin: Pin,
        /// What the host reported, if anything.
        pub observed: Option<String>,
        /// Outcome of the comparison.
        pub state: State,
    }

    /// Upper bound on the observation string, in bytes (directive section 60).
    pub const MAX_OBSERVATION_BYTES: usize = 8 * 1024;

    /// Upper bound on the number of `key=value` pairs (directive section 60).
    pub const MAX_OBSERVATION_PAIRS: usize = 64;

    /// Upper bound on a single observed value, in bytes.
    pub const MAX_OBSERVED_VALUE_BYTES: usize = 256;

    /// A bounded set of `key=value` observations supplied by the host.
    ///
    /// The input is untrusted (directive section 61): it arrives from the JNI
    /// boundary. Parsing is therefore bounded in length, in pair count and in
    /// value size, and anything the parser rejects becomes a diagnostic rather
    /// than being silently dropped (directive section 44).
    #[derive(Clone, Debug, Default)]
    pub struct Observation {
        pairs: Vec<(String, String)>,
    }

    impl Observation {
        /// Parses `input`, recording every rejection in `sink`.
        ///
        /// Accepted form: `key=value` separated by `;` or a newline. Surrounding
        /// whitespace is trimmed. Later duplicates of a key are rejected rather
        /// than silently overriding the earlier value.
        pub fn parse(input: &str, sink: &mut Sink) -> Observation {
            let mut observation = Observation::default();

            if input.len() > MAX_OBSERVATION_BYTES {
                sink.emit(
                    Diagnostic::new(
                        "E1101",
                        Severity::Error,
                        FailureClass::ResourceExhaustion,
                        "core.toolchain",
                        "Environment observation exceeds the accepted size.",
                    )
                    .with_context(format!("Limit: {} bytes", MAX_OBSERVATION_BYTES))
                    .with_context(format!("Received: {} bytes", input.len()))
                    .with_suggestion("Report only the keys listed in the toolchain lock."),
                );
                return observation;
            }

            for raw in input.split([';', '\n']) {
                let entry = raw.trim();
                if entry.is_empty() {
                    continue;
                }

                if observation.pairs.len() >= MAX_OBSERVATION_PAIRS {
                    sink.emit(
                        Diagnostic::new(
                            "E1102",
                            Severity::Error,
                            FailureClass::ResourceExhaustion,
                            "core.toolchain",
                            "Environment observation contains too many entries.",
                        )
                        .with_context(format!("Limit: {} entries", MAX_OBSERVATION_PAIRS))
                        .with_suggestion("Report only the keys listed in the toolchain lock."),
                    );
                    break;
                }

                let Some((key, value)) = entry.split_once('=') else {
                    sink.emit(
                        Diagnostic::new(
                            "W1103",
                            Severity::Warning,
                            FailureClass::ConfigurationError,
                            "core.toolchain",
                            "Environment observation entry is not a key=value pair.",
                        )
                        .with_context(format!("Entry: {}", truncate(entry, 64)))
                        .with_suggestion("Use the form key=value, separated by ';'."),
                    );
                    continue;
                };

                let key = key.trim();
                let value = value.trim();

                if key.is_empty() {
                    sink.emit(
                        Diagnostic::new(
                            "W1104",
                            Severity::Warning,
                            FailureClass::ConfigurationError,
                            "core.toolchain",
                            "Environment observation entry has an empty key.",
                        )
                        .with_suggestion("Use the form key=value, separated by ';'."),
                    );
                    continue;
                }

                if value.len() > MAX_OBSERVED_VALUE_BYTES {
                    sink.emit(
                        Diagnostic::new(
                            "E1105",
                            Severity::Error,
                            FailureClass::ResourceExhaustion,
                            "core.toolchain",
                            "Observed value exceeds the accepted size.",
                        )
                        .with_context(format!("Key: {}", truncate(key, 64)))
                        .with_context(format!("Limit: {} bytes", MAX_OBSERVED_VALUE_BYTES))
                        .with_suggestion("Report a version string, not a full tool banner."),
                    );
                    continue;
                }

                if observation.pairs.iter().any(|(k, _)| k == key) {
                    sink.emit(
                        Diagnostic::new(
                            "W1106",
                            Severity::Warning,
                            FailureClass::ConfigurationError,
                            "core.toolchain",
                            "Environment observation repeats a key.",
                        )
                        .with_context(format!("Key: {}", truncate(key, 64)))
                        .with_suggestion(
                            "Report each key once. The first value was kept and this \
                             entry was ignored.",
                        ),
                    );
                    continue;
                }

                if !LOCK.iter().any(|p| p.id == key) {
                    sink.emit(
                        Diagnostic::new(
                            "W1107",
                            Severity::Warning,
                            FailureClass::ConfigurationError,
                            "core.toolchain",
                            "Environment observation contains an unrecognised key.",
                        )
                        .with_context(format!("Key: {}", truncate(key, 64)))
                        .with_suggestion(
                            "Unknown keys are reported rather than ignored. Remove it \
                             or add a matching pin to the toolchain lock.",
                        ),
                    );
                }

                observation.pairs.push((key.to_string(), value.to_string()));
            }

            observation
        }

        /// Looks up an observed value.
        pub fn get(&self, key: &str) -> Option<&str> {
            self.pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        }

        /// Number of accepted observations.
        pub fn len(&self) -> usize {
            self.pairs.len()
        }

        /// Whether nothing was observed.
        pub fn is_empty(&self) -> bool {
            self.pairs.is_empty()
        }
    }

    fn truncate(value: &str, max: usize) -> String {
        if value.chars().count() <= max {
            return value.to_string();
        }
        let mut out: String = value.chars().take(max).collect();
        out.push('…');
        out
    }

    /// Compares every pin against `observation`, emitting diagnostics as it goes.
    ///
    /// A mismatch is an error: directive section 14 does not permit a build to
    /// proceed on an unpinned toolchain. A component that cannot be observed is
    /// reported as such and is never counted as verified.
    pub fn verify(observation: &Observation, sink: &mut Sink) -> Vec<Finding> {
        let mut findings = Vec::with_capacity(LOCK.len());

        for pin in LOCK {
            let observed = observation.get(pin.id).map(str::to_string);
            let state = match observed.as_deref() {
                Some(value) if pin.requirement.satisfied_by(pin.pinned, value) => State::Match,
                Some(_) => State::Mismatch,
                None if pin.observable_on_device => State::Missing,
                None => State::NotObservable,
            };

            match state {
                State::Mismatch => {
                    let value = observed.clone().unwrap_or_default();
                    sink.emit(
                        Diagnostic::new(
                            "E1001",
                            Severity::Error,
                            FailureClass::ToolchainError,
                            "core.toolchain",
                            format!("{} does not match the pinned version.", pin.display_name),
                        )
                        .with_context(format!(
                            "Expected: {} ({})",
                            pin.pinned,
                            pin.requirement.as_str()
                        ))
                        .with_context(format!("Found: {}", truncate(&value, 64)))
                        .with_context(format!("Source: {}", pin.source))
                        .with_suggestion(format!(
                            "Install {} {} or change the pin in the toolchain lock \
                             together with an architectural decision record.",
                            pin.display_name, pin.pinned
                        )),
                    );
                }
                State::Missing => {
                    sink.emit(
                        Diagnostic::new(
                            "E1002",
                            Severity::Error,
                            FailureClass::ToolchainError,
                            "core.toolchain",
                            format!("{} was not reported by the host.", pin.display_name),
                        )
                        .with_context(format!("Expected: {}", pin.pinned))
                        .with_suggestion(format!(
                            "The host is able to observe '{}' and must report it.",
                            pin.id
                        )),
                    );
                }
                State::NotObservable => {
                    sink.emit(
                        Diagnostic::new(
                            "W1003",
                            Severity::Warning,
                            FailureClass::ToolchainError,
                            "core.toolchain",
                            format!("{} cannot be verified from this host.", pin.display_name),
                        )
                        .with_context(format!("Pinned: {}", pin.pinned))
                        .with_context(format!("Source: {}", pin.source))
                        .with_suggestion(
                            "This is a build-host component. It is verified where the \
                             build actually runs, not on the device.",
                        ),
                    );
                }
                State::Match => {}
            }

            findings.push(Finding {
                pin: *pin,
                observed,
                state,
            });
        }

        findings
    }

    /// Serialises the findings as the array member `key`.
    pub fn write_json(findings: &[Finding], w: &mut Writer, key: &str) {
        w.begin_array(Some(key));
        for finding in findings {
            w.begin_object(None);
            w.field_str("id", finding.pin.id);
            w.field_str("displayName", finding.pin.display_name);
            w.field_str("pinned", finding.pin.pinned);
            w.field_str("requirement", finding.pin.requirement.as_str());
            w.field_str("source", finding.pin.source);
            match finding.pin.checksum {
                Some(sum) => w.field_str("checksum", sum),
                None => w.field_bool("checksumPinned", false),
            }
            w.field_bool("observableOnDevice", finding.pin.observable_on_device);
            w.field_str("note", finding.pin.note);
            if let Some(value) = &finding.observed {
                w.field_str("observed", value);
            }
            w.field_str("state", finding.state.as_str());
            w.end_object();
        }
        w.end_array();
    }
}

// ===========================================================================
// hash — SHA-256 (directive section 30)
// ===========================================================================

/// SHA-256, implemented from FIPS PUB 180-4.
///
/// Directive section 30 forbids inventing cryptography and requires every
/// primitive to rest on an established standard, its official specification and
/// its official test vectors. This is an implementation of a published standard,
/// not a new algorithm, and the tests are the vectors NIST publishes for it.
///
/// It exists because artifact digests (section 58), cache keys (section 11) and
/// build provenance (section 32) all need one hash, and ADR-0003 keeps the Core
/// free of third-party code.
///
/// It is **not** suitable for password hashing or for any use needing a keyed
/// or memory-hard construction; nothing in this tree asks it to be.
pub mod hash {
    /// A 32-byte SHA-256 digest.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Digest([u8; 32]);

    impl Digest {
        /// The raw bytes, most significant first.
        pub const fn as_bytes(&self) -> &[u8; 32] {
            &self.0
        }

        /// Lowercase hexadecimal, the form every report and log uses.
        pub fn to_hex(self) -> String {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut out = String::with_capacity(64);
            for byte in self.0 {
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
            out
        }

        /// The first `bytes` bytes as hexadecimal.
        ///
        /// Used where a digest identifies something to a human rather than
        /// authenticating it; never use a truncated digest for verification.
        pub fn to_short_hex(self, bytes: usize) -> String {
            let width = bytes.min(32) * 2;
            self.to_hex()[..width].to_string()
        }
    }

    impl core::fmt::Debug for Digest {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "sha256:{}", self.to_hex())
        }
    }

    impl core::fmt::Display for Digest {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.to_hex())
        }
    }

    /// Round constants: the first 32 bits of the fractional parts of the cube
    /// roots of the first 64 primes (FIPS 180-4, section 4.2.2).
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    /// Initial hash value: the first 32 bits of the fractional parts of the
    /// square roots of the first 8 primes (FIPS 180-4, section 5.3.3).
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    /// Streaming SHA-256 state.
    ///
    /// Streaming matters here: a build hashes files that must never be required
    /// to fit in memory all at once (directive section 37).
    #[derive(Clone)]
    pub struct Sha256 {
        state: [u32; 8],
        block: [u8; 64],
        buffered: usize,
        /// Message length in bits, as the padding scheme requires.
        length_bits: u64,
    }

    impl Default for Sha256 {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Sha256 {
        /// A fresh hasher.
        pub const fn new() -> Self {
            Sha256 {
                state: H0,
                block: [0u8; 64],
                buffered: 0,
                length_bits: 0,
            }
        }

        /// Absorbs more of the message.
        pub fn update(&mut self, mut data: &[u8]) {
            self.length_bits = self.length_bits.wrapping_add((data.len() as u64) * 8);

            if self.buffered > 0 {
                let want = 64 - self.buffered;
                let take = want.min(data.len());
                self.block[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
                self.buffered += take;
                data = &data[take..];
                if self.buffered == 64 {
                    let block = self.block;
                    self.compress(&block);
                    self.buffered = 0;
                }
            }

            while data.len() >= 64 {
                let (block, rest) = data.split_at(64);
                let mut fixed = [0u8; 64];
                fixed.copy_from_slice(block);
                self.compress(&fixed);
                data = rest;
            }

            if !data.is_empty() {
                self.block[..data.len()].copy_from_slice(data);
                self.buffered = data.len();
            }
        }

        /// Applies the padding of FIPS 180-4 section 5.1.1 and returns the digest.
        pub fn finish(mut self) -> Digest {
            let length_bits = self.length_bits;

            // A single 1 bit, then zeroes, then the 64-bit length.
            self.append_padding_byte(0x80);
            while self.buffered != 56 {
                self.append_padding_byte(0x00);
            }
            let encoded = length_bits.to_be_bytes();
            for byte in encoded {
                self.append_padding_byte(byte);
            }
            debug_assert_eq!(self.buffered, 0);

            let mut out = [0u8; 32];
            for (index, word) in self.state.iter().enumerate() {
                out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
            }
            Digest(out)
        }

        /// Appends one byte without touching the recorded message length.
        fn append_padding_byte(&mut self, byte: u8) {
            self.block[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.block;
                self.compress(&block);
                self.buffered = 0;
            }
        }

        /// The compression function of FIPS 180-4, section 6.2.2.
        fn compress(&mut self, block: &[u8; 64]) {
            let mut w = [0u32; 64];
            for (index, chunk) in block.chunks_exact(4).enumerate() {
                w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            for index in 16..64 {
                let s0 = w[index - 15].rotate_right(7)
                    ^ w[index - 15].rotate_right(18)
                    ^ (w[index - 15] >> 3);
                let s1 = w[index - 2].rotate_right(17)
                    ^ w[index - 2].rotate_right(19)
                    ^ (w[index - 2] >> 10);
                w[index] = w[index - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[index - 7])
                    .wrapping_add(s1);
            }

            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

            for index in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = h
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[index])
                    .wrapping_add(w[index]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
                *slot = slot.wrapping_add(value);
            }
        }
    }

    /// Hashes a byte slice in one call.
    pub fn sha256(data: &[u8]) -> Digest {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finish()
    }

    /// Hashes a sequence of labelled fields unambiguously.
    ///
    /// Concatenating values before hashing is a classic mistake: `("ab", "c")`
    /// and `("a", "bc")` would collide. Each field is therefore prefixed with its
    /// name and both lengths, so no two different field sets can produce the same
    /// input. Cache keys (directive section 11) depend on this being true.
    pub fn sha256_fields(fields: &[(&str, &[u8])]) -> Digest {
        let mut hasher = Sha256::new();
        hasher.update(&(fields.len() as u64).to_be_bytes());
        for (name, value) in fields {
            hasher.update(&(name.len() as u64).to_be_bytes());
            hasher.update(name.as_bytes());
            hasher.update(&(value.len() as u64).to_be_bytes());
            hasher.update(value);
        }
        hasher.finish()
    }
}

// ===========================================================================
// vfs — the virtual filesystem (directive section 8)
// ===========================================================================

/// The only way anything in a build reaches a file.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                       |
/// |----------------------|-------------------------------------------------------------|
/// | Module               | `omni_core::vfs`                                            |
/// | Purpose              | Give plugins file access that is named, bounded and audited. |
/// | Non-Responsibilities | Deciding *what* to read or write; that is the plugin's job.  |
/// | Security             | Every operation needs a capability grant. Paths cannot escape |
/// |                      | their mount, before or after symlink resolution.             |
/// | Failure Modes        | Rejected path, denied capability, unknown mount, read-only    |
/// |                      | mount, quota exhausted, underlying I/O error.                 |
/// | Determinism          | Reading the same bytes yields the same digest; writes are     |
/// |                      | atomic, so a build never observes a half-written file.        |
/// | Status               | PARTIAL — snapshot and rollback are not implemented.          |
///
/// Directive section 8 lists what a virtual filesystem must eventually do. What
/// is implemented here is path normalisation, traversal protection, read and
/// write policy, quotas, temporary files and atomic writes. Locking, snapshots
/// and rollback are **not** implemented, and no code in this tree pretends they
/// are.
pub mod vfs {
    use crate::caps::Capability;
    use crate::diag::{Diagnostic, Severity};
    use crate::hash::{sha256, Digest};
    use crate::plugin::Context;
    use crate::FailureClass;
    use std::fs;
    use std::io::Write;
    use std::path::{Component, Path, PathBuf};

    /// Longest accepted path, in bytes (directive section 60).
    pub const MAX_PATH_BYTES: usize = 4096;

    /// Deepest accepted path. Guards against path explosion.
    pub const MAX_SEGMENTS: usize = 64;

    /// Longest accepted single segment, in bytes. Matches the common
    /// filesystem limit.
    pub const MAX_SEGMENT_BYTES: usize = 255;

    /// A path that has been proven safe.
    ///
    /// The only way to build one is [`VirtualPath::parse`], so a value of this
    /// type is evidence that the path is relative, free of `..`, free of control
    /// characters and within every bound. Functions downstream can rely on that
    /// rather than re-checking, which is the point: a check that has to be
    /// repeated is a check that will eventually be forgotten.
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct VirtualPath {
        segments: Vec<String>,
    }

    impl VirtualPath {
        /// Normalises and validates a path expressed inside a mount.
        ///
        /// Accepts `/`-separated relative paths. `.` segments are dropped. Every
        /// other rejection is deliberate and reported.
        pub fn parse(input: &str) -> Result<VirtualPath, Diagnostic> {
            fn reject(code: &str, message: &str, suggestion: &str) -> Diagnostic {
                Diagnostic::new(
                    code,
                    Severity::Error,
                    FailureClass::SecurityFailure,
                    "core.vfs",
                    message,
                )
                .with_suggestion(suggestion)
            }

            if input.is_empty() {
                return Err(reject(
                    "E2001",
                    "The path is empty.",
                    "Give a path relative to the mount, such as 'Source/Main/Builder.kt'.",
                ));
            }

            if input.len() > MAX_PATH_BYTES {
                return Err(reject(
                    "E2002",
                    "The path is longer than the accepted limit.",
                    "Paths are limited to 4096 bytes.",
                )
                .with_context(format!("Length: {} bytes", input.len())));
            }

            if let Some(bad) = input.chars().find(|c| (*c as u32) < 0x20 || *c == '\u{7f}') {
                return Err(reject(
                    "E2003",
                    "The path contains a control character.",
                    "Remove it. A control character in a path is either a mistake \
                     or an attempt to confuse something downstream.",
                )
                .with_context(format!("Character: U+{:04X}", bad as u32)));
            }

            if input.contains('\\') {
                return Err(reject(
                    "E2004",
                    "The path contains a backslash.",
                    "Use '/' as the separator. Accepting both would mean two \
                     spellings of one path, and two spellings are two chances to \
                     get a security check wrong.",
                ));
            }

            if input.starts_with('/') {
                return Err(reject(
                    "E2005",
                    "The path is absolute.",
                    "Give a path relative to a mount. Absolute paths would let a \
                     plugin name a file the build never granted it.",
                ));
            }

            // A Windows drive specifier is absolute on the platform that
            // understands it, so it is refused here even though this Core does
            // not run there.
            if input.len() >= 2 && input.as_bytes()[1] == b':' {
                return Err(reject(
                    "E2005",
                    "The path names a drive.",
                    "Give a path relative to a mount.",
                ));
            }

            let mut segments = Vec::new();
            for raw in input.split('/') {
                match raw {
                    "" => {
                        return Err(reject(
                            "E2006",
                            "The path contains an empty segment.",
                            "Remove the repeated or trailing '/'.",
                        ));
                    }
                    "." => continue,
                    ".." => {
                        return Err(reject(
                            "E2007",
                            "The path tries to leave its mount.",
                            "Remove the '..' segment. A build never needs to reach \
                             outside the directories it was given.",
                        ));
                    }
                    segment => {
                        if segment.len() > MAX_SEGMENT_BYTES {
                            return Err(reject(
                                "E2008",
                                "A path segment is longer than the accepted limit.",
                                "Segments are limited to 255 bytes.",
                            ));
                        }
                        segments.push(segment.to_string());
                    }
                }
            }

            if segments.is_empty() {
                return Err(reject(
                    "E2001",
                    "The path names no file.",
                    "A path of only '.' segments refers to the mount itself.",
                ));
            }

            if segments.len() > MAX_SEGMENTS {
                return Err(reject(
                    "E2009",
                    "The path is nested more deeply than the accepted limit.",
                    "Paths are limited to 64 segments.",
                )
                .with_context(format!("Segments: {}", segments.len())));
            }

            Ok(VirtualPath { segments })
        }

        /// The normalised path, always using `/`.
        pub fn as_str(&self) -> String {
            self.segments.join("/")
        }

        /// The individual segments, in order.
        pub fn segments(&self) -> &[String] {
            &self.segments
        }

        /// The final segment.
        pub fn file_name(&self) -> &str {
            self.segments.last().map(String::as_str).unwrap_or_default()
        }

        /// The extension of the final segment, without the dot.
        pub fn extension(&self) -> Option<&str> {
            let name = self.file_name();
            name.rsplit_once('.')
                .map(|(_, ext)| ext)
                .filter(|e| !e.is_empty())
        }
    }

    impl core::fmt::Display for VirtualPath {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.as_str())
        }
    }

    /// What a mount permits.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Access {
        /// Reads only. A write is refused even with `FS_WRITE` granted.
        ReadOnly,
        /// Reads and writes.
        ReadWrite,
    }

    impl Access {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Access::ReadOnly => "READ_ONLY",
                Access::ReadWrite => "READ_WRITE",
            }
        }
    }

    /// A named directory a build may reach into.
    #[derive(Clone, Debug)]
    pub struct Mount {
        name: String,
        root: PathBuf,
        access: Access,
    }

    impl Mount {
        /// Name plugins use to address this mount.
        pub fn name(&self) -> &str {
            &self.name
        }

        /// What it permits.
        pub fn access(&self) -> Access {
            self.access
        }
    }

    /// Byte budgets for one build (directive section 60).
    #[derive(Clone, Copy, Debug)]
    pub struct Quota {
        /// Largest single file that may be read or written.
        pub max_file_bytes: u64,
        /// Total that may be written across the whole build.
        pub max_written_bytes: u64,
    }

    impl Default for Quota {
        /// Deliberately modest. A mobile device is the target, not a build farm
        /// (directive section 36).
        fn default() -> Self {
            Quota {
                max_file_bytes: 64 * 1024 * 1024,
                max_written_bytes: 512 * 1024 * 1024,
            }
        }
    }

    /// Counters worth reporting after a build.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Usage {
        /// Files read.
        pub reads: u64,
        /// Files written.
        pub writes: u64,
        /// Bytes read.
        pub bytes_read: u64,
        /// Bytes written.
        pub bytes_written: u64,
        /// Operations refused, for any reason.
        pub refusals: u64,
    }

    /// The filesystem a build sees.
    #[derive(Debug)]
    pub struct VirtualFs {
        mounts: Vec<Mount>,
        quota: Quota,
        usage: Usage,
    }

    impl VirtualFs {
        /// An empty filesystem. Nothing is reachable until something is mounted.
        pub fn new(quota: Quota) -> Self {
            VirtualFs {
                mounts: Vec::new(),
                quota,
                usage: Usage::default(),
            }
        }

        /// Makes a real directory reachable under a name.
        ///
        /// The root is canonicalised now, so every later containment check
        /// compares against a path with no symlinks left in it.
        pub fn mount(
            &mut self,
            name: impl Into<String>,
            root: impl AsRef<Path>,
            access: Access,
        ) -> Result<(), Diagnostic> {
            let name = name.into();
            let root = root.as_ref();

            if name.is_empty() || name.contains('/') {
                return Err(Diagnostic::new(
                    "E2010",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "core.vfs",
                    "A mount name must be a single non-empty word.",
                )
                .with_context(format!("Name: {name}"))
                .with_suggestion("Use a name such as 'project' or 'output'."));
            }

            if self.mounts.iter().any(|m| m.name == name) {
                return Err(Diagnostic::new(
                    "E2011",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "core.vfs",
                    "That mount name is already in use.",
                )
                .with_context(format!("Name: {name}"))
                .with_suggestion("Mount names are unique so a path means one thing."));
            }

            let canonical = fs::canonicalize(root).map_err(|error| {
                Diagnostic::new(
                    "E2012",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "core.vfs",
                    "The mount root could not be resolved.",
                )
                .with_context(format!("Root: {}", root.display()))
                .with_context(format!("Cause: {error}"))
                .with_suggestion("The directory must exist before it is mounted.")
            })?;

            if !canonical.is_dir() {
                return Err(Diagnostic::new(
                    "E2013",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "core.vfs",
                    "The mount root is not a directory.",
                )
                .with_context(format!("Root: {}", canonical.display())));
            }

            self.mounts.push(Mount {
                name,
                root: canonical,
                access,
            });
            Ok(())
        }

        /// Every mount, in the order they were added.
        pub fn mounts(&self) -> &[Mount] {
            &self.mounts
        }

        /// Counters accumulated so far.
        pub fn usage(&self) -> Usage {
            self.usage
        }

        /// The quota in force.
        pub fn quota(&self) -> Quota {
            self.quota
        }

        /// Reads a file.
        ///
        /// Requires `FS_READ`. The digest of the bytes is returned alongside them
        /// so that a caller never has to re-read a file to know what it hashed
        /// to (directive sections 11 and 58).
        pub fn read(
            &mut self,
            ctx: &mut Context<'_>,
            subject: &str,
            mount: &str,
            path: &VirtualPath,
        ) -> Result<(Vec<u8>, Digest), Diagnostic> {
            self.require(ctx, subject, Capability::FsRead)?;
            let resolved = self.resolve(mount, path, false)?;

            let metadata = fs::metadata(&resolved).map_err(|error| {
                self.usage.refusals += 1;
                Self::io_failure("E2020", "The file could not be opened.", path, error)
            })?;

            if metadata.len() > self.quota.max_file_bytes {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2021",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.vfs",
                    "The file is larger than this build is allowed to read.",
                )
                .with_context(format!("Path: {path}"))
                .with_context(format!("Size: {} bytes", metadata.len()))
                .with_context(format!("Limit: {} bytes", self.quota.max_file_bytes))
                .with_suggestion(
                    "Raise the quota deliberately if the file really is this large.",
                ));
            }

            let bytes = fs::read(&resolved).map_err(|error| {
                self.usage.refusals += 1;
                Self::io_failure("E2020", "The file could not be read.", path, error)
            })?;

            self.usage.reads += 1;
            self.usage.bytes_read += bytes.len() as u64;
            let digest = sha256(&bytes);
            Ok((bytes, digest))
        }

        /// Writes a file, atomically.
        ///
        /// Requires `FS_WRITE` and a read-write mount. The bytes go to a
        /// temporary file in the destination directory and are renamed into
        /// place, so a reader sees either the previous file or the complete new
        /// one, never a partial write (directive section 59).
        pub fn write_atomic(
            &mut self,
            ctx: &mut Context<'_>,
            subject: &str,
            mount: &str,
            path: &VirtualPath,
            bytes: &[u8],
        ) -> Result<Digest, Diagnostic> {
            self.require(ctx, subject, Capability::FsWrite)?;

            let size = bytes.len() as u64;
            if size > self.quota.max_file_bytes {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2022",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.vfs",
                    "The file is larger than this build is allowed to write.",
                )
                .with_context(format!("Path: {path}"))
                .with_context(format!("Size: {size} bytes"))
                .with_context(format!("Limit: {} bytes", self.quota.max_file_bytes)));
            }

            if self.usage.bytes_written + size > self.quota.max_written_bytes {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2023",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.vfs",
                    "This build has written as much as it is allowed to.",
                )
                .with_context(format!("Written: {} bytes", self.usage.bytes_written))
                .with_context(format!("Limit: {} bytes", self.quota.max_written_bytes))
                .with_suggestion(
                    "A build that writes this much is usually looping; check before \
                     raising the quota.",
                ));
            }

            let resolved = self.resolve(mount, path, true)?;

            if let Some(parent) = resolved.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    self.usage.refusals += 1;
                    Self::io_failure("E2024", "The directory could not be created.", path, error)
                })?;
            }

            // Named after the destination so a stray temporary file is always
            // traceable to the write that left it behind.
            let temporary = resolved.with_extension(format!(
                "{}omni-partial",
                resolved.extension().map(|_| ".").unwrap_or_default()
            ));

            let write_result = (|| -> std::io::Result<()> {
                let mut file = fs::File::create(&temporary)?;
                file.write_all(bytes)?;
                // Flush to the device before the rename, or a crash could leave
                // the name pointing at content that never reached storage.
                file.sync_all()?;
                fs::rename(&temporary, &resolved)
            })();

            if let Err(error) = write_result {
                let _ = fs::remove_file(&temporary);
                self.usage.refusals += 1;
                return Err(Self::io_failure(
                    "E2024",
                    "The file could not be written.",
                    path,
                    error,
                ));
            }

            self.usage.writes += 1;
            self.usage.bytes_written += size;
            Ok(sha256(bytes))
        }

        /// Whether a file exists. Requires `FS_READ`, because existence is
        /// information.
        pub fn exists(
            &mut self,
            ctx: &mut Context<'_>,
            subject: &str,
            mount: &str,
            path: &VirtualPath,
        ) -> Result<bool, Diagnostic> {
            self.require(ctx, subject, Capability::FsRead)?;
            Ok(self
                .resolve(mount, path, false)
                .map(|p| p.exists())
                .unwrap_or(false))
        }

        fn require(
            &mut self,
            ctx: &mut Context<'_>,
            subject: &str,
            capability: Capability,
        ) -> Result<(), Diagnostic> {
            if ctx.require(subject, capability).is_granted() {
                return Ok(());
            }
            self.usage.refusals += 1;
            Err(Diagnostic::new(
                "E2030",
                Severity::Error,
                FailureClass::SecurityFailure,
                "core.vfs",
                format!("{subject} does not hold {capability}."),
            )
            .with_context("The capability model denies by default (directive section 7).")
            .with_suggestion(format!(
                "Grant {capability} to this plugin explicitly if it genuinely needs it.",
            )))
        }

        /// Maps a virtual path onto a real one, refusing anything that escapes.
        ///
        /// [`VirtualPath`] already guarantees the path is relative and free of
        /// `..`, so this guards against the case that syntax cannot: a symlink
        /// inside the mount pointing out of it.
        fn resolve(
            &mut self,
            mount: &str,
            path: &VirtualPath,
            for_write: bool,
        ) -> Result<PathBuf, Diagnostic> {
            let Some(entry) = self.mounts.iter().find(|m| m.name == mount) else {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2031",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "core.vfs",
                    format!("There is no mount named '{mount}'."),
                )
                .with_context(format!(
                    "Mounted: {}",
                    self.mounts
                        .iter()
                        .map(|m| m.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .with_suggestion("Mount the directory before reading from it."));
            };

            if for_write && entry.access == Access::ReadOnly {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2032",
                    Severity::Error,
                    FailureClass::SecurityFailure,
                    "core.vfs",
                    format!("The mount '{mount}' is read-only."),
                )
                .with_context(format!("Path: {path}"))
                .with_suggestion(
                    "Write to an output mount. A source tree is mounted read-only \
                     on purpose.",
                ));
            }

            let root = entry.root.clone();
            let mut candidate = root.clone();
            for segment in path.segments() {
                candidate.push(segment);
            }

            // Canonicalise as much of the path as exists. For a write the file
            // itself may not exist yet, so the deepest existing ancestor is what
            // gets checked.
            let mut existing = candidate.as_path();
            let canonical = loop {
                match fs::canonicalize(existing) {
                    Ok(resolved) => break resolved,
                    Err(_) => match existing.parent() {
                        Some(parent) if parent.starts_with(&root) || parent == root => {
                            existing = parent;
                        }
                        _ => break root.clone(),
                    },
                }
            };

            if !canonical.starts_with(&root) {
                self.usage.refusals += 1;
                return Err(Diagnostic::new(
                    "E2033",
                    Severity::Error,
                    FailureClass::SecurityFailure,
                    "core.vfs",
                    "The path resolves outside its mount.",
                )
                .with_context(format!("Path: {path}"))
                .with_context(format!("Mount: {mount}"))
                .with_suggestion(
                    "A link inside the mount points outside it. The build refuses \
                     to follow it rather than reading a file it was never given.",
                ));
            }

            // Rebuild from the canonical root so the returned path contains no
            // component the check did not see.
            let mut safe = root;
            for segment in path.segments() {
                safe.push(segment);
            }
            debug_assert!(safe
                .components()
                .all(|c| !matches!(c, Component::ParentDir)));
            Ok(safe)
        }

        fn io_failure(
            code: &str,
            message: &str,
            path: &VirtualPath,
            error: std::io::Error,
        ) -> Diagnostic {
            Diagnostic::new(
                code,
                Severity::Error,
                FailureClass::Recoverable,
                "core.vfs",
                message,
            )
            .with_context(format!("Path: {path}"))
            .with_context(format!("Cause: {error}"))
        }
    }
}

// ===========================================================================
// project — the project model and its manifest (sections 13, 44, 45, 61)
// ===========================================================================

/// What a project is, and how `Omni.toml` is read.
///
/// ## Contract (directive section 2)
///
/// * **Purpose** — turn untrusted project input into a validated model, or into
///   diagnostics explaining exactly why it could not.
/// * **Inputs** — the text of `Omni.toml`. Untrusted (directive section 61).
/// * **Outputs** — a [`Project`], plus diagnostics.
/// * **Security** — the manifest cannot request a capability, name a path
///   outside the project, or cause anything to be executed. It is data.
/// * **Determinism** — the same text always produces the same model and the
///   same [`Project::digest`].
/// * **Status** — PARTIAL. The manifest of directive section 44 is fully
///   modelled; dependency declarations (section 62) are not, because nothing
///   resolves dependencies yet.
///
/// The syntax is a deliberately small subset: sections, `Key = value`, and three
/// value types. It is not TOML and does not claim to be. A full TOML parser is
/// a large attack surface for a file this simple, and directive section 61
/// requires project input to be validated before it is believed.
pub mod project {
    use crate::diag::{Diagnostic, Location, Severity, Sink};
    use crate::hash::{sha256_fields, Digest};
    use crate::json::Writer;
    use crate::FailureClass;

    /// Largest accepted manifest, in bytes (directive section 60).
    pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

    /// Largest accepted number of lines.
    pub const MAX_LINES: usize = 2_000;

    /// Largest accepted number of key/value entries.
    pub const MAX_ENTRIES: usize = 256;

    /// Name of the manifest file.
    pub const MANIFEST_NAME: &str = "Omni.toml";

    /// A build profile (directive section 13).
    ///
    /// Every profile is explicit. Directive section 13 forbids implicit
    /// configuration, so there is no "default" variant that quietly means
    /// something else somewhere.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Profile {
        /// Fast to build, easy to debug, not optimised.
        Debug,
        /// Optimised, intended for distribution.
        Release,
        /// Release plus the instrumentation a profiler needs.
        Profile,
        /// Built for measurement, with optimisation and no instrumentation.
        Benchmark,
        /// Release plus every verification the build can perform.
        Production,
        /// What continuous integration runs: deterministic and fully checked.
        Ci,
        /// Smallest possible output.
        Minimal,
        /// Fastest possible build, correctness checks kept.
        Fast,
        /// Every security gate on, whatever it costs.
        Secure,
    }

    impl Profile {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Profile::Debug => "Debug",
                Profile::Release => "Release",
                Profile::Profile => "Profile",
                Profile::Benchmark => "Benchmark",
                Profile::Production => "Production",
                Profile::Ci => "CI",
                Profile::Minimal => "Minimal",
                Profile::Fast => "Fast",
                Profile::Secure => "Secure",
            }
        }

        /// Every profile, in declaration order.
        pub const ALL: &'static [Profile] = &[
            Profile::Debug,
            Profile::Release,
            Profile::Profile,
            Profile::Benchmark,
            Profile::Production,
            Profile::Ci,
            Profile::Minimal,
            Profile::Fast,
            Profile::Secure,
        ];

        fn parse(value: &str) -> Option<Profile> {
            Profile::ALL.iter().copied().find(|p| p.as_str() == value)
        }
    }

    impl core::fmt::Display for Profile {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    /// What the build optimises for.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Optimization {
        /// No optimisation.
        None,
        /// Execution speed.
        Speed,
        /// Artifact size.
        Size,
        /// Neither at the other's expense.
        Balanced,
    }

    impl Optimization {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Optimization::None => "None",
                Optimization::Speed => "Speed",
                Optimization::Size => "Size",
                Optimization::Balanced => "Balanced",
            }
        }

        /// Every level, in declaration order.
        pub const ALL: &'static [Optimization] = &[
            Optimization::None,
            Optimization::Speed,
            Optimization::Size,
            Optimization::Balanced,
        ];

        fn parse(value: &str) -> Option<Optimization> {
            Optimization::ALL
                .iter()
                .copied()
                .find(|o| o.as_str() == value)
        }
    }

    /// How much Omni_Guard is asked to do (directive section 26).
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum GuardLevel {
        /// No integrity work.
        Off,
        /// Artifact digests only.
        Low,
        /// Digests and provenance.
        Medium,
        /// Everything the platform offers.
        High,
    }

    impl GuardLevel {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                GuardLevel::Off => "Off",
                GuardLevel::Low => "Low",
                GuardLevel::Medium => "Medium",
                GuardLevel::High => "High",
            }
        }

        /// Every level, in declaration order.
        pub const ALL: &'static [GuardLevel] = &[
            GuardLevel::Off,
            GuardLevel::Low,
            GuardLevel::Medium,
            GuardLevel::High,
        ];

        fn parse(value: &str) -> Option<GuardLevel> {
            GuardLevel::ALL
                .iter()
                .copied()
                .find(|g| g.as_str() == value)
        }
    }

    /// A validated project.
    ///
    /// Constructing one means every field has already been checked, so the rest
    /// of the build can use it without re-validating.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Project {
        /// Human-facing name.
        pub name: String,
        /// Android application identifier, in reverse-DNS form.
        pub id: String,
        /// Project version.
        pub version: String,
        /// Edition date, as written in the manifest.
        pub edition: Option<String>,

        /// Lowest Android release the application supports.
        pub min_sdk: u32,
        /// Behavioural contract the application opts into.
        pub target_sdk: u32,
        /// Platform the application is compiled against.
        pub compile_sdk: u32,

        /// Build profile.
        pub profile: Profile,
        /// What to optimise for.
        pub optimization: Optimization,
        /// Whether link-time optimisation is requested.
        pub lto: bool,
        /// Whether the build may reuse previous results.
        pub incremental: bool,
        /// Whether independent work may run concurrently.
        pub parallel: bool,
        /// Whether the build must be reproducible (directive section 12).
        pub deterministic: bool,

        /// How much integrity work Omni_Guard performs.
        pub guard: GuardLevel,
        /// Whether provenance is recorded (directive section 32).
        pub provenance: bool,
        /// Whether artifacts are verified before publication (section 58).
        pub verification: bool,

        /// Feature switches, sorted by name so the model is order-independent.
        pub features: Vec<(String, bool)>,
    }

    impl Project {
        /// The safe defaults of directive section 45.
        ///
        /// A project with no manifest still has to build, and every value below
        /// is a decision rather than an accident: the SDK levels match the
        /// toolchain lock, the profile is the one that cannot silently ship
        /// unoptimised code, and every security switch starts on.
        pub fn defaults(name: impl Into<String>, id: impl Into<String>) -> Project {
            Project {
                name: name.into(),
                id: id.into(),
                version: "0.1.0".to_string(),
                edition: None,
                min_sdk: 28,
                target_sdk: 36,
                compile_sdk: 36,
                profile: Profile::Debug,
                optimization: Optimization::None,
                lto: false,
                incremental: true,
                parallel: true,
                deterministic: true,
                guard: GuardLevel::Medium,
                provenance: true,
                verification: true,
                features: Vec::new(),
            }
        }

        /// Whether a feature is switched on.
        pub fn feature(&self, name: &str) -> bool {
            self.features
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
                .unwrap_or(false)
        }

        /// A digest of everything that can change what the build produces.
        ///
        /// This is the configuration component of a cache key (directive section
        /// 11) and of build provenance (section 32). The name is deliberately
        /// absent: renaming a project does not change its output.
        pub fn digest(&self) -> Digest {
            let numbers = format!("{}|{}|{}", self.min_sdk, self.target_sdk, self.compile_sdk);
            let switches = format!(
                "{}|{}|{}|{}|{}|{}|{}",
                self.lto,
                self.incremental,
                self.parallel,
                self.deterministic,
                self.provenance,
                self.verification,
                self.guard.as_str(),
            );
            let features = self
                .features
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(",");

            sha256_fields(&[
                ("id", self.id.as_bytes()),
                ("version", self.version.as_bytes()),
                ("sdk", numbers.as_bytes()),
                ("profile", self.profile.as_str().as_bytes()),
                ("optimization", self.optimization.as_str().as_bytes()),
                ("switches", switches.as_bytes()),
                ("features", features.as_bytes()),
            ])
        }

        /// Serialises the project as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_str("name", &self.name);
            w.field_str("id", &self.id);
            w.field_str("version", &self.version);
            if let Some(edition) = &self.edition {
                w.field_str("edition", edition);
            }
            w.field_u64("minSdk", self.min_sdk as u64);
            w.field_u64("targetSdk", self.target_sdk as u64);
            w.field_u64("compileSdk", self.compile_sdk as u64);
            w.field_str("profile", self.profile.as_str());
            w.field_str("optimization", self.optimization.as_str());
            w.field_bool("lto", self.lto);
            w.field_bool("incremental", self.incremental);
            w.field_bool("parallel", self.parallel);
            w.field_bool("deterministic", self.deterministic);
            w.field_str("guard", self.guard.as_str());
            w.field_bool("provenance", self.provenance);
            w.field_bool("verification", self.verification);
            w.begin_array(Some("features"));
            for (name, enabled) in &self.features {
                w.begin_object(None);
                w.field_str("name", name);
                w.field_bool("enabled", *enabled);
                w.end_object();
            }
            w.end_array();
            w.field_str("configurationDigest", &self.digest().to_hex());
            w.end_object();
        }
    }

    // -----------------------------------------------------------------------
    // Manifest parsing
    // -----------------------------------------------------------------------

    /// One `Key = value` entry, with where it came from.
    #[derive(Clone, Debug)]
    struct Entry {
        section: String,
        key: String,
        value: Value,
        line: u32,
        column: u32,
    }

    /// The three value forms the manifest grammar has.
    #[derive(Clone, PartialEq, Eq, Debug)]
    enum Value {
        Text(String),
        Integer(i64),
        Boolean(bool),
    }

    impl Value {
        fn type_name(&self) -> &'static str {
            match self {
                Value::Text(_) => "text",
                Value::Integer(_) => "integer",
                Value::Boolean(_) => "boolean",
            }
        }
    }

    /// Reads a manifest into a validated [`Project`].
    ///
    /// Returns `None` when the manifest cannot be trusted. Every rejection is a
    /// diagnostic with a location, and nothing is ever silently ignored:
    /// directive section 44 is explicit that unknown critical fields must not
    /// pass unnoticed.
    ///
    /// `fallback_id` supplies the application identifier when the manifest omits
    /// one, so that a project with a minimal manifest still builds
    /// (directive section 45).
    pub fn parse_manifest(text: &str, fallback_id: &str, sink: &mut Sink) -> Option<Project> {
        let entries = read_entries(text, sink)?;
        build_project(&entries, fallback_id, sink)
    }

    fn diagnostic(
        code: &str,
        severity: Severity,
        class: FailureClass,
        message: impl Into<String>,
        line: u32,
        column: u32,
    ) -> Diagnostic {
        Diagnostic::new(code, severity, class, "core.project", message).with_location(Location::at(
            MANIFEST_NAME,
            line,
            column,
        ))
    }

    /// Turns manifest text into entries, or explains why it could not.
    fn read_entries(text: &str, sink: &mut Sink) -> Option<Vec<Entry>> {
        if text.len() > MAX_MANIFEST_BYTES {
            sink.emit(
                diagnostic(
                    "E3001",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "The manifest is larger than the accepted limit.",
                    0,
                    0,
                )
                .with_context(format!("Limit: {MAX_MANIFEST_BYTES} bytes"))
                .with_context(format!("Received: {} bytes", text.len()))
                .with_suggestion("A manifest describes a project; it is not a data file."),
            );
            return None;
        }

        let mut entries: Vec<Entry> = Vec::new();
        let mut section: Option<String> = None;
        let mut fatal = false;

        for (index, raw) in text.lines().enumerate() {
            let line = index as u32 + 1;

            if index >= MAX_LINES {
                sink.emit(
                    diagnostic(
                        "E3002",
                        Severity::Error,
                        FailureClass::ResourceExhaustion,
                        "The manifest has more lines than the accepted limit.",
                        line,
                        0,
                    )
                    .with_context(format!("Limit: {MAX_LINES} lines")),
                );
                return None;
            }

            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }

            if let Some(rest) = content.strip_prefix('[') {
                let Some(name) = rest.strip_suffix(']') else {
                    sink.emit(
                        diagnostic(
                            "E3003",
                            Severity::Error,
                            FailureClass::ConfigurationError,
                            "A section header is not closed.",
                            line,
                            1,
                        )
                        .with_context(format!("Read: {}", truncate(content, 64)))
                        .with_suggestion("Write it as [ Project ]."),
                    );
                    fatal = true;
                    continue;
                };
                // Directive section 44 writes headers as "[ Project ]", so the
                // padding inside the brackets is part of the accepted form.
                section = Some(name.trim().to_string());
                continue;
            }

            let Some((key_part, value_part)) = content.split_once('=') else {
                sink.emit(
                    diagnostic(
                        "E3004",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "This line is neither a section header nor a Key = value entry.",
                        line,
                        1,
                    )
                    .with_context(format!("Read: {}", truncate(content, 64)))
                    .with_suggestion("Every entry has the form Key = value."),
                );
                fatal = true;
                continue;
            };

            let Some(current) = section.clone() else {
                sink.emit(
                    diagnostic(
                        "E3005",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "This entry appears before any section header.",
                        line,
                        1,
                    )
                    .with_context(format!("Key: {}", truncate(key_part.trim(), 64)))
                    .with_suggestion("Open a section first, for example [ Project ]."),
                );
                fatal = true;
                continue;
            };

            let key = key_part.trim().to_string();
            let column = (raw.len() - raw.trim_start().len()) as u32 + 1;

            if key.is_empty() {
                sink.emit(
                    diagnostic(
                        "E3006",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "This entry has no key.",
                        line,
                        column,
                    )
                    .with_suggestion("Every entry has the form Key = value."),
                );
                fatal = true;
                continue;
            }

            let Some(value) = read_value(value_part.trim(), line, column, sink) else {
                fatal = true;
                continue;
            };

            if entries.len() >= MAX_ENTRIES {
                sink.emit(
                    diagnostic(
                        "E3007",
                        Severity::Error,
                        FailureClass::ResourceExhaustion,
                        "The manifest has more entries than the accepted limit.",
                        line,
                        column,
                    )
                    .with_context(format!("Limit: {MAX_ENTRIES} entries")),
                );
                return None;
            }

            if let Some(previous) = entries
                .iter()
                .find(|e| e.section == current && e.key == key)
            {
                sink.emit(
                    diagnostic(
                        "E3008",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        format!("'{key}' is set twice in [ {current} ]."),
                        line,
                        column,
                    )
                    .with_context(format!("First set on line {}", previous.line))
                    .with_suggestion(
                        "Remove one. Silently keeping the last value would make the \
                         build depend on line order.",
                    ),
                );
                fatal = true;
                continue;
            }

            entries.push(Entry {
                section: current,
                key,
                value,
                line,
                column,
            });
        }

        if fatal {
            None
        } else {
            Some(entries)
        }
    }

    fn read_value(raw: &str, line: u32, column: u32, sink: &mut Sink) -> Option<Value> {
        if raw.is_empty() {
            sink.emit(
                diagnostic(
                    "E3009",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "This entry has no value.",
                    line,
                    column,
                )
                .with_suggestion("Write a quoted string, a whole number, true or false."),
            );
            return None;
        }

        if let Some(inner) = raw.strip_prefix('"') {
            let Some(text) = inner.strip_suffix('"') else {
                sink.emit(
                    diagnostic(
                        "E3010",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "A quoted value is not closed.",
                        line,
                        column,
                    )
                    .with_context(format!("Read: {}", truncate(raw, 64)))
                    .with_suggestion("Close the quote."),
                );
                return None;
            };
            if text.contains('"') {
                sink.emit(
                    diagnostic(
                        "E3011",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "A quoted value contains a quote.",
                        line,
                        column,
                    )
                    .with_suggestion(
                        "Escapes are not part of this grammar. Choose a value \
                         without a quote in it.",
                    ),
                );
                return None;
            }
            if let Some(bad) = text.chars().find(|c| (*c as u32) < 0x20) {
                sink.emit(
                    diagnostic(
                        "E3012",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        "A value contains a control character.",
                        line,
                        column,
                    )
                    .with_context(format!("Character: U+{:04X}", bad as u32)),
                );
                return None;
            }
            return Some(Value::Text(text.to_string()));
        }

        match raw {
            "true" => return Some(Value::Boolean(true)),
            "false" => return Some(Value::Boolean(false)),
            _ => {}
        }

        if let Ok(number) = raw.parse::<i64>() {
            return Some(Value::Integer(number));
        }

        sink.emit(
            diagnostic(
                "E3013",
                Severity::Error,
                FailureClass::ConfigurationError,
                "This value has no recognised form.",
                line,
                column,
            )
            .with_context(format!("Read: {}", truncate(raw, 64)))
            .with_suggestion(
                "Text must be quoted. Numbers are written plainly. Booleans are \
                 exactly true or false, in lower case.",
            ),
        );
        None
    }

    fn truncate(value: &str, max: usize) -> String {
        if value.chars().count() <= max {
            return value.to_string();
        }
        let mut out: String = value.chars().take(max).collect();
        out.push('…');
        out
    }

    /// Every section and key the manifest grammar defines.
    ///
    /// Anything outside this table is reported rather than ignored. Directive
    /// section 44 requires it, and section 64 forbids configuration that appears
    /// to be in force when it is not.
    const KNOWN: &[(&str, &[&str])] = &[
        ("Project", &["Name", "Id", "Version", "Edition"]),
        ("Android", &["Min_sdk", "Target_sdk", "Compile_sdk"]),
        (
            "Build",
            &[
                "Profile",
                "Optimization",
                "Lto",
                "Incremental",
                "Parallel",
                "Deterministic",
            ],
        ),
        ("Security", &["Guard", "Provenance", "Verification"]),
        ("Features", &[]),
    ];

    fn build_project(entries: &[Entry], fallback_id: &str, sink: &mut Sink) -> Option<Project> {
        let mut project = Project::defaults("", fallback_id);
        let mut failed = false;
        let mut seen_name = false;

        for entry in entries {
            let Some((_, keys)) = KNOWN.iter().find(|(name, _)| *name == entry.section) else {
                sink.emit(
                    diagnostic(
                        "E3020",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        format!(
                            "[ {} ] is not a section this build understands.",
                            entry.section
                        ),
                        entry.line,
                        entry.column,
                    )
                    .with_context(format!(
                        "Known sections: {}",
                        KNOWN.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                    ))
                    .with_suggestion(
                        "An unknown section is reported rather than ignored, because \
                         settings that look like they are in force but are not are \
                         worse than settings that are missing.",
                    ),
                );
                failed = true;
                continue;
            };

            // [ Features ] takes any boolean, so its key list is empty by design.
            if entry.section != "Features" && !keys.contains(&entry.key.as_str()) {
                let suggestion = keys
                    .iter()
                    .find(|known| {
                        known.eq_ignore_ascii_case(entry.key.trim_end_matches(['.', ' ']))
                    })
                    .map(|known| format!("Did you mean '{known}'? Keys are case-sensitive."))
                    .unwrap_or_else(|| {
                        format!("Keys in [ {} ]: {}", entry.section, keys.join(", "))
                    });

                sink.emit(
                    diagnostic(
                        "E3021",
                        Severity::Error,
                        FailureClass::ConfigurationError,
                        format!("'{}' is not a key of [ {} ].", entry.key, entry.section),
                        entry.line,
                        entry.column,
                    )
                    .with_suggestion(suggestion),
                );
                failed = true;
                continue;
            }

            let applied = apply(&mut project, entry, sink, &mut seen_name);
            failed |= !applied;
        }

        if !seen_name {
            sink.emit(
                diagnostic(
                    "E3030",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "The project has no name.",
                    0,
                    0,
                )
                .with_suggestion("Add a quoted Name entry to [ Project ]."),
            );
            failed = true;
        }

        if project.min_sdk > project.target_sdk {
            sink.emit(
                diagnostic(
                    "E3031",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "Min_sdk is higher than Target_sdk.",
                    0,
                    0,
                )
                .with_context(format!(
                    "Min_sdk: {}, Target_sdk: {}",
                    project.min_sdk, project.target_sdk
                ))
                .with_suggestion("An application cannot target a release it refuses to run on."),
            );
            failed = true;
        }

        if project.target_sdk > project.compile_sdk {
            sink.emit(
                diagnostic(
                    "E3032",
                    Severity::Error,
                    FailureClass::ConfigurationError,
                    "Target_sdk is higher than Compile_sdk.",
                    0,
                    0,
                )
                .with_context(format!(
                    "Target_sdk: {}, Compile_sdk: {}",
                    project.target_sdk, project.compile_sdk
                ))
                .with_suggestion(
                    "Targeting a release the code is not compiled against means \
                     opting into behaviour that cannot be checked.",
                ),
            );
            failed = true;
        }

        if project.deterministic && project.profile == Profile::Debug {
            sink.emit(
                diagnostic(
                    "W3033",
                    Severity::Warning,
                    FailureClass::ConfigurationError,
                    "A Debug build is asked to be deterministic.",
                    0,
                    0,
                )
                .with_suggestion(
                    "Debug output carries paths and timestamps that reproducibility \
                     cannot survive. The request is honoured as far as the build \
                     can, and reported here because it may not be fully met.",
                ),
            );
        }

        project.features.sort_by(|a, b| a.0.cmp(&b.0));

        if failed {
            None
        } else {
            Some(project)
        }
    }

    fn apply(project: &mut Project, entry: &Entry, sink: &mut Sink, seen_name: &mut bool) -> bool {
        macro_rules! text {
            () => {
                match &entry.value {
                    Value::Text(text) => text.clone(),
                    other => return wrong_type(entry, "text", other, sink),
                }
            };
        }
        macro_rules! integer {
            () => {
                match &entry.value {
                    Value::Integer(number) => *number,
                    other => return wrong_type(entry, "integer", other, sink),
                }
            };
        }
        macro_rules! boolean {
            () => {
                match &entry.value {
                    Value::Boolean(flag) => *flag,
                    other => return wrong_type(entry, "boolean", other, sink),
                }
            };
        }

        match (entry.section.as_str(), entry.key.as_str()) {
            ("Project", "Name") => {
                let value = text!();
                if value.trim().is_empty() {
                    return reject_value(
                        entry,
                        "The name is empty.",
                        "Give the project a name.",
                        sink,
                    );
                }
                project.name = value;
                *seen_name = true;
            }
            ("Project", "Id") => {
                let value = text!();
                if let Err(reason) = validate_application_id(&value) {
                    return reject_value(entry, &reason, APPLICATION_ID_HELP, sink);
                }
                project.id = value;
            }
            ("Project", "Version") => {
                let value = text!();
                if let Err(reason) = validate_version(&value) {
                    return reject_value(
                        entry,
                        &reason,
                        "Write a version as major.minor.patch, for example 1.0.0.",
                        sink,
                    );
                }
                project.version = value;
            }
            ("Project", "Edition") => {
                let value = text!();
                if let Err(reason) = validate_edition(&value) {
                    return reject_value(
                        entry,
                        &reason,
                        "Write the edition as dd/mm/yyyy, for example 01/01/2000.",
                        sink,
                    );
                }
                project.edition = Some(value);
            }

            ("Android", key) => {
                let value = integer!();
                let Ok(level) = u32::try_from(value) else {
                    return reject_value(
                        entry,
                        "An API level cannot be negative.",
                        "Use a level such as 28 or 36.",
                        sink,
                    );
                };
                if !(1..=100).contains(&level) {
                    return reject_value(
                        entry,
                        "That is not a plausible Android API level.",
                        "Levels currently run from 1 to about 36.",
                        sink,
                    );
                }
                match key {
                    "Min_sdk" => project.min_sdk = level,
                    "Target_sdk" => project.target_sdk = level,
                    "Compile_sdk" => project.compile_sdk = level,
                    _ => unreachable!("the key table already restricted this"),
                }
            }

            ("Build", "Profile") => {
                let value = text!();
                let Some(profile) = Profile::parse(&value) else {
                    return reject_value(
                        entry,
                        &format!("'{value}' is not a build profile."),
                        &format!(
                            "Profiles: {}",
                            Profile::ALL
                                .iter()
                                .map(|p| p.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        sink,
                    );
                };
                project.profile = profile;
            }
            ("Build", "Optimization") => {
                let value = text!();
                let Some(optimization) = Optimization::parse(&value) else {
                    return reject_value(
                        entry,
                        &format!("'{value}' is not an optimisation level."),
                        &format!(
                            "Levels: {}",
                            Optimization::ALL
                                .iter()
                                .map(|o| o.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        sink,
                    );
                };
                project.optimization = optimization;
            }
            ("Build", "Lto") => project.lto = boolean!(),
            ("Build", "Incremental") => project.incremental = boolean!(),
            ("Build", "Parallel") => project.parallel = boolean!(),
            ("Build", "Deterministic") => project.deterministic = boolean!(),

            ("Security", "Guard") => {
                let value = text!();
                let Some(level) = GuardLevel::parse(&value) else {
                    return reject_value(
                        entry,
                        &format!("'{value}' is not a guard level."),
                        &format!(
                            "Levels: {}",
                            GuardLevel::ALL
                                .iter()
                                .map(|g| g.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        sink,
                    );
                };
                project.guard = level;
            }
            ("Security", "Provenance") => project.provenance = boolean!(),
            ("Security", "Verification") => project.verification = boolean!(),

            ("Features", name) => {
                let enabled = boolean!();
                if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return reject_value(
                        entry,
                        "A feature name may contain only letters, digits and underscores.",
                        "Rename the feature.",
                        sink,
                    );
                }
                project.features.push((name.to_string(), enabled));
            }

            _ => unreachable!("the section and key tables already restricted this"),
        }

        true
    }

    const APPLICATION_ID_HELP: &str =
        "An application identifier is reverse-DNS, such as com.example.app: at \
         least two parts, each starting with a letter and containing only \
         letters, digits and underscores.";

    fn wrong_type(entry: &Entry, expected: &str, found: &Value, sink: &mut Sink) -> bool {
        sink.emit(
            diagnostic(
                "E3040",
                Severity::Error,
                FailureClass::ConfigurationError,
                format!("'{}' expects {expected}.", entry.key),
                entry.line,
                entry.column,
            )
            .with_context(format!("Found: {}", found.type_name()))
            .with_suggestion(match expected {
                "text" => "Quote the value.",
                "integer" => "Write a whole number without quotes.",
                _ => "Write true or false, without quotes.",
            }),
        );
        false
    }

    fn reject_value(entry: &Entry, message: &str, suggestion: &str, sink: &mut Sink) -> bool {
        sink.emit(
            diagnostic(
                "E3041",
                Severity::Error,
                FailureClass::UserError,
                message,
                entry.line,
                entry.column,
            )
            .with_context(format!("Key: {}", entry.key))
            .with_suggestion(suggestion),
        );
        false
    }

    fn validate_application_id(value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("The application identifier is empty.".to_string());
        }
        if value.len() > 255 {
            return Err("The application identifier is too long.".to_string());
        }
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() < 2 {
            return Err(format!("'{value}' has only one part."));
        }
        for part in parts {
            if part.is_empty() {
                return Err("The application identifier has an empty part.".to_string());
            }
            if !part.starts_with(|c: char| c.is_ascii_alphabetic()) {
                return Err(format!("The part '{part}' does not start with a letter."));
            }
            if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("The part '{part}' contains an unusable character."));
            }
        }
        Ok(())
    }

    fn validate_version(value: &str) -> Result<(), String> {
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("'{value}' is not major.minor.patch."));
        }
        for part in parts {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("'{value}' has a part that is not a number."));
            }
            if part.len() > 1 && part.starts_with('0') {
                return Err(format!("'{value}' has a part with a leading zero."));
            }
        }
        Ok(())
    }

    fn validate_edition(value: &str) -> Result<(), String> {
        let parts: Vec<&str> = value.split('/').collect();
        if parts.len() != 3 {
            return Err(format!("'{value}' is not a dd/mm/yyyy date."));
        }
        let widths = [2usize, 2, 4];
        let mut numbers = [0u32; 3];
        for (index, part) in parts.iter().enumerate() {
            if part.len() != widths[index] || !part.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("'{value}' is not a dd/mm/yyyy date."));
            }
            numbers[index] = part.parse().unwrap_or(0);
        }
        if !(1..=31).contains(&numbers[0]) || !(1..=12).contains(&numbers[1]) {
            return Err(format!("'{value}' is not a real date."));
        }
        Ok(())
    }
}

// ===========================================================================
// artifact — what a build produces (directive sections 58 and 59)
// ===========================================================================

/// Artifacts and the states they pass through.
///
/// Directive section 58 fixes the lifecycle:
///
/// ```text
/// CREATED -> HASHED -> VALIDATED -> SIGNED -> VERIFIED -> PUBLISHED
/// ```
///
/// and says plainly that an invalid artifact cannot be published. That sentence
/// is the whole reason this module exists as a type rather than as a convention:
/// the transitions are the only way to move an artifact forward, so "published
/// without being verified" is not a bug that can be written.
///
/// **Status** — PARTIAL. The lifecycle is real and enforced; nothing in this tree
/// signs anything yet, so [`State::Signed`] is reachable but unused.
pub mod artifact {
    use crate::diag::{Diagnostic, Severity};
    use crate::hash::Digest;
    use crate::json::Writer;
    use crate::vfs::VirtualPath;
    use crate::FailureClass;

    /// Where an artifact is in its lifecycle.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub enum State {
        /// It exists.
        Created,
        /// Its content has been hashed.
        Hashed,
        /// Its structure has been checked against what it claims to be.
        Validated,
        /// It carries a signature.
        Signed,
        /// Its digest, and its signature where it has one, have been checked.
        Verified,
        /// It has been moved to where consumers look for it.
        Published,
    }

    impl State {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                State::Created => "CREATED",
                State::Hashed => "HASHED",
                State::Validated => "VALIDATED",
                State::Signed => "SIGNED",
                State::Verified => "VERIFIED",
                State::Published => "PUBLISHED",
            }
        }

        /// Whether this state may be followed by `next`.
        ///
        /// Signing is optional, so `VALIDATED` may go straight to `VERIFIED`;
        /// every other step is mandatory and in order.
        pub const fn may_advance_to(self, next: State) -> bool {
            matches!(
                (self, next),
                (State::Created, State::Hashed)
                    | (State::Hashed, State::Validated)
                    | (State::Validated, State::Signed)
                    | (State::Validated, State::Verified)
                    | (State::Signed, State::Verified)
                    | (State::Verified, State::Published)
            )
        }
    }

    impl core::fmt::Display for State {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    /// Stable identity of an artifact within a build.
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct ArtifactId(String);

    impl ArtifactId {
        /// Creates an identifier.
        ///
        /// Identifiers name things in reports and in cache keys, so the accepted
        /// character set is deliberately narrow: anything that would need
        /// escaping somewhere later is refused here instead.
        pub fn new(value: impl Into<String>) -> Result<ArtifactId, Diagnostic> {
            let value = value.into();
            let usable = !value.is_empty()
                && value.len() <= 128
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');

            if !usable {
                return Err(Diagnostic::new(
                    "E5001",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.artifact",
                    "That is not a usable artifact identifier.",
                )
                .with_context(format!("Given: {value}"))
                .with_suggestion(
                    "Use letters, digits, '.', '-' and '_', up to 128 characters, \
                     for example 'dex.classes'.",
                ));
            }

            Ok(ArtifactId(value))
        }

        /// The identifier as text.
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    impl core::fmt::Display for ArtifactId {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.0)
        }
    }

    /// Something a build produced.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Artifact {
        id: ArtifactId,
        kind: String,
        path: VirtualPath,
        size: u64,
        digest: Option<Digest>,
        state: State,
        /// Every state this artifact has been in, oldest first.
        history: Vec<State>,
    }

    impl Artifact {
        /// Records that an artifact now exists.
        pub fn created(id: ArtifactId, kind: impl Into<String>, path: VirtualPath) -> Artifact {
            Artifact {
                id,
                kind: kind.into(),
                path,
                size: 0,
                digest: None,
                state: State::Created,
                history: vec![State::Created],
            }
        }

        /// Identity.
        pub fn id(&self) -> &ArtifactId {
            &self.id
        }

        /// What kind of thing it is, for example `dex` or `apk`.
        pub fn kind(&self) -> &str {
            &self.kind
        }

        /// Where it lives.
        pub fn path(&self) -> &VirtualPath {
            &self.path
        }

        /// Its size in bytes, once hashed.
        pub fn size(&self) -> u64 {
            self.size
        }

        /// Its digest, once hashed.
        pub fn digest(&self) -> Option<Digest> {
            self.digest
        }

        /// Its current state.
        pub fn state(&self) -> State {
            self.state
        }

        /// Every state it has been in.
        pub fn history(&self) -> &[State] {
            &self.history
        }

        /// Records the digest of its content.
        pub fn hashed(&mut self, digest: Digest, size: u64) -> Result<(), Diagnostic> {
            self.advance(State::Hashed)?;
            self.digest = Some(digest);
            self.size = size;
            Ok(())
        }

        /// Records that its structure has been checked.
        pub fn validated(&mut self) -> Result<(), Diagnostic> {
            self.advance(State::Validated)
        }

        /// Records that it has been signed.
        pub fn signed(&mut self) -> Result<(), Diagnostic> {
            self.advance(State::Signed)
        }

        /// Records that its digest still matches its content.
        ///
        /// The digest presented here is compared with the one taken at hashing
        /// time. Directive section 6's invariant I6 requires verification before
        /// use, and a verification that does not compare anything is decoration.
        pub fn verified(&mut self, observed: Digest) -> Result<(), Diagnostic> {
            let Some(expected) = self.digest else {
                return Err(self.refuse(
                    "E5010",
                    "The artifact has no digest to verify against.",
                    "Hash it before verifying it.",
                ));
            };

            if observed != expected {
                return Err(Diagnostic::new(
                    "E5011",
                    Severity::Fatal,
                    FailureClass::Corruption,
                    "core.artifact",
                    "The artifact does not match the digest taken when it was built.",
                )
                .with_context(format!("Artifact: {}", self.id))
                .with_context(format!("Expected: {expected}"))
                .with_context(format!("Found: {observed}"))
                .with_suggestion(
                    "The file changed after the build produced it, or storage \
                     returned different bytes. Do not use it.",
                ));
            }

            self.advance(State::Verified)
        }

        /// Publishes the artifact.
        ///
        /// Refuses unless it has been verified. Directive section 58 is a single
        /// sentence on this, and this is where that sentence is enforced.
        pub fn published(&mut self) -> Result<(), Diagnostic> {
            self.advance(State::Published)
        }

        fn advance(&mut self, next: State) -> Result<(), Diagnostic> {
            if !self.state.may_advance_to(next) {
                return Err(self.refuse(
                    "E5012",
                    &format!("An artifact cannot go from {} to {next}.", self.state),
                    "The lifecycle of directive section 58 runs CREATED, HASHED, \
                     VALIDATED, optionally SIGNED, VERIFIED, PUBLISHED.",
                ));
            }
            self.state = next;
            self.history.push(next);
            Ok(())
        }

        fn refuse(&self, code: &str, message: &str, suggestion: &str) -> Diagnostic {
            Diagnostic::new(
                code,
                Severity::Error,
                FailureClass::InternalError,
                "core.artifact",
                message,
            )
            .with_context(format!("Artifact: {}", self.id))
            .with_context(format!("State: {}", self.state))
            .with_suggestion(suggestion)
        }

        /// Serialises the artifact as an object inside an open array.
        pub fn write_json(&self, w: &mut Writer) {
            w.begin_object(None);
            w.field_str("id", self.id.as_str());
            w.field_str("kind", &self.kind);
            w.field_str("path", &self.path.as_str());
            w.field_u64("size", self.size);
            w.field_str("state", self.state.as_str());
            match self.digest {
                Some(digest) => w.field_str("digest", &digest.to_hex()),
                None => w.field_bool("hashed", false),
            }
            w.begin_array(Some("history"));
            for state in &self.history {
                w.element_str(state.as_str());
            }
            w.end_array();
            w.end_object();
        }
    }
}

// ===========================================================================
// cache — incremental build keys (directive section 11)
// ===========================================================================

/// Cache keys and the four outcomes a lookup can have.
///
/// Directive section 11 lists what a cache key must cover and insists the four
/// outcomes stay distinguishable. Both are enforced here by construction: the
/// key is built from a struct with a field per required input, so a key can
/// never be computed from a subset by accident, and a corrupt entry is a state
/// of its own rather than a miss.
pub mod cache {
    use crate::hash::{sha256_fields, Digest};
    use crate::json::Writer;
    use crate::project::{Optimization, Profile};

    /// Everything that may change what a build step produces.
    ///
    /// Every field of directive section 11 is present and none is optional. If a
    /// new input starts to affect output, adding it here is a compile error at
    /// every construction site, which is exactly the reminder that is wanted.
    #[derive(Clone, Copy, Debug)]
    pub struct Inputs<'a> {
        /// Digest of the sources this step reads.
        pub source_digest: Digest,
        /// Combined digest of the outputs this step depends on.
        pub dependency_digest: Digest,
        /// Version of the plugin performing the step.
        pub plugin_version: &'a str,
        /// Version of the compiler it drives.
        pub compiler_version: &'a str,
        /// Version of the toolchain as a whole.
        pub toolchain_version: &'a str,
        /// Behavioural contract the application opts into.
        pub target_sdk: u32,
        /// Lowest release the application supports.
        pub min_sdk: u32,
        /// Target architecture.
        pub abi: &'a str,
        /// Build profile.
        pub profile: Profile,
        /// Optimisation level.
        pub optimization: Optimization,
        /// Serialised feature switches.
        pub feature_configuration: &'a str,
        /// Environment variables that genuinely affect the output.
        ///
        /// Deliberately a list rather than "the environment": a build that
        /// depends on the whole environment is not reproducible, and one that
        /// silently ignores it is wrong (directive section 64).
        pub relevant_environment: &'a [(&'a str, &'a str)],
        /// Identifier of the security policy in force.
        pub security_policy: &'a str,
    }

    impl Inputs<'_> {
        /// Computes the cache key.
        pub fn key(&self) -> Key {
            let numbers = format!("{}|{}", self.min_sdk, self.target_sdk);
            let environment = self
                .relevant_environment
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("\u{1f}");

            Key(sha256_fields(&[
                ("source", self.source_digest.as_bytes()),
                ("dependencies", self.dependency_digest.as_bytes()),
                ("plugin", self.plugin_version.as_bytes()),
                ("compiler", self.compiler_version.as_bytes()),
                ("toolchain", self.toolchain_version.as_bytes()),
                ("sdk", numbers.as_bytes()),
                ("abi", self.abi.as_bytes()),
                ("profile", self.profile.as_str().as_bytes()),
                ("optimization", self.optimization.as_str().as_bytes()),
                ("features", self.feature_configuration.as_bytes()),
                ("environment", environment.as_bytes()),
                ("security", self.security_policy.as_bytes()),
            ]))
        }
    }

    /// The identity of a cached result.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct Key(Digest);

    impl Key {
        /// The key as hexadecimal.
        pub fn to_hex(self) -> String {
            self.0.to_hex()
        }

        /// A short form for logs and reports.
        pub fn to_short_hex(self) -> String {
            self.0.to_short_hex(8)
        }
    }

    impl core::fmt::Display for Key {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.0.to_hex())
        }
    }

    /// What a lookup found.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Lookup {
        /// A usable entry whose content still matches its recorded digest.
        Hit,
        /// Nothing stored under this key.
        Miss,
        /// An entry was stored but has been marked unusable.
        Invalidated,
        /// An entry exists and its content does not match its digest.
        ///
        /// Kept separate from a miss on purpose. Directive section 11 does not
        /// permit a corrupt entry to be treated as absent: a miss is normal,
        /// corruption is a fault that someone needs to know about.
        Corrupted,
    }

    impl Lookup {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Lookup::Hit => "CACHE_HIT",
                Lookup::Miss => "CACHE_MISS",
                Lookup::Invalidated => "CACHE_INVALIDATED",
                Lookup::Corrupted => "CACHE_CORRUPTED",
            }
        }

        /// Whether the stored result may be reused.
        pub const fn is_usable(self) -> bool {
            matches!(self, Lookup::Hit)
        }
    }

    /// One stored result.
    #[derive(Clone, Copy, Debug)]
    struct Entry {
        key: Key,
        content: Digest,
        valid: bool,
    }

    /// Counters worth reporting after a build (directive section 56).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Statistics {
        /// Lookups that could be reused.
        pub hits: u64,
        /// Lookups that found nothing.
        pub misses: u64,
        /// Lookups that found an entry marked unusable.
        pub invalidated: u64,
        /// Lookups that found an entry whose content had changed underneath it.
        pub corrupted: u64,
    }

    /// An in-memory cache index.
    ///
    /// **Status** — FOUNDATION. It records what is cached and answers lookups
    /// correctly; it stores no bytes and survives no restart. A persistent store
    /// needs the virtual filesystem underneath it and a decision about eviction,
    /// neither of which is written yet.
    #[derive(Clone, Debug, Default)]
    pub struct Index {
        entries: Vec<Entry>,
        statistics: Statistics,
    }

    impl Index {
        /// An empty index.
        pub fn new() -> Self {
            Index::default()
        }

        /// Records a result under a key.
        pub fn store(&mut self, key: Key, content: Digest) {
            match self.entries.iter_mut().find(|entry| entry.key == key) {
                Some(entry) => {
                    entry.content = content;
                    entry.valid = true;
                }
                None => self.entries.push(Entry {
                    key,
                    content,
                    valid: true,
                }),
            }
        }

        /// Marks an entry unusable without forgetting that it existed.
        pub fn invalidate(&mut self, key: Key) {
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
                entry.valid = false;
            }
        }

        /// Asks what is stored under a key, given the content found on disk.
        ///
        /// `observed` is the digest of what is actually there now. Passing it is
        /// mandatory: a cache that answers without looking at the content cannot
        /// tell a hit from corruption.
        pub fn lookup(&mut self, key: Key, observed: Option<Digest>) -> Lookup {
            let outcome = match self.entries.iter().find(|entry| entry.key == key) {
                None => Lookup::Miss,
                Some(entry) if !entry.valid => Lookup::Invalidated,
                Some(entry) => match observed {
                    Some(found) if found == entry.content => Lookup::Hit,
                    Some(_) => Lookup::Corrupted,
                    None => Lookup::Miss,
                },
            };

            match outcome {
                Lookup::Hit => self.statistics.hits += 1,
                Lookup::Miss => self.statistics.misses += 1,
                Lookup::Invalidated => self.statistics.invalidated += 1,
                Lookup::Corrupted => self.statistics.corrupted += 1,
            }
            outcome
        }

        /// Counters accumulated so far.
        pub fn statistics(&self) -> Statistics {
            self.statistics
        }

        /// Number of entries held.
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether nothing is held.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Serialises the counters as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_u64("entries", self.entries.len() as u64);
            w.field_u64("hits", self.statistics.hits);
            w.field_u64("misses", self.statistics.misses);
            w.field_u64("invalidated", self.statistics.invalidated);
            w.field_u64("corrupted", self.statistics.corrupted);
            w.field_bool("persistent", false);
            w.end_object();
        }
    }
}

// ===========================================================================
// graph — the build graph (directive sections 9 and 10)
// ===========================================================================

/// A real directed acyclic graph of build work.
///
/// Directive section 9 opens with the point of this module: a build system is
/// not an ordered list of files. Every node names its dependencies explicitly,
/// the order comes out of the graph rather than out of the order things were
/// added, and a cycle is a diagnostic rather than a hang.
///
/// **Status** — PARTIAL. The graph, its invariants and its ordering are real.
/// Node timing and memory figures are recorded but are only as good as what the
/// scheduler measures, and nothing yet reads a previous build's graph back.
pub mod graph {
    use crate::artifact::ArtifactId;
    use crate::cache::Key as CacheKey;
    use crate::diag::{Diagnostic, Severity};
    use crate::hash::Digest;
    use crate::json::Writer;
    use crate::FailureClass;

    /// Diagnostic code for a graph that is not acyclic.
    ///
    /// Directive section 10 names this code literally, so it is spelled the way
    /// the directive spells it rather than following the `E****` convention.
    pub const CYCLE_CODE: &str = "BUILD_GRAPH_CYCLE";

    /// Largest graph the scheduler will accept (directive section 60).
    pub const MAX_NODES: usize = 100_000;

    /// Stable identity of a node.
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct NodeId(String);

    impl NodeId {
        /// Creates an identifier.
        pub fn new(value: impl Into<String>) -> Result<NodeId, Diagnostic> {
            let value = value.into();
            let usable = !value.is_empty()
                && value.len() <= 128
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');

            if !usable {
                return Err(Diagnostic::new(
                    "E4001",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.graph",
                    "That is not a usable node identifier.",
                )
                .with_context(format!("Given: {value}"))
                .with_suggestion(
                    "Use letters, digits, '.', '-' and '_', for example \
                     'compile.kotlin'.",
                ));
            }

            Ok(NodeId(value))
        }

        /// The identifier as text.
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    impl core::fmt::Display for NodeId {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.0)
        }
    }

    /// What a node does, following the pipeline of directive section 9.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Kind {
        /// Read and validate the project manifest.
        Manifest,
        /// Compile resources.
        Resources,
        /// Analyse sources.
        SourceAnalysis,
        /// Produce compiler intermediate representation.
        CompilerIr,
        /// Produce Dalvik executables.
        Dex,
        /// Produce native libraries.
        Native,
        /// Decide the package layout.
        ApkLayout,
        /// Build the package.
        Package,
        /// Sign the package.
        Sign,
        /// Verify the result.
        Verify,
    }

    impl Kind {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Kind::Manifest => "MANIFEST",
                Kind::Resources => "RESOURCES",
                Kind::SourceAnalysis => "SOURCE_ANALYSIS",
                Kind::CompilerIr => "COMPILER_IR",
                Kind::Dex => "DEX",
                Kind::Native => "NATIVE",
                Kind::ApkLayout => "APK_LAYOUT",
                Kind::Package => "PACKAGE",
                Kind::Sign => "SIGN",
                Kind::Verify => "VERIFY",
            }
        }
    }

    /// Where a node is in its execution.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Status {
        /// Not started.
        Pending,
        /// Started, not finished.
        Running,
        /// Finished and produced what it promised.
        Succeeded,
        /// Its result was reused from the cache.
        CacheHit,
        /// It ran and failed.
        Failed,
        /// It did not run because something it depends on failed.
        ///
        /// Distinct from `Failed`: this node has no defect of its own, and
        /// reporting it as failed would send whoever reads the build to the
        /// wrong place.
        Skipped,
        /// It did not run because the build was cancelled.
        Cancelled,
    }

    impl Status {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Status::Pending => "PENDING",
                Status::Running => "RUNNING",
                Status::Succeeded => "SUCCEEDED",
                Status::CacheHit => "CACHE_HIT",
                Status::Failed => "FAILED",
                Status::Skipped => "SKIPPED",
                Status::Cancelled => "CANCELLED",
            }
        }

        /// Whether dependents of this node may run.
        ///
        /// Directive section 10 forbids treating a failed, skipped or cancelled
        /// node as a success, and this is the single place that question is
        /// answered.
        pub const fn produced_its_outputs(self) -> bool {
            matches!(self, Status::Succeeded | Status::CacheHit)
        }

        /// Whether the node is finished, whatever the outcome.
        pub const fn is_finished(self) -> bool {
            !matches!(self, Status::Pending | Status::Running)
        }
    }

    /// What a node measured while it ran.
    ///
    /// Wall-clock timestamps are deliberately absent. Directive section 9 lists
    /// start and end times, but an artifact that embeds them is not reproducible
    /// (section 12), so the graph records how long a node took rather than when
    /// it happened. The scheduler supplies the figures; a zero means nothing was
    /// measured, never that nothing was used.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Measurements {
        /// How long the node ran, in microseconds.
        pub duration_micros: u64,
        /// Peak memory attributable to the node, in bytes.
        pub peak_memory_bytes: u64,
    }

    /// One unit of build work.
    #[derive(Clone, Debug)]
    pub struct Node {
        id: NodeId,
        kind: Kind,
        plugin: String,
        inputs: Vec<ArtifactId>,
        outputs: Vec<ArtifactId>,
        dependencies: Vec<NodeId>,

        input_digest: Digest,
        configuration_digest: Digest,
        toolchain_digest: Digest,
        plugin_digest: Digest,

        status: Status,
        measurements: Measurements,
        cache_key: Option<CacheKey>,
        artifact_digest: Option<Digest>,
        diagnostics: Vec<String>,
    }

    impl Node {
        /// Describes a unit of work.
        pub fn new(
            id: NodeId,
            kind: Kind,
            plugin: impl Into<String>,
            digests: [Digest; 4],
        ) -> Node {
            Node {
                id,
                kind,
                plugin: plugin.into(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                dependencies: Vec::new(),
                input_digest: digests[0],
                configuration_digest: digests[1],
                toolchain_digest: digests[2],
                plugin_digest: digests[3],
                status: Status::Pending,
                measurements: Measurements::default(),
                cache_key: None,
                artifact_digest: None,
                diagnostics: Vec::new(),
            }
        }

        /// Declares an artifact this node consumes.
        pub fn with_input(mut self, id: ArtifactId) -> Self {
            self.inputs.push(id);
            self
        }

        /// Declares an artifact this node produces.
        pub fn with_output(mut self, id: ArtifactId) -> Self {
            self.outputs.push(id);
            self
        }

        /// Declares a node that must finish first.
        pub fn after(mut self, id: NodeId) -> Self {
            self.dependencies.push(id);
            self
        }

        /// Identity.
        pub fn id(&self) -> &NodeId {
            &self.id
        }

        /// What it does.
        pub fn kind(&self) -> Kind {
            self.kind
        }

        /// Which plugin performs it.
        pub fn plugin(&self) -> &str {
            &self.plugin
        }

        /// Artifacts it consumes.
        pub fn inputs(&self) -> &[ArtifactId] {
            &self.inputs
        }

        /// Artifacts it produces.
        pub fn outputs(&self) -> &[ArtifactId] {
            &self.outputs
        }

        /// Nodes that must finish first.
        pub fn dependencies(&self) -> &[NodeId] {
            &self.dependencies
        }

        /// Where it is in its execution.
        pub fn status(&self) -> Status {
            self.status
        }

        /// What it measured.
        pub fn measurements(&self) -> Measurements {
            self.measurements
        }

        /// Its cache key, once computed.
        pub fn cache_key(&self) -> Option<CacheKey> {
            self.cache_key
        }

        /// Digest of what it produced, once it has produced it.
        pub fn artifact_digest(&self) -> Option<Digest> {
            self.artifact_digest
        }

        /// Codes of diagnostics raised while it ran.
        pub fn diagnostics(&self) -> &[String] {
            &self.diagnostics
        }

        /// Serialises the node as an object inside an open array.
        pub fn write_json(&self, w: &mut Writer) {
            w.begin_object(None);
            w.field_str("id", self.id.as_str());
            w.field_str("kind", self.kind.as_str());
            w.field_str("plugin", &self.plugin);
            w.field_str("status", self.status.as_str());

            w.begin_array(Some("inputs"));
            for input in &self.inputs {
                w.element_str(input.as_str());
            }
            w.end_array();
            w.begin_array(Some("outputs"));
            for output in &self.outputs {
                w.element_str(output.as_str());
            }
            w.end_array();
            w.begin_array(Some("dependencies"));
            for dependency in &self.dependencies {
                w.element_str(dependency.as_str());
            }
            w.end_array();

            w.field_str("inputDigest", &self.input_digest.to_short_hex(8));
            w.field_str(
                "configurationDigest",
                &self.configuration_digest.to_short_hex(8),
            );
            w.field_str("toolchainDigest", &self.toolchain_digest.to_short_hex(8));
            w.field_str("pluginDigest", &self.plugin_digest.to_short_hex(8));
            w.field_u64("durationMicros", self.measurements.duration_micros);
            w.field_u64("peakMemoryBytes", self.measurements.peak_memory_bytes);
            if let Some(key) = self.cache_key {
                w.field_str("cacheKey", &key.to_short_hex());
            }
            if let Some(digest) = self.artifact_digest {
                w.field_str("artifactDigest", &digest.to_hex());
            }
            w.begin_array(Some("diagnostics"));
            for code in &self.diagnostics {
                w.element_str(code);
            }
            w.end_array();
            w.end_object();
        }
    }

    /// The graph itself.
    #[derive(Clone, Debug, Default)]
    pub struct Graph {
        nodes: Vec<Node>,
    }

    impl Graph {
        /// An empty graph.
        pub fn new() -> Self {
            Graph::default()
        }

        /// Adds a node.
        ///
        /// Refuses a duplicate identifier: two nodes answering to one name would
        /// make every dependency edge ambiguous.
        pub fn add(&mut self, node: Node) -> Result<(), Diagnostic> {
            if self.nodes.len() >= MAX_NODES {
                return Err(Diagnostic::new(
                    "E4002",
                    Severity::Fatal,
                    FailureClass::ResourceExhaustion,
                    "core.graph",
                    "The build graph has more nodes than the scheduler accepts.",
                )
                .with_context(format!("Limit: {MAX_NODES} nodes")));
            }

            if self.nodes.iter().any(|existing| existing.id == node.id) {
                return Err(Diagnostic::new(
                    "E4003",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.graph",
                    format!("There is already a node called '{}'.", node.id),
                )
                .with_suggestion("Node identifiers are unique within a graph."));
            }

            self.nodes.push(node);
            Ok(())
        }

        /// Every node, in the order they were added.
        pub fn nodes(&self) -> &[Node] {
            &self.nodes
        }

        /// Number of nodes.
        pub fn len(&self) -> usize {
            self.nodes.len()
        }

        /// Whether the graph is empty.
        pub fn is_empty(&self) -> bool {
            self.nodes.is_empty()
        }

        /// Looks a node up.
        pub fn node(&self, id: &NodeId) -> Option<&Node> {
            self.nodes.iter().find(|node| &node.id == id)
        }

        /// Looks a node up for modification.
        pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
            self.nodes.iter_mut().find(|node| &node.id == id)
        }

        /// Records how a node finished.
        pub(crate) fn finish(
            &mut self,
            id: &NodeId,
            status: Status,
            measurements: Measurements,
            artifact_digest: Option<Digest>,
            diagnostics: Vec<String>,
        ) {
            if let Some(node) = self.node_mut(id) {
                node.status = status;
                node.measurements = measurements;
                node.artifact_digest = artifact_digest;
                node.diagnostics = diagnostics;
            }
        }

        /// Records the cache key computed for a node.
        pub fn set_cache_key(&mut self, id: &NodeId, key: CacheKey) {
            if let Some(node) = self.node_mut(id) {
                node.cache_key = Some(key);
            }
        }

        /// Nodes that depend on this one, directly.
        pub fn dependents(&self, id: &NodeId) -> Vec<&NodeId> {
            self.nodes
                .iter()
                .filter(|node| node.dependencies.contains(id))
                .map(|node| &node.id)
                .collect()
        }

        /// Checks the graph and returns the order work may run in.
        ///
        /// The order is produced by Kahn's algorithm over nodes taken in
        /// insertion order, so a given graph always yields the same plan
        /// (directive section 12). Every failure mode of directive section 10 is
        /// answered here: an edge to a node that does not exist, and a cycle.
        pub fn plan(&self) -> Result<Vec<NodeId>, Diagnostic> {
            for node in &self.nodes {
                for dependency in &node.dependencies {
                    if self.node(dependency).is_none() {
                        return Err(Diagnostic::new(
                            "E4004",
                            Severity::Fatal,
                            FailureClass::InternalError,
                            "core.graph",
                            format!(
                                "'{}' depends on '{dependency}', which is not in the graph.",
                                node.id
                            ),
                        )
                        .with_suggestion(
                            "Add the node, or remove the edge. A build cannot wait \
                             for work nobody is going to do.",
                        ));
                    }
                }

                if node.dependencies.contains(&node.id) {
                    return Err(Self::cycle_diagnostic(std::slice::from_ref(&node.id)));
                }
            }

            let mut remaining: Vec<usize> = (0..self.nodes.len()).collect();
            let mut order: Vec<NodeId> = Vec::with_capacity(self.nodes.len());
            let mut done: Vec<NodeId> = Vec::with_capacity(self.nodes.len());

            while !remaining.is_empty() {
                let ready: Vec<usize> = remaining
                    .iter()
                    .copied()
                    .filter(|index| {
                        self.nodes[*index]
                            .dependencies
                            .iter()
                            .all(|dependency| done.contains(dependency))
                    })
                    .collect();

                if ready.is_empty() {
                    let involved: Vec<NodeId> = remaining
                        .iter()
                        .map(|index| self.nodes[*index].id.clone())
                        .collect();
                    return Err(Self::cycle_diagnostic(&involved));
                }

                for index in ready {
                    order.push(self.nodes[index].id.clone());
                    done.push(self.nodes[index].id.clone());
                    remaining.retain(|candidate| *candidate != index);
                }
            }

            Ok(order)
        }

        fn cycle_diagnostic(involved: &[NodeId]) -> Diagnostic {
            let mut names: Vec<String> =
                involved.iter().map(|id| id.as_str().to_string()).collect();
            names.sort();

            Diagnostic::new(
                CYCLE_CODE,
                Severity::Fatal,
                FailureClass::ConfigurationError,
                "core.graph",
                "The build graph contains a cycle.",
            )
            .with_context(format!("Nodes involved: {}", names.join(", ")))
            .with_suggestion(
                "Every dependency edge points at work that must finish first, so a \
                 cycle asks for something to happen before itself. Break it by \
                 removing an edge or by splitting a node.",
            )
        }

        /// Serialises the graph as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_u64("nodes", self.nodes.len() as u64);
            w.field_u64(
                "edges",
                self.nodes
                    .iter()
                    .map(|node| node.dependencies.len() as u64)
                    .sum(),
            );
            match self.plan() {
                Ok(order) => {
                    w.field_bool("acyclic", true);
                    w.begin_array(Some("order"));
                    for id in order {
                        w.element_str(id.as_str());
                    }
                    w.end_array();
                }
                Err(error) => {
                    w.field_bool("acyclic", false);
                    w.field_str("problem", &error.code);
                }
            }
            w.begin_array(Some("nodeDetail"));
            for node in &self.nodes {
                node.write_json(w);
            }
            w.end_array();
            w.end_object();
        }
    }
}

// ===========================================================================
// scheduler — running the graph (directive sections 10, 35 and 36)
// ===========================================================================

/// Executes a build graph without violating any of its invariants.
///
/// Directive section 10 states what a scheduler may not do: cross a dependency
/// edge, accept a cycle, ignore a failed dependency, call a cancelled node
/// successful, or treat a stale artifact as fresh. Each of those is a test in
/// this file rather than a promise in a comment.
///
/// **Status** — PARTIAL. Ordering, failure propagation and cancellation are
/// real. Execution is sequential: directive section 36 asks for a scheduler that
/// is aware of memory, battery and thermal state, and none of that is
/// implemented, so nothing here claims to run work in parallel.
pub mod scheduler {
    use crate::caps::Policy;
    use crate::diag::{Diagnostic, Severity, Sink};
    use crate::graph::{Graph, Measurements, Node, NodeId, Status};
    use crate::hash::Digest;
    use crate::json::Writer;
    use crate::plugin::{Context, Registry};
    use crate::FailureClass;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    /// A build's cancellation flag (directive section 35).
    ///
    /// Cheap to clone and safe to share, so a user interface can hold one while
    /// the build holds another. Cancelling is one-way: a build that has been
    /// asked to stop is not restarted by clearing a flag, it is started again.
    #[derive(Clone, Debug, Default)]
    pub struct Cancellation {
        flag: Arc<AtomicBool>,
    }

    impl Cancellation {
        /// A token that has not been cancelled.
        pub fn new() -> Self {
            Cancellation::default()
        }

        /// Asks the build to stop at its next checkpoint.
        pub fn cancel(&self) {
            self.flag.store(true, Ordering::SeqCst);
        }

        /// Whether cancellation has been requested.
        pub fn is_cancelled(&self) -> bool {
            self.flag.load(Ordering::SeqCst)
        }
    }

    /// What a node produced.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct NodeResult {
        /// Digest of the artifact it produced, when it produced one.
        pub artifact_digest: Option<Digest>,
        /// Whether the result was reused rather than computed.
        pub from_cache: bool,
        /// Peak memory attributable to the node.
        ///
        /// Zero means "not measured", never "none used". Directive section 1
        /// does not allow an unmeasured figure to be presented as a measurement.
        pub peak_memory_bytes: u64,
    }

    /// Whatever actually performs a node's work.
    ///
    /// The scheduler owns ordering and invariants; what a node *does* is
    /// somebody else's problem. That separation is what lets these invariants be
    /// tested without a compiler existing (directive section 66).
    pub trait NodeExecutor {
        /// Performs the node's work.
        fn execute(&mut self, node: &Node, ctx: &mut Context<'_>)
            -> Result<NodeResult, Diagnostic>;
    }

    /// Runs each node through the plugin registry.
    ///
    /// Every plugin in this tree is `PLANNED`, so every node this executor runs
    /// fails with `E0001`. That is the honest behaviour: the scheduler works, and
    /// there is nothing yet for it to schedule.
    pub struct PluginRegistryExecutor {
        registry: Registry,
    }

    impl PluginRegistryExecutor {
        /// Uses the plugins compiled into this build.
        pub fn new() -> Self {
            PluginRegistryExecutor {
                registry: Registry::builtin(),
            }
        }
    }

    impl Default for PluginRegistryExecutor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl NodeExecutor for PluginRegistryExecutor {
        fn execute(
            &mut self,
            node: &Node,
            ctx: &mut Context<'_>,
        ) -> Result<NodeResult, Diagnostic> {
            let Some(plugin) = self.registry.find(node.plugin()) else {
                return Err(Diagnostic::new(
                    "E6001",
                    Severity::Fatal,
                    FailureClass::ConfigurationError,
                    "core.scheduler",
                    format!("No plugin called '{}' is registered.", node.plugin()),
                )
                .with_context(format!("Node: {}", node.id()))
                .with_suggestion(
                    "The graph names a plugin this build does not contain. Either \
                     the graph or the plugin set is out of date.",
                ));
            };

            plugin.execute(ctx).map(|_| NodeResult::default())
        }
    }

    /// How a build ended.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Outcome {
        /// Every node produced its outputs.
        Completed,
        /// At least one node failed.
        Failed,
        /// The build was cancelled before it finished.
        Cancelled,
        /// The graph was rejected before any node ran.
        Rejected,
    }

    impl Outcome {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Outcome::Completed => "COMPLETED",
                Outcome::Failed => "FAILED",
                Outcome::Cancelled => "CANCELLED",
                Outcome::Rejected => "REJECTED",
            }
        }
    }

    /// What happened during a build.
    #[derive(Clone, Debug)]
    pub struct Report {
        /// How it ended.
        pub outcome: Outcome,
        /// Nodes that produced their outputs.
        pub succeeded: u64,
        /// Nodes whose results were reused.
        pub cache_hits: u64,
        /// Nodes that ran and failed.
        pub failed: u64,
        /// Nodes that did not run because a dependency failed.
        pub skipped: u64,
        /// Nodes that did not run because the build was cancelled.
        pub cancelled: u64,
        /// The order the scheduler chose, whether or not it got through it.
        pub order: Vec<NodeId>,
        /// Total time spent inside node execution, in microseconds.
        pub duration_micros: u64,
    }

    impl Report {
        /// Serialises the report as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_str("outcome", self.outcome.as_str());
            w.field_u64("succeeded", self.succeeded);
            w.field_u64("cacheHits", self.cache_hits);
            w.field_u64("failed", self.failed);
            w.field_u64("skipped", self.skipped);
            w.field_u64("cancelled", self.cancelled);
            w.field_u64("durationMicros", self.duration_micros);
            w.field_bool("parallel", false);
            w.begin_array(Some("order"));
            for id in &self.order {
                w.element_str(id.as_str());
            }
            w.end_array();
            w.end_object();
        }
    }

    /// Runs the graph.
    ///
    /// The plan is computed first, so a graph that cannot be executed is
    /// rejected before anything runs rather than part-way through. Cancellation
    /// is checked between nodes, which is the checkpoint directive section 35
    /// asks for; a node already running is left to finish, because stopping it
    /// mid-write is what atomic output exists to prevent.
    pub fn run(
        graph: &mut Graph,
        executor: &mut dyn NodeExecutor,
        policy: &mut Policy,
        sink: &mut Sink,
        cancellation: &Cancellation,
    ) -> Report {
        let order = match graph.plan() {
            Ok(order) => order,
            Err(error) => {
                sink.emit(error);
                return Report {
                    outcome: Outcome::Rejected,
                    succeeded: 0,
                    cache_hits: 0,
                    failed: 0,
                    skipped: 0,
                    cancelled: 0,
                    order: Vec::new(),
                    duration_micros: 0,
                };
            }
        };

        let mut report = Report {
            outcome: Outcome::Completed,
            succeeded: 0,
            cache_hits: 0,
            failed: 0,
            skipped: 0,
            cancelled: 0,
            order: order.clone(),
            duration_micros: 0,
        };

        for id in &order {
            if cancellation.is_cancelled() {
                graph.finish(
                    id,
                    Status::Cancelled,
                    Measurements::default(),
                    None,
                    Vec::new(),
                );
                report.cancelled += 1;
                report.outcome = Outcome::Cancelled;
                continue;
            }

            // The node is cloned so the graph can be written to while its work
            // runs. Nodes are small; the alternative is threading a borrow of the
            // graph through every executor, which would make an executor able to
            // rewrite the graph it is running inside.
            let Some(node) = graph.node(id).cloned() else {
                continue;
            };

            let blocked: Vec<String> = node
                .dependencies()
                .iter()
                .filter(|dependency| {
                    graph
                        .node(dependency)
                        .map(|d| !d.status().produced_its_outputs())
                        .unwrap_or(true)
                })
                .map(|dependency| dependency.as_str().to_string())
                .collect();

            if !blocked.is_empty() {
                // Directive section 10: a failed dependency is never ignored, and
                // a node that never ran is never called failed.
                sink.emit(
                    Diagnostic::new(
                        "W6002",
                        Severity::Warning,
                        FailureClass::Recoverable,
                        "core.scheduler",
                        format!("'{}' was skipped.", node.id()),
                    )
                    .with_context(format!("Waiting on: {}", blocked.join(", ")))
                    .with_suggestion(
                        "Fix what those nodes reported. This node has no problem of \
                         its own.",
                    ),
                );
                graph.finish(
                    id,
                    Status::Skipped,
                    Measurements::default(),
                    None,
                    Vec::new(),
                );
                report.skipped += 1;
                if report.outcome == Outcome::Completed {
                    report.outcome = Outcome::Failed;
                }
                continue;
            }

            graph.finish(
                id,
                Status::Running,
                Measurements::default(),
                None,
                Vec::new(),
            );

            let before = sink.len();
            let started = Instant::now();
            let result = {
                let mut ctx = Context {
                    policy,
                    diagnostics: sink,
                };
                executor.execute(&node, &mut ctx)
            };
            let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            report.duration_micros += elapsed;

            let codes: Vec<String> = sink.entries()[before..]
                .iter()
                .map(|d| d.code.clone())
                .collect();

            match result {
                Ok(node_result) => {
                    let status = if node_result.from_cache {
                        report.cache_hits += 1;
                        Status::CacheHit
                    } else {
                        report.succeeded += 1;
                        Status::Succeeded
                    };
                    graph.finish(
                        id,
                        status,
                        Measurements {
                            duration_micros: elapsed,
                            peak_memory_bytes: node_result.peak_memory_bytes,
                        },
                        node_result.artifact_digest,
                        codes,
                    );
                }
                Err(error) => {
                    let mut codes = codes;
                    codes.push(error.code.clone());
                    sink.emit(error);
                    graph.finish(
                        id,
                        Status::Failed,
                        Measurements {
                            duration_micros: elapsed,
                            peak_memory_bytes: 0,
                        },
                        None,
                        codes,
                    );
                    report.failed += 1;
                    if report.outcome == Outcome::Completed {
                        report.outcome = Outcome::Failed;
                    }
                }
            }
        }

        report
    }
}

// ===========================================================================
// report — the single source of truth the user interface renders
// ===========================================================================

/// Builds the state report the host displays.
///
/// Directive section 43 requires the user interface to render Core state rather
/// than imitate it: `Core State -> View Model -> UI`. This function *is* that
/// core state. Anything the interface shows that does not appear here would be
/// a fabrication.
///
/// The output is deterministic: it contains no timestamp, no locale-dependent
/// formatting and no random identifier (directive section 12).
pub fn state_report(observed_environment: &str) -> String {
    let mut sink = diag::Sink::new();
    let observation = toolchain::Observation::parse(observed_environment, &mut sink);
    let findings = toolchain::verify(&observation, &mut sink);
    let registry = plugin::Registry::builtin();

    let verified = findings
        .iter()
        .filter(|f| f.state == toolchain::State::Match)
        .count();
    let unverifiable = findings
        .iter()
        .filter(|f| f.state == toolchain::State::NotObservable)
        .count();

    let mut w = json::Writer::new();
    w.begin_object(None);

    w.begin_object(Some("core"));
    w.field_str("name", "Omni_Builder Core");
    w.field_str("version", CORE_VERSION);
    w.field_str("status", CORE_STATUS.as_str());
    w.field_str("phase", CORE_PHASE);
    w.field_u64("abiVersion", ffi::OMNI_ABI_VERSION as u64);
    w.field_bool("selfHosted", false);
    w.field_str(
        "selfHostingNote",
        "Omni_Builder is not self-hosted. Directive section 53 requires the whole \
         chain from source to signed artifact to run on Omni infrastructure; none \
         of it does yet.",
    );
    w.begin_array(Some("bootstrapDependencies"));
    for dependency in BOOTSTRAP_DEPENDENCIES {
        w.element_str(dependency);
    }
    w.end_array();
    w.end_object();

    w.begin_object(Some("subsystems"));
    w.field_u64("count", SUBSYSTEMS.len() as u64);
    w.field_u64(
        "production",
        SUBSYSTEMS
            .iter()
            .filter(|s| s.status == Status::Production)
            .count() as u64,
    );
    w.begin_array(Some("detail"));
    for subsystem in SUBSYSTEMS {
        w.begin_object(None);
        w.field_str("name", subsystem.name);
        w.field_str("status", subsystem.status.as_str());
        w.field_u64("directiveSection", subsystem.directive_section as u64);
        w.field_str("summary", subsystem.summary);
        w.begin_array(Some("missing"));
        for gap in subsystem.missing {
            w.element_str(gap);
        }
        w.end_array();
        w.end_object();
    }
    w.end_array();
    w.end_object();

    w.begin_object(Some("toolchain"));
    w.field_u64("pinnedComponents", toolchain::LOCK.len() as u64);
    w.field_u64("verified", verified as u64);
    w.field_u64("notObservableHere", unverifiable as u64);
    w.field_bool("dynamicVersionsUsed", false);
    toolchain::write_json(&findings, &mut w, "components");
    w.end_object();

    w.begin_object(Some("capabilityModel"));
    w.field_str("default", "DENY");
    w.field_str(
        "note",
        "No plugin holds a capability implicitly. Every request is evaluated by a \
         policy and recorded in an audit trail.",
    );
    w.begin_array(Some("capabilities"));
    for capability in caps::Capability::ALL {
        w.begin_object(None);
        w.field_str("name", capability.as_str());
        w.field_bool("sensitive", capability.is_sensitive());
        w.end_object();
    }
    w.end_array();
    w.end_object();

    w.begin_object(Some("plugins"));
    w.field_u64("count", registry.len() as u64);
    w.field_u64(
        "implemented",
        registry
            .all()
            .iter()
            .filter(|p| p.contract().status.may_produce_artifacts())
            .count() as u64,
    );
    registry.write_json(&mut w, "contracts");
    w.end_object();

    sink.write_json(&mut w, "diagnostics");
    w.begin_object(Some("diagnosticSummary"));
    w.field_u64("count", sink.len() as u64);
    w.field_bool("blocking", sink.has_blocking());
    match sink.max_severity() {
        Some(severity) => w.field_str("maxSeverity", severity.as_str()),
        None => w.field_str("maxSeverity", "NONE"),
    }
    w.end_object();

    w.end_object();
    w.finish()
}

// ===========================================================================
// ffi — the C ABI consumed by Builder/Source/Main/Native/Builder.cpp (ADR-0004)
// ===========================================================================

/// Stable C ABI boundary.
///
/// ## Contract
///
/// * No function here unwinds. Every entry point is wrapped in
///   [`std::panic::catch_unwind`]; a panic becomes a null return, never
///   undefined behaviour.
/// * Every pointer this module returns from an `omni_*_new`-style call must be
///   released with [`omni_string_free`], and with nothing else.
/// * [`omni_core_version`] returns a pointer with static lifetime that must
///   **not** be freed.
/// * The Core never retains a pointer the caller passed in.
pub mod ffi {
    use std::ffi::{c_char, CStr, CString};
    use std::panic::catch_unwind;

    /// Version of this ABI.
    ///
    /// The C++ bridge checks this at load time and refuses to run against a Core
    /// it was not compiled for (directive section 65).
    pub const OMNI_ABI_VERSION: u32 = 1;

    /// Returns the ABI version the Core was built with.
    ///
    /// Never fails.
    #[no_mangle]
    pub extern "C" fn omni_abi_version() -> u32 {
        OMNI_ABI_VERSION
    }

    /// Returns the Core version as a NUL-terminated string with static lifetime.
    ///
    /// # Safety for the caller
    ///
    /// The returned pointer is valid for the lifetime of the process and must
    /// not be passed to [`omni_string_free`] or to `free`.
    #[no_mangle]
    pub extern "C" fn omni_core_version() -> *const c_char {
        const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
        VERSION.as_ptr() as *const c_char
    }

    /// Builds the full state report as a NUL-terminated JSON document.
    ///
    /// `observed_environment` may be null, which is treated as "nothing
    /// observed". Otherwise it must point to a NUL-terminated string; invalid
    /// UTF-8 is rejected rather than repaired.
    ///
    /// Returns null on failure. Ownership of a non-null result transfers to the
    /// caller, which must release it with [`omni_string_free`].
    ///
    /// # Safety
    ///
    /// `observed_environment` must be null or point to a valid NUL-terminated
    /// C string that stays valid for the duration of the call.
    #[no_mangle]
    pub unsafe extern "C" fn omni_state_report(observed_environment: *const c_char) -> *mut c_char {
        let result = catch_unwind(|| {
            let observed = if observed_environment.is_null() {
                String::new()
            } else {
                // SAFETY: the caller guarantees a valid NUL-terminated string.
                match unsafe { CStr::from_ptr(observed_environment) }.to_str() {
                    Ok(text) => text.to_string(),
                    Err(_) => return std::ptr::null_mut(),
                }
            };

            match CString::new(crate::state_report(&observed)) {
                Ok(report) => report.into_raw(),
                // The report is generated by the Core and cannot contain an
                // interior NUL, but the ABI stays total rather than asserting.
                Err(_) => std::ptr::null_mut(),
            }
        });

        result.unwrap_or(std::ptr::null_mut())
    }

    /// Releases a string previously returned by [`omni_state_report`].
    ///
    /// Passing null is allowed and does nothing.
    ///
    /// # Safety
    ///
    /// `value` must be null or a pointer returned by [`omni_state_report`] that
    /// has not already been released. Passing any other pointer is undefined
    /// behaviour.
    #[no_mangle]
    pub unsafe extern "C" fn omni_string_free(value: *mut c_char) {
        if value.is_null() {
            return;
        }
        // SAFETY: the caller guarantees this pointer came from CString::into_raw
        // and has not been released. Dropping it returns the allocation to the
        // same allocator that produced it.
        let _ = catch_unwind(|| unsafe {
            drop(CString::from_raw(value));
        });
    }
}

// ===========================================================================
// Tests (directive sections 40 and 51)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::artifact::{Artifact, ArtifactId, State as ArtifactState};
    use super::cache::{Index as CacheIndex, Inputs as CacheInputs, Lookup as CacheLookup};
    use super::caps::{Capability, Decision, Policy};
    use super::diag::{Diagnostic, Location, Severity, Sink};
    use super::graph::{Graph, Kind as NodeKind, Node, NodeId, Status as NodeStatus};
    use super::hash::Digest;
    use super::json::Writer;
    use super::plugin::{Registry, Version};
    use super::project::{parse_manifest, GuardLevel, Optimization, Profile, Project};
    use super::scheduler::{Cancellation, Outcome as SchedulerOutcome};
    use super::toolchain::{self, Observation, Requirement, State};
    use super::vfs::{Access, Quota, VirtualFs, VirtualPath};
    use super::{FailureClass, Status};

    /// Structural check that a document is balanced and quotes are terminated.
    ///
    /// The Core has no JSON parser by design (ADR-0003), so the tests verify the
    /// writer's output structurally rather than by round-tripping it.
    fn is_structurally_valid(document: &str) -> bool {
        let mut stack: Vec<char> = Vec::new();
        let mut in_string = false;
        let mut escaped = false;

        for ch in document.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                } else if (ch as u32) < 0x20 {
                    return false; // raw control character inside a string
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' => stack.push('}'),
                '[' => stack.push(']'),
                '}' | ']' if stack.pop() != Some(ch) => return false,
                _ => {}
            }
        }

        stack.is_empty() && !in_string
    }

    // --- json ---------------------------------------------------------------

    #[test]
    fn json_escapes_every_character_that_would_break_a_document() {
        let mut w = Writer::new();
        w.begin_object(None);
        w.field_str("quote\"key", "line1\nline2\ttab\\slash\r\u{08}\u{0C}\u{01}");
        w.end_object();
        let document = w.finish();

        assert!(document.contains("\\\"key"));
        assert!(document.contains("\\n"));
        assert!(document.contains("\\t"));
        assert!(document.contains("\\\\"));
        assert!(document.contains("\\r"));
        assert!(document.contains("\\b"));
        assert!(document.contains("\\f"));
        assert!(document.contains("\\u0001"));
        assert!(is_structurally_valid(&document), "document: {document}");
    }

    #[test]
    fn json_separates_members_and_nests_containers() {
        let mut w = Writer::new();
        w.begin_object(None);
        w.field_str("a", "1");
        w.field_u64("b", 2);
        w.field_bool("c", true);
        w.begin_array(Some("d"));
        w.element_str("x");
        w.element_str("y");
        w.begin_object(None);
        w.field_str("nested", "value");
        w.end_object();
        w.end_array();
        w.end_object();

        assert_eq!(
            w.finish(),
            r#"{"a":"1","b":2,"c":true,"d":["x","y",{"nested":"value"}]}"#
        );
    }

    #[test]
    fn json_writer_keeps_unicode_intact() {
        let mut w = Writer::new();
        w.begin_object(None);
        w.field_str("türkçe", "ş ğ ı İ ö ü ç — 🚀");
        w.end_object();
        let document = w.finish();
        assert!(document.contains("ş ğ ı İ ö ü ç — 🚀"));
        assert!(is_structurally_valid(&document));
    }

    #[test]
    #[should_panic(expected = "unclosed container")]
    fn json_writer_refuses_to_finish_an_unbalanced_document() {
        let mut w = Writer::new();
        w.begin_object(None);
        let _ = w.finish();
    }

    // --- diagnostics --------------------------------------------------------

    #[test]
    fn diagnostic_renders_the_form_required_by_the_directive() {
        let diagnostic = Diagnostic::new(
            "E1004",
            Severity::Error,
            FailureClass::UserError,
            "plugin.kotlin",
            "Type mismatch.",
        )
        .with_location(Location::at("Main.kt", 24, 17))
        .with_context("Expected: Int")
        .with_context("Found: String")
        .with_suggestion("Convert the expression to Int.");

        let rendered = diagnostic.to_string();
        assert!(rendered.starts_with("E1004 [ERROR] Main.kt:24:17"));
        assert!(rendered.contains("Expected: Int"));
        assert!(rendered.contains("Found: String"));
        assert!(rendered.contains("Suggestion: Convert the expression to Int."));
    }

    #[test]
    fn location_display_degrades_gracefully() {
        assert_eq!(Location::file("A.kt").to_string(), "A.kt");
        assert_eq!(Location::at("A.kt", 3, 0).to_string(), "A.kt:3");
        assert_eq!(Location::at("A.kt", 3, 9).to_string(), "A.kt:3:9");
    }

    #[test]
    fn sink_reports_blocking_state_and_preserves_order() {
        let mut sink = Sink::new();
        assert!(sink.is_empty());
        assert!(!sink.has_blocking());
        assert_eq!(sink.max_severity(), None);

        sink.emit(Diagnostic::new(
            "I0001",
            Severity::Info,
            FailureClass::Success,
            "core",
            "first",
        ));
        assert!(!sink.has_blocking());

        sink.emit(Diagnostic::new(
            "E0002",
            Severity::Error,
            FailureClass::InternalError,
            "core",
            "second",
        ));
        assert!(sink.has_blocking());
        assert_eq!(sink.max_severity(), Some(Severity::Error));
        assert_eq!(sink.entries()[0].code, "I0001");
        assert_eq!(sink.entries()[1].code, "E0002");
    }

    #[test]
    fn severity_blocking_matches_the_failure_model() {
        assert!(!Severity::Trace.is_blocking());
        assert!(!Severity::Info.is_blocking());
        assert!(!Severity::Warning.is_blocking());
        assert!(Severity::Error.is_blocking());
        assert!(Severity::Fatal.is_blocking());
    }

    // --- capability security ------------------------------------------------

    #[test]
    fn a_fresh_policy_grants_nothing() {
        let mut policy = Policy::new("test");
        for capability in Capability::ALL {
            assert_eq!(
                policy.request("plugin.test", *capability),
                Decision::Deny,
                "{capability} must not be granted by default"
            );
        }
        assert!(policy.granted().is_empty());
    }

    #[test]
    fn granting_is_explicit_idempotent_and_revocable() {
        let mut policy = Policy::new("test");
        policy.grant(Capability::FsRead).grant(Capability::FsRead);
        assert_eq!(policy.granted(), &[Capability::FsRead]);
        assert_eq!(policy.request("p", Capability::FsRead), Decision::Grant);
        assert_eq!(policy.request("p", Capability::FsWrite), Decision::Deny);

        policy.revoke(Capability::FsRead);
        assert_eq!(policy.request("p", Capability::FsRead), Decision::Deny);
    }

    #[test]
    fn every_request_leaves_an_audit_record() {
        let mut policy = Policy::new("test");
        policy.grant(Capability::Cache);
        policy.request("plugin.dex", Capability::Cache);
        policy.request("plugin.dex", Capability::Network);

        let audit = policy.audit();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].decision, Decision::Grant);
        assert_eq!(audit[0].capability, Capability::Cache);
        assert_eq!(audit[1].decision, Decision::Deny);
        assert_eq!(audit[1].subject, "plugin.dex");
        assert!(audit[1].reason.contains("default deny"));
    }

    #[test]
    fn audit_records_never_carry_a_payload() {
        // The audit type has exactly four fields, none of which can hold key
        // material. This test exists so that adding one is a deliberate act.
        let mut policy = Policy::new("test");
        policy.request("p", Capability::KeyAccess);
        let record = &policy.audit()[0];
        assert!(!record.reason.is_empty());
        assert!(record.capability.is_sensitive());
    }

    #[test]
    fn sensitive_capabilities_are_marked() {
        for capability in [
            Capability::KeyAccess,
            Capability::Crypto,
            Capability::SensitiveOutput,
            Capability::ProcessExec,
            Capability::Network,
            Capability::Internet,
        ] {
            assert!(capability.is_sensitive(), "{capability}");
        }
        assert!(!Capability::Cache.is_sensitive());
    }

    #[test]
    fn capability_names_match_the_directive() {
        assert_eq!(Capability::FsRead.as_str(), "FS_READ");
        assert_eq!(Capability::ProcessExec.as_str(), "PROCESS_EXEC");
        assert_eq!(Capability::SensitiveOutput.as_str(), "SENSITIVE_OUTPUT");
        assert_eq!(Capability::ALL.len(), 13);
    }

    // --- plugin registry ----------------------------------------------------

    #[test]
    fn the_registry_holds_exactly_the_nine_declared_plugins() {
        let registry = Registry::builtin();
        assert_eq!(registry.len(), 9);

        let expected = [
            "omni.plugin.kotlin",
            "omni.plugin.java",
            "omni.plugin.cpp",
            "omni.plugin.rust",
            "omni.plugin.resources",
            "omni.plugin.dex",
            "omni.plugin.apk",
            "omni.plugin.sign",
            "omni.plugin.guard",
        ];
        let actual: Vec<&str> = registry.all().iter().map(|p| p.contract().id).collect();
        assert_eq!(actual, expected, "registry order must be stable");
    }

    #[test]
    fn plugin_identifiers_are_unique() {
        let registry = Registry::builtin();
        let mut ids: Vec<&str> = registry.all().iter().map(|p| p.contract().id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[test]
    fn no_plugin_claims_to_be_implemented() {
        // Directive section 1. This test is the mechanical guard against a
        // subsystem being marked finished before it is.
        for plugin in Registry::builtin().all() {
            let contract = plugin.contract();
            assert_eq!(
                contract.status,
                Status::Planned,
                "{} claims status {}",
                contract.id,
                contract.status
            );
            assert!(
                !contract.status.may_produce_artifacts(),
                "{} must not be allowed to publish artifacts",
                contract.id
            );
        }
    }

    #[test]
    fn every_plugin_refuses_to_execute_and_says_why() {
        let mut policy = Policy::new("test");
        for plugin in Registry::builtin().all() {
            let mut sink = Sink::new();
            let mut ctx = super::plugin::Context {
                policy: &mut policy,
                diagnostics: &mut sink,
            };
            let error = plugin
                .execute(&mut ctx)
                .expect_err("a PLANNED plugin must not report success");
            assert_eq!(error.code, "E0001");
            assert_eq!(error.severity, Severity::Error);
            assert_eq!(error.origin, plugin.contract().id);
            assert!(error.suggestion.is_some());
        }
    }

    #[test]
    fn every_plugin_declares_a_non_empty_contract() {
        for plugin in Registry::builtin().all() {
            let contract = plugin.contract();
            assert!(!contract.summary.is_empty(), "{}", contract.id);
            assert!(!contract.outputs.is_empty(), "{}", contract.id);
            assert!(
                !contract.non_responsibilities.is_empty(),
                "{} must state what it does not do",
                contract.id
            );
            assert!(!contract.roadmap_phase.is_empty(), "{}", contract.id);
        }
    }

    #[test]
    fn no_plugin_requires_a_capability_it_also_forbids() {
        for plugin in Registry::builtin().all() {
            let contract = plugin.contract();
            for required in contract.required_capabilities {
                assert!(
                    !contract.forbidden_capabilities.contains(required),
                    "{} both requires and forbids {}",
                    contract.id,
                    required
                );
            }
        }
    }

    #[test]
    fn no_plugin_requests_internet_access() {
        // Directive section 7: a compiler plugin has no business reaching the
        // network. Relaxing this must be a deliberate, reviewed change.
        for plugin in Registry::builtin().all() {
            let contract = plugin.contract();
            for capability in [Capability::Network, Capability::Internet] {
                assert!(
                    !contract.required_capabilities.contains(&capability),
                    "{} requires {}",
                    contract.id,
                    capability
                );
            }
        }
    }

    #[test]
    fn registry_lookup_finds_declared_plugins_only() {
        let registry = Registry::builtin();
        assert!(registry.find("omni.plugin.dex").is_some());
        assert!(registry.find("omni.plugin.does-not-exist").is_none());
    }

    #[test]
    fn plugin_version_compatibility_follows_semver_rules() {
        let required = Version::new(1, 2, 3);
        assert!(Version::new(1, 2, 3).is_compatible_with(required));
        assert!(Version::new(1, 3, 0).is_compatible_with(required));
        assert!(Version::new(1, 2, 4).is_compatible_with(required));
        assert!(!Version::new(1, 2, 2).is_compatible_with(required));
        assert!(!Version::new(2, 0, 0).is_compatible_with(required));
        assert!(!Version::new(0, 9, 9).is_compatible_with(required));
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    }

    // --- toolchain lock -----------------------------------------------------

    #[test]
    fn the_lock_contains_no_dynamic_version() {
        // Directive section 14 forbids `latest`, `9.+` and `*`.
        for pin in toolchain::LOCK {
            let pinned = pin.pinned;
            assert!(!pinned.is_empty(), "{}", pin.id);
            assert!(!pinned.contains('+'), "{} uses a dynamic version", pin.id);
            assert!(!pinned.contains('*'), "{} uses a dynamic version", pin.id);
            assert!(
                !pinned.eq_ignore_ascii_case("latest"),
                "{} uses a dynamic version",
                pin.id
            );
            assert!(!pin.source.is_empty(), "{} has no provenance", pin.id);
            assert!(!pin.note.is_empty(), "{} has no note", pin.id);
        }
    }

    #[test]
    fn the_lock_covers_every_component_named_by_the_directive() {
        let ids: Vec<&str> = toolchain::LOCK.iter().map(|p| p.id).collect();
        for required in [
            "jdk",
            "gradle",
            "agp",
            "kotlin",
            "rust",
            "ndk",
            "androidApi",
            "buildTools",
            "cmake",
            "minSdk",
            "targetSdk",
        ] {
            assert!(ids.contains(&required), "missing pin: {required}");
        }
    }

    #[test]
    fn lock_identifiers_are_unique() {
        let mut ids: Vec<&str> = toolchain::LOCK.iter().map(|p| p.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[test]
    fn requirement_matching_is_strict_where_it_must_be() {
        assert!(Requirement::Exact.satisfied_by("9.7.0", "9.7.0"));
        assert!(Requirement::Exact.satisfied_by("9.7.0", " 9.7.0 "));
        assert!(!Requirement::Exact.satisfied_by("9.7.0", "9.7.1"));
        assert!(!Requirement::Exact.satisfied_by("9.7.0", "9.7"));

        assert!(Requirement::Series.satisfied_by("4", "4.1.2"));
        assert!(Requirement::Series.satisfied_by("25", "25.0.1"));
        assert!(!Requirement::Series.satisfied_by("4", "3.28.3"));
        assert!(!Requirement::Series.satisfied_by("4", ""));
    }

    #[test]
    fn a_matching_observation_produces_no_diagnostic_for_that_pin() {
        let mut sink = Sink::new();
        let observation = Observation::parse("minSdk=28;targetSdk=36", &mut sink);
        let findings = toolchain::verify(&observation, &mut sink);

        let min_sdk = findings.iter().find(|f| f.pin.id == "minSdk").unwrap();
        assert_eq!(min_sdk.state, State::Match);
        assert!(!sink
            .entries()
            .iter()
            .any(|d| d.code == "E1001" && d.message.contains("minSdk")));
    }

    #[test]
    fn a_mismatched_observation_is_an_error_with_both_versions() {
        let mut sink = Sink::new();
        let observation = Observation::parse("minSdk=21", &mut sink);
        let findings = toolchain::verify(&observation, &mut sink);

        let min_sdk = findings.iter().find(|f| f.pin.id == "minSdk").unwrap();
        assert_eq!(min_sdk.state, State::Mismatch);

        let diagnostic = sink
            .entries()
            .iter()
            .find(|d| d.code == "E1001")
            .expect("a mismatch must be reported");
        assert!(diagnostic
            .context
            .iter()
            .any(|c| c.contains("Expected: 28")));
        assert!(diagnostic.context.iter().any(|c| c.contains("Found: 21")));
        assert!(diagnostic.suggestion.is_some());
    }

    #[test]
    fn host_only_components_are_reported_as_unverifiable_not_as_verified() {
        // Directive section 15: bootstrap dependencies must be visible, and the
        // device cannot observe them. Claiming a match here would be a lie.
        let mut sink = Sink::new();
        let observation = Observation::parse("", &mut sink);
        let findings = toolchain::verify(&observation, &mut sink);

        let gradle = findings.iter().find(|f| f.pin.id == "gradle").unwrap();
        assert_eq!(gradle.state, State::NotObservable);
        assert!(gradle.observed.is_none());

        let min_sdk = findings.iter().find(|f| f.pin.id == "minSdk").unwrap();
        assert_eq!(min_sdk.state, State::Missing);
    }

    #[test]
    fn observation_rejects_an_oversized_input() {
        let mut sink = Sink::new();
        let huge = "minSdk=28;".repeat(toolchain::MAX_OBSERVATION_BYTES);
        let observation = Observation::parse(&huge, &mut sink);
        assert!(observation.is_empty());
        assert!(sink.entries().iter().any(|d| d.code == "E1101"));
    }

    #[test]
    fn observation_bounds_the_number_of_entries() {
        let mut sink = Sink::new();
        let many: String = (0..toolchain::MAX_OBSERVATION_PAIRS + 20)
            .map(|i| format!("key{i}=v;"))
            .collect();
        let observation = Observation::parse(&many, &mut sink);
        assert_eq!(observation.len(), toolchain::MAX_OBSERVATION_PAIRS);
        assert!(sink.entries().iter().any(|d| d.code == "E1102"));
    }

    #[test]
    fn observation_bounds_a_single_value() {
        let mut sink = Sink::new();
        let long = "x".repeat(toolchain::MAX_OBSERVED_VALUE_BYTES + 1);
        let observation = Observation::parse(&format!("gradle={long}"), &mut sink);
        assert!(observation.get("gradle").is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E1105"));
    }

    #[test]
    fn observation_reports_malformed_entries_instead_of_dropping_them() {
        let mut sink = Sink::new();
        let observation = Observation::parse("not-a-pair;=novalue;minSdk=28", &mut sink);
        assert_eq!(observation.get("minSdk"), Some("28"));
        assert!(sink.entries().iter().any(|d| d.code == "W1103"));
        assert!(sink.entries().iter().any(|d| d.code == "W1104"));
    }

    #[test]
    fn observation_reports_a_repeated_key_and_keeps_the_first_value() {
        let mut sink = Sink::new();
        let observation = Observation::parse("minSdk=28;minSdk=21", &mut sink);
        assert_eq!(observation.get("minSdk"), Some("28"));
        assert!(sink.entries().iter().any(|d| d.code == "W1106"));
    }

    #[test]
    fn observation_reports_unknown_keys_rather_than_ignoring_them() {
        // Directive section 44: unknown critical fields must not be silently
        // discarded.
        let mut sink = Sink::new();
        let _ = Observation::parse("somethingElse=1", &mut sink);
        assert!(sink.entries().iter().any(|d| d.code == "W1107"));
    }

    #[test]
    fn observation_tolerates_newline_separators_and_blank_entries() {
        let mut sink = Sink::new();
        let observation = Observation::parse("minSdk=28\n\n;targetSdk=36\n", &mut sink);
        assert_eq!(observation.get("minSdk"), Some("28"));
        assert_eq!(observation.get("targetSdk"), Some("36"));
    }

    // --- state report -------------------------------------------------------

    #[test]
    fn the_state_report_is_structurally_valid_json() {
        let report = super::state_report("minSdk=28;targetSdk=36");
        assert!(is_structurally_valid(&report), "report: {report}");
    }

    #[test]
    fn the_state_report_is_deterministic() {
        // Directive section 12: identical input, identical bytes.
        let a = super::state_report("minSdk=28;targetSdk=36");
        let b = super::state_report("minSdk=28;targetSdk=36");
        assert_eq!(a, b);
    }

    #[test]
    fn the_state_report_never_claims_self_hosting() {
        let report = super::state_report("");
        assert!(report.contains("\"selfHosted\":false"));
        assert!(report.contains("\"implemented\":0"));
        assert!(report.contains("Gradle"));
    }

    #[test]
    fn the_state_report_survives_hostile_input() {
        // Directive section 41: malformed input must not crash, hang or allocate
        // without bound.
        for input in [
            "",
            "=",
            ";;;;;;",
            "\0",
            "minSdk=\"};DROP",
            "minSdk=28;minSdk=28;minSdk=28",
            "a=\n\r\t\u{1}\u{7f}",
            "🚀=🚀",
        ] {
            let report = super::state_report(input);
            assert!(
                is_structurally_valid(&report),
                "input {input:?} -> {report}"
            );
        }
    }

    #[test]
    fn the_state_report_lists_every_plugin_and_every_pin() {
        let report = super::state_report("");
        for plugin in Registry::builtin().all() {
            assert!(report.contains(plugin.contract().id));
        }
        for pin in toolchain::LOCK {
            assert!(report.contains(pin.id), "missing pin in report: {}", pin.id);
        }
    }

    // --- subsystem inventory -------------------------------------------------

    #[test]
    fn no_subsystem_claims_to_be_finished() {
        // Directive section 1. The gates of section 51 decide when this may
        // change, and this test is what makes that a decision rather than an
        // oversight.
        for subsystem in super::SUBSYSTEMS {
            assert_ne!(
                subsystem.status,
                Status::Production,
                "{} claims PRODUCTION",
                subsystem.name
            );
        }
    }

    #[test]
    fn every_subsystem_states_what_is_missing() {
        for subsystem in super::SUBSYSTEMS {
            assert!(!subsystem.name.is_empty());
            assert!(!subsystem.summary.is_empty(), "{}", subsystem.name);
            assert!(subsystem.directive_section > 0, "{}", subsystem.name);

            // Anything short of BETA has unfinished work by definition, and
            // saying so is the whole point of the table.
            if subsystem.status < Status::Beta {
                assert!(
                    !subsystem.missing.is_empty(),
                    "{} is {} but lists nothing missing",
                    subsystem.name,
                    subsystem.status
                );
            }
        }
    }

    #[test]
    fn the_report_carries_the_subsystem_inventory() {
        let report = super::state_report("");
        assert!(is_structurally_valid(&report));
        assert!(report.contains("\"production\":0"));
        for subsystem in super::SUBSYSTEMS {
            assert!(
                report.contains(subsystem.name),
                "missing: {}",
                subsystem.name
            );
        }
    }

    // --- artifact lifecycle --------------------------------------------------

    fn artifact_named(name: &str) -> Artifact {
        Artifact::created(
            ArtifactId::new(name).unwrap(),
            "dex",
            VirtualPath::parse("build/classes.dex").unwrap(),
        )
    }

    #[test]
    fn artifact_identifiers_are_restricted_to_what_reports_can_carry() {
        assert!(ArtifactId::new("dex.classes").is_ok());
        assert!(ArtifactId::new("apk-unsigned_1").is_ok());
        for bad in ["", "has space", "has/slash", "has\"quote", &"x".repeat(129)] {
            assert_eq!(ArtifactId::new(bad).unwrap_err().code, "E5001", "{bad:?}");
        }
    }

    #[test]
    fn an_artifact_walks_the_lifecycle_the_directive_defines() {
        let mut artifact = artifact_named("dex.classes");
        assert_eq!(artifact.state(), ArtifactState::Created);

        let digest = super::hash::sha256(b"dex bytes");
        artifact.hashed(digest, 9).unwrap();
        assert_eq!(artifact.state(), ArtifactState::Hashed);
        assert_eq!(artifact.digest(), Some(digest));
        assert_eq!(artifact.size(), 9);

        artifact.validated().unwrap();
        artifact.signed().unwrap();
        artifact.verified(digest).unwrap();
        artifact.published().unwrap();

        assert_eq!(artifact.state(), ArtifactState::Published);
        assert_eq!(
            artifact.history(),
            [
                ArtifactState::Created,
                ArtifactState::Hashed,
                ArtifactState::Validated,
                ArtifactState::Signed,
                ArtifactState::Verified,
                ArtifactState::Published,
            ]
        );
    }

    #[test]
    fn signing_is_optional_but_every_other_step_is_not() {
        let mut artifact = artifact_named("apk.unsigned");
        let digest = super::hash::sha256(b"apk");
        artifact.hashed(digest, 3).unwrap();
        artifact.validated().unwrap();
        artifact.verified(digest).unwrap();
        artifact.published().unwrap();
        assert_eq!(artifact.state(), ArtifactState::Published);
    }

    #[test]
    fn an_unverified_artifact_cannot_be_published() {
        // Directive section 58, in one sentence: an invalid artifact cannot be
        // published. This is where that sentence is enforced.
        let mut artifact = artifact_named("apk.release");
        assert_eq!(artifact.published().unwrap_err().code, "E5012");

        let digest = super::hash::sha256(b"apk");
        artifact.hashed(digest, 3).unwrap();
        assert_eq!(artifact.published().unwrap_err().code, "E5012");

        artifact.validated().unwrap();
        assert_eq!(artifact.published().unwrap_err().code, "E5012");
        assert_eq!(artifact.state(), ArtifactState::Validated);
    }

    #[test]
    fn steps_cannot_be_skipped_or_repeated() {
        let mut artifact = artifact_named("a.b");
        assert_eq!(artifact.validated().unwrap_err().code, "E5012");

        let digest = super::hash::sha256(b"x");
        artifact.hashed(digest, 1).unwrap();
        assert_eq!(artifact.hashed(digest, 1).unwrap_err().code, "E5012");
    }

    #[test]
    fn verification_actually_compares_the_digest() {
        // Invariant I6: an artifact is verified before use. A verification that
        // compares nothing would satisfy the letter of that and none of its
        // point.
        let mut artifact = artifact_named("a.b");
        let original = super::hash::sha256(b"original");
        artifact.hashed(original, 8).unwrap();
        artifact.validated().unwrap();

        let tampered = super::hash::sha256(b"tampered");
        let error = artifact.verified(tampered).unwrap_err();
        assert_eq!(error.code, "E5011");
        assert_eq!(error.class, FailureClass::Corruption);
        assert_eq!(error.severity, Severity::Fatal);
        assert_eq!(artifact.state(), ArtifactState::Validated);

        artifact.verified(original).unwrap();
        assert_eq!(artifact.state(), ArtifactState::Verified);
    }

    #[test]
    fn an_artifact_without_a_digest_cannot_be_verified() {
        // Reaching VALIDATED without a digest is impossible by construction, so
        // the guard is checked on a fresh artifact. Both the missing digest and
        // the illegal transition apply here; the diagnostic reports the missing
        // digest, because "hash it first" is the actionable half.
        let mut artifact = artifact_named("a.b");
        let error = artifact.verified(super::hash::sha256(b"x")).unwrap_err();
        assert_eq!(error.code, "E5010");
        assert!(error.suggestion.as_deref().unwrap().contains("Hash it"));
        assert_eq!(artifact.state(), ArtifactState::Created);
    }

    // --- cache ---------------------------------------------------------------

    fn cache_inputs<'a>(source: &'a Digest, dependencies: &'a Digest) -> CacheInputs<'a> {
        CacheInputs {
            source_digest: *source,
            dependency_digest: *dependencies,
            plugin_version: "0.1.0",
            compiler_version: "2.4.10",
            toolchain_version: "9.7.0",
            target_sdk: 36,
            min_sdk: 28,
            abi: "arm64-v8a",
            profile: Profile::Release,
            optimization: Optimization::Size,
            feature_configuration: "Compose=true",
            relevant_environment: &[],
            security_policy: "default",
        }
    }

    #[test]
    fn a_cache_key_changes_when_any_declared_input_changes() {
        // Directive section 11 lists what a key must cover. Each of these is one
        // of those inputs, and each must move the key.
        let source = super::hash::sha256(b"source");
        let dependencies = super::hash::sha256(b"deps");
        let base = cache_inputs(&source, &dependencies).key();

        let other = super::hash::sha256(b"other");
        let mutations: Vec<CacheInputs> = vec![
            CacheInputs {
                source_digest: other,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                dependency_digest: other,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                plugin_version: "0.2.0",
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                compiler_version: "2.4.11",
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                toolchain_version: "9.7.1",
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                target_sdk: 35,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                min_sdk: 29,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                abi: "x86_64",
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                profile: Profile::Debug,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                optimization: Optimization::Speed,
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                feature_configuration: "Compose=false",
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                relevant_environment: &[("OMNI_X", "1")],
                ..cache_inputs(&source, &dependencies)
            },
            CacheInputs {
                security_policy: "strict",
                ..cache_inputs(&source, &dependencies)
            },
        ];

        for (index, mutated) in mutations.iter().enumerate() {
            assert_ne!(base, mutated.key(), "input {index} did not affect the key");
        }
    }

    #[test]
    fn a_cache_key_is_stable_for_the_same_inputs() {
        let source = super::hash::sha256(b"source");
        let dependencies = super::hash::sha256(b"deps");
        assert_eq!(
            cache_inputs(&source, &dependencies).key(),
            cache_inputs(&source, &dependencies).key()
        );
        assert_eq!(
            cache_inputs(&source, &dependencies).key().to_hex().len(),
            64
        );
    }

    #[test]
    fn the_four_cache_outcomes_stay_distinguishable() {
        // Directive section 11 requires them to be told apart, and section 11's
        // last line forbids treating a corrupt entry as usable.
        let source = super::hash::sha256(b"source");
        let dependencies = super::hash::sha256(b"deps");
        let key = cache_inputs(&source, &dependencies).key();
        let stored = super::hash::sha256(b"result");

        let mut index = CacheIndex::new();
        assert_eq!(index.lookup(key, Some(stored)), CacheLookup::Miss);

        index.store(key, stored);
        assert_eq!(index.lookup(key, Some(stored)), CacheLookup::Hit);
        assert_eq!(
            index.lookup(key, Some(super::hash::sha256(b"changed"))),
            CacheLookup::Corrupted
        );

        index.invalidate(key);
        assert_eq!(index.lookup(key, Some(stored)), CacheLookup::Invalidated);

        let statistics = index.statistics();
        assert_eq!(statistics.misses, 1);
        assert_eq!(statistics.hits, 1);
        assert_eq!(statistics.corrupted, 1);
        assert_eq!(statistics.invalidated, 1);

        assert!(CacheLookup::Hit.is_usable());
        for unusable in [
            CacheLookup::Miss,
            CacheLookup::Invalidated,
            CacheLookup::Corrupted,
        ] {
            assert!(!unusable.is_usable(), "{unusable:?}");
        }
    }

    // --- build graph ---------------------------------------------------------

    fn digests() -> [Digest; 4] {
        [
            super::hash::sha256(b"input"),
            super::hash::sha256(b"configuration"),
            super::hash::sha256(b"toolchain"),
            super::hash::sha256(b"plugin"),
        ]
    }

    fn node(id: &str, kind: NodeKind) -> Node {
        Node::new(NodeId::new(id).unwrap(), kind, "omni.plugin.dex", digests())
    }

    #[test]
    fn a_graph_orders_work_by_its_edges_not_by_insertion() {
        // Directive section 9: a build system is not a file ordering.
        let mut graph = Graph::new();
        graph
            .add(node("package", NodeKind::Package).after(NodeId::new("dex").unwrap()))
            .unwrap();
        graph
            .add(node("dex", NodeKind::Dex).after(NodeId::new("compile").unwrap()))
            .unwrap();
        graph.add(node("compile", NodeKind::CompilerIr)).unwrap();

        let order: Vec<String> = graph
            .plan()
            .unwrap()
            .iter()
            .map(|id| id.as_str().to_string())
            .collect();
        assert_eq!(order, ["compile", "dex", "package"]);
    }

    #[test]
    fn the_plan_is_the_same_every_time() {
        // Directive section 12: identical input, identical order, so a build's
        // shape never depends on iteration luck.
        let mut graph = Graph::new();
        graph.add(node("a", NodeKind::Manifest)).unwrap();
        graph.add(node("b", NodeKind::Resources)).unwrap();
        graph
            .add(node("c", NodeKind::Dex).after(NodeId::new("a").unwrap()))
            .unwrap();
        graph
            .add(node("d", NodeKind::Package).after(NodeId::new("b").unwrap()))
            .unwrap();

        let first = graph.plan().unwrap();
        for _ in 0..16 {
            assert_eq!(graph.plan().unwrap(), first);
        }
    }

    #[test]
    fn a_cycle_is_reported_with_the_code_the_directive_names() {
        let mut graph = Graph::new();
        graph
            .add(node("a", NodeKind::Dex).after(NodeId::new("b").unwrap()))
            .unwrap();
        graph
            .add(node("b", NodeKind::Dex).after(NodeId::new("a").unwrap()))
            .unwrap();

        let error = graph.plan().unwrap_err();
        assert_eq!(error.code, super::graph::CYCLE_CODE);
        assert_eq!(error.code, "BUILD_GRAPH_CYCLE");
        assert_eq!(error.severity, Severity::Fatal);
        assert!(error.context.iter().any(|line| line.contains("a, b")));
    }

    #[test]
    fn a_node_that_depends_on_itself_is_a_cycle() {
        let mut graph = Graph::new();
        graph
            .add(node("a", NodeKind::Dex).after(NodeId::new("a").unwrap()))
            .unwrap();
        assert_eq!(graph.plan().unwrap_err().code, super::graph::CYCLE_CODE);
    }

    #[test]
    fn an_edge_to_a_node_that_does_not_exist_is_refused() {
        let mut graph = Graph::new();
        graph
            .add(node("a", NodeKind::Dex).after(NodeId::new("ghost").unwrap()))
            .unwrap();
        let error = graph.plan().unwrap_err();
        assert_eq!(error.code, "E4004");
        assert!(error.message.contains("ghost"));
    }

    #[test]
    fn node_identifiers_are_unique_and_restricted() {
        let mut graph = Graph::new();
        graph.add(node("a", NodeKind::Dex)).unwrap();
        assert_eq!(
            graph.add(node("a", NodeKind::Dex)).unwrap_err().code,
            "E4003"
        );
        assert_eq!(NodeId::new("has space").unwrap_err().code, "E4001");
        assert_eq!(NodeId::new("").unwrap_err().code, "E4001");
    }

    #[test]
    fn a_status_only_unblocks_dependents_when_it_produced_something() {
        // Directive section 10, stated once and used everywhere.
        assert!(NodeStatus::Succeeded.produced_its_outputs());
        assert!(NodeStatus::CacheHit.produced_its_outputs());
        for blocked in [
            NodeStatus::Pending,
            NodeStatus::Running,
            NodeStatus::Failed,
            NodeStatus::Skipped,
            NodeStatus::Cancelled,
        ] {
            assert!(!blocked.produced_its_outputs(), "{blocked:?}");
        }
    }

    // --- scheduler -----------------------------------------------------------

    /// An executor whose behaviour each test decides node by node.
    struct ScriptedExecutor {
        failing: Vec<String>,
        cached: Vec<String>,
        ran: Vec<String>,
    }

    impl ScriptedExecutor {
        fn new() -> Self {
            ScriptedExecutor {
                failing: Vec::new(),
                cached: Vec::new(),
                ran: Vec::new(),
            }
        }

        fn failing(mut self, id: &str) -> Self {
            self.failing.push(id.to_string());
            self
        }

        fn cached(mut self, id: &str) -> Self {
            self.cached.push(id.to_string());
            self
        }
    }

    impl super::scheduler::NodeExecutor for ScriptedExecutor {
        fn execute(
            &mut self,
            node: &Node,
            _ctx: &mut super::plugin::Context<'_>,
        ) -> Result<super::scheduler::NodeResult, Diagnostic> {
            let id = node.id().as_str().to_string();
            self.ran.push(id.clone());

            if self.failing.contains(&id) {
                return Err(Diagnostic::new(
                    "E9999",
                    Severity::Error,
                    FailureClass::InternalError,
                    "test",
                    "scripted failure",
                ));
            }

            Ok(super::scheduler::NodeResult {
                artifact_digest: Some(super::hash::sha256(id.as_bytes())),
                from_cache: self.cached.contains(&id),
                peak_memory_bytes: 0,
            })
        }
    }

    fn linear_graph() -> Graph {
        let mut graph = Graph::new();
        graph.add(node("compile", NodeKind::CompilerIr)).unwrap();
        graph
            .add(node("dex", NodeKind::Dex).after(NodeId::new("compile").unwrap()))
            .unwrap();
        graph
            .add(node("package", NodeKind::Package).after(NodeId::new("dex").unwrap()))
            .unwrap();
        graph
    }

    #[test]
    fn a_healthy_graph_runs_to_completion_in_dependency_order() {
        let mut graph = linear_graph();
        let mut executor = ScriptedExecutor::new();
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert_eq!(report.outcome, SchedulerOutcome::Completed);
        assert_eq!(report.succeeded, 3);
        assert_eq!(report.failed, 0);
        assert_eq!(executor.ran, ["compile", "dex", "package"]);
        for id in ["compile", "dex", "package"] {
            let found = graph.node(&NodeId::new(id).unwrap()).unwrap();
            assert_eq!(found.status(), NodeStatus::Succeeded);
            assert!(found.artifact_digest().is_some());
        }
    }

    #[test]
    fn a_failure_stops_its_dependents_and_says_why() {
        // Directive section 10: a failed dependency is never ignored, and the
        // nodes behind it are skipped rather than called failed.
        let mut graph = linear_graph();
        let mut executor = ScriptedExecutor::new().failing("dex");
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert_eq!(report.outcome, SchedulerOutcome::Failed);
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(
            executor.ran,
            ["compile", "dex"],
            "package must not have run"
        );

        assert_eq!(
            graph.node(&NodeId::new("dex").unwrap()).unwrap().status(),
            NodeStatus::Failed
        );
        assert_eq!(
            graph
                .node(&NodeId::new("package").unwrap())
                .unwrap()
                .status(),
            NodeStatus::Skipped
        );

        let skip = sink.entries().iter().find(|d| d.code == "W6002").unwrap();
        assert!(skip.context.iter().any(|line| line.contains("dex")));
        assert!(sink.entries().iter().any(|d| d.code == "E9999"));
    }

    #[test]
    fn a_cancelled_build_marks_nothing_as_successful() {
        // Directive section 10 forbids calling a cancelled node successful, and
        // section 35 asks for a safe stop rather than a torn one.
        let mut graph = linear_graph();
        let mut executor = ScriptedExecutor::new();
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let cancellation = Cancellation::new();
        cancellation.cancel();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &cancellation,
        );

        assert_eq!(report.outcome, SchedulerOutcome::Cancelled);
        assert_eq!(report.cancelled, 3);
        assert_eq!(report.succeeded, 0);
        assert!(executor.ran.is_empty());
        for id in ["compile", "dex", "package"] {
            let status = graph.node(&NodeId::new(id).unwrap()).unwrap().status();
            assert_eq!(status, NodeStatus::Cancelled);
            assert!(!status.produced_its_outputs());
        }
    }

    #[test]
    fn a_cache_hit_is_recorded_as_such_and_still_unblocks_dependents() {
        let mut graph = linear_graph();
        let mut executor = ScriptedExecutor::new().cached("compile");
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert_eq!(report.outcome, SchedulerOutcome::Completed);
        assert_eq!(report.cache_hits, 1);
        assert_eq!(report.succeeded, 2);
        assert_eq!(
            graph
                .node(&NodeId::new("compile").unwrap())
                .unwrap()
                .status(),
            NodeStatus::CacheHit
        );
    }

    #[test]
    fn a_graph_that_cannot_be_planned_runs_nothing_at_all() {
        let mut graph = Graph::new();
        graph
            .add(node("a", NodeKind::Dex).after(NodeId::new("b").unwrap()))
            .unwrap();
        graph
            .add(node("b", NodeKind::Dex).after(NodeId::new("a").unwrap()))
            .unwrap();

        let mut executor = ScriptedExecutor::new();
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert_eq!(report.outcome, SchedulerOutcome::Rejected);
        assert!(executor.ran.is_empty());
        assert!(report.order.is_empty());
        assert!(sink
            .entries()
            .iter()
            .any(|d| d.code == super::graph::CYCLE_CODE));
        for id in ["a", "b"] {
            assert_eq!(
                graph.node(&NodeId::new(id).unwrap()).unwrap().status(),
                NodeStatus::Pending
            );
        }
    }

    #[test]
    fn the_real_executor_refuses_to_invent_a_result_for_a_planned_plugin() {
        // Every plugin in this tree is PLANNED, so a real build fails - loudly,
        // with E0001, and with nothing marked as produced (directive section 1).
        let mut graph = Graph::new();
        graph
            .add(Node::new(
                NodeId::new("dex").unwrap(),
                NodeKind::Dex,
                "omni.plugin.dex",
                digests(),
            ))
            .unwrap();

        let mut executor = super::scheduler::PluginRegistryExecutor::new();
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert_eq!(report.outcome, SchedulerOutcome::Failed);
        assert_eq!(report.succeeded, 0);
        assert!(sink.entries().iter().any(|d| d.code == "E0001"));
        assert_eq!(
            graph.node(&NodeId::new("dex").unwrap()).unwrap().status(),
            NodeStatus::Failed
        );
    }

    #[test]
    fn a_node_naming_an_unknown_plugin_fails_rather_than_being_skipped() {
        let mut graph = Graph::new();
        graph
            .add(Node::new(
                NodeId::new("mystery").unwrap(),
                NodeKind::Dex,
                "omni.plugin.nonexistent",
                digests(),
            ))
            .unwrap();

        let mut executor = super::scheduler::PluginRegistryExecutor::new();
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();

        super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        assert!(sink.entries().iter().any(|d| d.code == "E6001"));
    }

    #[test]
    fn cancellation_is_shared_between_the_holder_and_the_build() {
        let token = Cancellation::new();
        let observer = token.clone();
        assert!(!observer.is_cancelled());
        token.cancel();
        assert!(observer.is_cancelled());
    }

    #[test]
    fn the_graph_and_the_scheduler_serialise_into_a_valid_report() {
        let mut graph = linear_graph();
        let mut executor = ScriptedExecutor::new().failing("dex");
        let mut policy = Policy::new("build");
        let mut sink = Sink::new();
        let report = super::scheduler::run(
            &mut graph,
            &mut executor,
            &mut policy,
            &mut sink,
            &Cancellation::new(),
        );

        let mut w = Writer::new();
        w.begin_object(None);
        graph.write_json(&mut w, "graph");
        report.write_json(&mut w, "build");
        w.end_object();
        let document = w.finish();

        assert!(is_structurally_valid(&document), "{document}");
        assert!(document.contains("\"acyclic\":true"));
        assert!(document.contains("\"outcome\":\"FAILED\""));
        assert!(document.contains("\"status\":\"SKIPPED\""));
    }

    // --- project manifest ----------------------------------------------------

    /// The manifest exactly as directive section 44 writes it, with the one
    /// typographical slip corrected: the directive prints `Deterministic.`
    /// with a trailing dot, and a separate test covers what happens if that is
    /// typed literally.
    const DIRECTIVE_MANIFEST: &str = r#"
[ Project ]
Name    = "Demo App"
Id        = "com.demo"
Version   = "1.0.0"
Edition    = "01/01/2000"

[ Android ]
Min_sdk      = 28
Target_sdk   = 36
Compile_sdk = 36

[ Build ]
Profile = "Release"
Optimization = "Size"
Lto             = true
Incremental     = true
Parallel         = true
Deterministic   = true

[ Security ]
Guard = "High"
Provenance    = true
Verification    = true

[ Features ]
Compose      = true
Viewbinding   = false
"#;

    #[test]
    fn the_manifest_from_the_directive_parses_exactly() {
        let mut sink = Sink::new();
        let project = parse_manifest(DIRECTIVE_MANIFEST, "com.fallback", &mut sink)
            .expect("the directive's own manifest must parse");

        assert!(!sink.has_blocking(), "diagnostics: {:?}", sink.entries());
        assert_eq!(project.name, "Demo App");
        assert_eq!(project.id, "com.demo");
        assert_eq!(project.version, "1.0.0");
        assert_eq!(project.edition.as_deref(), Some("01/01/2000"));
        assert_eq!(project.min_sdk, 28);
        assert_eq!(project.target_sdk, 36);
        assert_eq!(project.compile_sdk, 36);
        assert_eq!(project.profile, Profile::Release);
        assert_eq!(project.optimization, Optimization::Size);
        assert!(project.lto);
        assert!(project.incremental);
        assert!(project.parallel);
        assert!(project.deterministic);
        assert_eq!(project.guard, GuardLevel::High);
        assert!(project.provenance);
        assert!(project.verification);
        assert!(project.feature("Compose"));
        assert!(!project.feature("Viewbinding"));
        assert!(!project.feature("SomethingElse"));
    }

    #[test]
    fn a_mistyped_key_is_named_and_corrected() {
        // Directive section 44 prints "Deterministic." with a trailing dot. A
        // build that quietly accepted it would leave the author believing a
        // setting was in force when it was not (section 64), so it is refused -
        // with the correction spelled out.
        let manifest = "[ Project ]\nName = \"A\"\nId = \"com.a\"\n\
                        [ Build ]\nDeterministic.   = true\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.fallback", &mut sink).is_none());

        let error = sink
            .entries()
            .iter()
            .find(|d| d.code == "E3021")
            .expect("the unknown key must be reported");
        assert!(error
            .suggestion
            .as_deref()
            .unwrap()
            .contains("Deterministic"));
        assert_eq!(error.location.as_ref().unwrap().line, 5);
    }

    #[test]
    fn keys_and_sections_are_case_sensitive() {
        // Directive section 44 says so explicitly.
        let manifest = "[ project ]\nname = \"A\"\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.fallback", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3020"));

        let manifest = "[ Project ]\nname = \"A\"\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.fallback", &mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "E3021").unwrap();
        assert!(error
            .suggestion
            .as_deref()
            .unwrap()
            .contains("case-sensitive"));
    }

    #[test]
    fn a_minimal_manifest_gets_safe_defaults() {
        // Directive section 45: the smallest useful project still builds, and
        // every default is a decision rather than an accident.
        let mut sink = Sink::new();
        let project =
            parse_manifest("[ Project ]\nName = \"Tiny\"\n", "com.omni.tiny", &mut sink).unwrap();

        assert_eq!(project.name, "Tiny");
        assert_eq!(project.id, "com.omni.tiny");
        assert_eq!(project.min_sdk, 28);
        assert_eq!(project.target_sdk, 36);
        assert_eq!(project.compile_sdk, 36);
        assert!(project.deterministic);
        assert!(project.provenance);
        assert!(project.verification);
        assert_eq!(project.guard, GuardLevel::Medium);
    }

    #[test]
    fn a_project_without_a_name_is_refused() {
        let mut sink = Sink::new();
        assert!(parse_manifest("[ Project ]\nId = \"com.a\"\n", "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3030"));
    }

    #[test]
    fn values_must_have_the_type_the_key_expects() {
        let cases = [
            ("[ Project ]\nName = 5\n", "E3040"),
            (
                "[ Project ]\nName = \"A\"\n[ Android ]\nMin_sdk = \"28\"\n",
                "E3040",
            ),
            (
                "[ Project ]\nName = \"A\"\n[ Build ]\nLto = \"true\"\n",
                "E3040",
            ),
            (
                "[ Project ]\nName = \"A\"\n[ Features ]\nCompose = 1\n",
                "E3040",
            ),
        ];
        for (manifest, code) in cases {
            let mut sink = Sink::new();
            assert!(
                parse_manifest(manifest, "com.f", &mut sink).is_none(),
                "{manifest}"
            );
            assert!(
                sink.entries().iter().any(|d| d.code == code),
                "{manifest} -> {:?}",
                sink.entries()
            );
        }
    }

    #[test]
    fn malformed_syntax_is_reported_with_a_position() {
        let cases = [
            ("[ Project\nName = \"A\"\n", "E3003", 1u32),
            ("[ Project ]\nName\n", "E3004", 2),
            ("Name = \"A\"\n", "E3005", 1),
            ("[ Project ]\n = \"A\"\n", "E3006", 2),
            ("[ Project ]\nName =\n", "E3009", 2),
            ("[ Project ]\nName = \"unterminated\n", "E3010", 2),
            ("[ Project ]\nName = maybe\n", "E3013", 2),
        ];
        for (manifest, code, line) in cases {
            let mut sink = Sink::new();
            assert!(
                parse_manifest(manifest, "com.f", &mut sink).is_none(),
                "{manifest:?}"
            );
            let error = sink
                .entries()
                .iter()
                .find(|d| d.code == code)
                .unwrap_or_else(|| panic!("{manifest:?} -> {:?}", sink.entries()));
            assert_eq!(error.location.as_ref().unwrap().line, line, "{manifest:?}");
            assert_eq!(error.location.as_ref().unwrap().file, "Omni.toml");
        }
    }

    #[test]
    fn a_key_set_twice_is_refused_rather_than_resolved() {
        let manifest = "[ Project ]\nName = \"A\"\nName = \"B\"\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.f", &mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "E3008").unwrap();
        assert!(error.context.iter().any(|line| line.contains("line 2")));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let manifest = "# a project\n\n[ Project ]  # the section\nName = \"A\" # the name\n";
        let mut sink = Sink::new();
        let project = parse_manifest(manifest, "com.f", &mut sink).unwrap();
        assert_eq!(project.name, "A");
    }

    #[test]
    fn application_identifiers_are_validated() {
        for (id, ok) in [
            ("com.demo", true),
            ("com.demo.app_two", true),
            ("a.b", true),
            ("demo", false),
            ("com..demo", false),
            ("com.1demo", false),
            ("com.de-mo", false),
            ("", false),
        ] {
            let manifest = format!("[ Project ]\nName = \"A\"\nId = \"{id}\"\n");
            let mut sink = Sink::new();
            let parsed = parse_manifest(&manifest, "com.f", &mut sink);
            assert_eq!(parsed.is_some(), ok, "id: {id:?} -> {:?}", sink.entries());
            if !ok {
                assert!(sink.entries().iter().any(|d| d.code == "E3041"));
            }
        }
    }

    #[test]
    fn versions_and_editions_are_validated() {
        for (version, ok) in [
            ("1.0.0", true),
            ("10.20.30", true),
            ("1.0", false),
            ("1.0.x", false),
            ("01.0.0", false),
        ] {
            let manifest = format!("[ Project ]\nName = \"A\"\nVersion = \"{version}\"\n");
            let mut sink = Sink::new();
            assert_eq!(
                parse_manifest(&manifest, "com.f", &mut sink).is_some(),
                ok,
                "version: {version}"
            );
        }

        for (edition, ok) in [
            ("01/01/2000", true),
            ("31/12/2026", true),
            ("2000-01-01", false),
            ("1/1/2000", false),
            ("32/01/2000", false),
        ] {
            let manifest = format!("[ Project ]\nName = \"A\"\nEdition = \"{edition}\"\n");
            let mut sink = Sink::new();
            assert_eq!(
                parse_manifest(&manifest, "com.f", &mut sink).is_some(),
                ok,
                "edition: {edition}"
            );
        }
    }

    #[test]
    fn sdk_levels_must_be_consistent_with_each_other() {
        let manifest = "[ Project ]\nName = \"A\"\n[ Android ]\nMin_sdk = 36\nTarget_sdk = 28\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3031"));

        let manifest =
            "[ Project ]\nName = \"A\"\n[ Android ]\nTarget_sdk = 36\nCompile_sdk = 30\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3032"));

        let manifest = "[ Project ]\nName = \"A\"\n[ Android ]\nMin_sdk = -1\n";
        let mut sink = Sink::new();
        assert!(parse_manifest(manifest, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3041"));
    }

    #[test]
    fn a_deterministic_debug_build_is_flagged_but_allowed() {
        let manifest =
            "[ Project ]\nName = \"A\"\n[ Build ]\nProfile = \"Debug\"\nDeterministic = true\n";
        let mut sink = Sink::new();
        let project = parse_manifest(manifest, "com.f", &mut sink).unwrap();
        assert!(project.deterministic);
        assert!(sink.entries().iter().any(|d| d.code == "W3033"));
        assert!(!sink.has_blocking());
    }

    #[test]
    fn every_profile_and_level_named_by_the_directive_is_accepted() {
        for profile in Profile::ALL {
            let manifest = format!(
                "[ Project ]\nName = \"A\"\n[ Build ]\nProfile = \"{}\"\n",
                profile.as_str()
            );
            let mut sink = Sink::new();
            let parsed = parse_manifest(&manifest, "com.f", &mut sink).unwrap();
            assert_eq!(parsed.profile, *profile);
        }
        // Directive section 13 requires at least these nine.
        assert_eq!(Profile::ALL.len(), 9);

        for level in GuardLevel::ALL {
            let manifest = format!(
                "[ Project ]\nName = \"A\"\n[ Security ]\nGuard = \"{}\"\n",
                level.as_str()
            );
            let mut sink = Sink::new();
            assert_eq!(
                parse_manifest(&manifest, "com.f", &mut sink).unwrap().guard,
                *level
            );
        }
    }

    #[test]
    fn the_configuration_digest_reflects_behaviour_not_naming() {
        let base = parse_manifest(DIRECTIVE_MANIFEST, "com.f", &mut Sink::new()).unwrap();

        // Deterministic (directive section 12).
        assert_eq!(base.digest(), base.digest());
        let again = parse_manifest(DIRECTIVE_MANIFEST, "com.f", &mut Sink::new()).unwrap();
        assert_eq!(base.digest(), again.digest());

        // Renaming a project does not change what it produces.
        let mut renamed = base.clone();
        renamed.name = "Something Else".to_string();
        assert_eq!(base.digest(), renamed.digest());

        // Anything that does change the output changes the digest.
        for mutate in [
            (|p: &mut Project| p.optimization = Optimization::Speed) as fn(&mut Project),
            |p: &mut Project| p.lto = false,
            |p: &mut Project| p.min_sdk = 29,
            |p: &mut Project| p.profile = Profile::Debug,
            |p: &mut Project| p.guard = GuardLevel::Off,
            |p: &mut Project| p.features.push(("Extra".to_string(), true)),
        ] {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(base.digest(), changed.digest());
        }
    }

    #[test]
    fn feature_order_does_not_change_the_project() {
        let forwards = "[ Project ]\nName = \"A\"\n[ Features ]\nA = true\nB = false\n";
        let backwards = "[ Project ]\nName = \"A\"\n[ Features ]\nB = false\nA = true\n";
        let left = parse_manifest(forwards, "com.f", &mut Sink::new()).unwrap();
        let right = parse_manifest(backwards, "com.f", &mut Sink::new()).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.digest(), right.digest());
    }

    #[test]
    fn the_manifest_is_bounded() {
        // Directive section 60, on input that is not trusted.
        let mut sink = Sink::new();
        let huge = "x".repeat(super::project::MAX_MANIFEST_BYTES + 1);
        assert!(parse_manifest(&huge, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3001"));

        let mut sink = Sink::new();
        let many_lines = "\n".repeat(super::project::MAX_LINES + 10);
        assert!(parse_manifest(&many_lines, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3002"));

        let mut sink = Sink::new();
        let many_entries: String = std::iter::once("[ Features ]\n".to_string())
            .chain((0..super::project::MAX_ENTRIES + 10).map(|i| format!("F{i} = true\n")))
            .collect();
        assert!(parse_manifest(&many_entries, "com.f", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E3007"));
    }

    #[test]
    fn hostile_manifests_are_rejected_without_crashing() {
        // Directive section 41: malformed input must not crash, hang or allocate
        // without bound. Project files come from outside (section 61).
        let cases = [
            "",
            "[",
            "[]",
            "[ ]",
            "=",
            "==",
            "[ Project ]\n=\n",
            "[ Project ]\nName = \"\u{0}\"\n",
            "[ Project ]\nName = \"a\"b\"\n",
            "\u{feff}[ Project ]\nName = \"A\"\n",
            "[ Project ]\r\nName = \"A\"\r\n",
            "[ Features ]\n= true\n",
            "[ Project ]\nName = 99999999999999999999999\n",
            "[ Android ]\nMin_sdk = 9999999999999999999\n",
        ];
        for manifest in cases {
            let mut sink = Sink::new();
            // The only requirement is that this returns rather than misbehaving.
            let _ = parse_manifest(manifest, "com.f", &mut sink);
        }
    }

    #[test]
    fn a_project_serialises_into_a_valid_report() {
        let project = parse_manifest(DIRECTIVE_MANIFEST, "com.f", &mut Sink::new()).unwrap();
        let mut w = Writer::new();
        w.begin_object(None);
        project.write_json(&mut w, "project");
        w.end_object();
        let document = w.finish();

        assert!(is_structurally_valid(&document), "{document}");
        assert!(document.contains("\"id\":\"com.demo\""));
        assert!(document.contains("\"profile\":\"Release\""));
        assert!(document.contains(&project.digest().to_hex()));
    }

    // --- virtual filesystem --------------------------------------------------

    /// A fresh directory under the system temporary directory.
    ///
    /// No dependency is used for this: a counter plus the process id is enough
    /// to keep concurrent tests apart, and ADR-0003 keeps the Core dependency
    /// free right down to its tests.
    fn temp_directory(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("omni-vfs-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    /// A policy that grants exactly what the test asks for.
    fn policy_with(capabilities: &[Capability]) -> Policy {
        let mut policy = Policy::new("test");
        for capability in capabilities {
            policy.grant(*capability);
        }
        policy
    }

    #[test]
    fn virtual_paths_normalise_what_they_accept() {
        let path = VirtualPath::parse("Source/./Main/Builder.kt").unwrap();
        assert_eq!(path.as_str(), "Source/Main/Builder.kt");
        assert_eq!(path.segments(), ["Source", "Main", "Builder.kt"]);
        assert_eq!(path.file_name(), "Builder.kt");
        assert_eq!(path.extension(), Some("kt"));

        assert_eq!(VirtualPath::parse("a").unwrap().as_str(), "a");
        assert_eq!(VirtualPath::parse("./a").unwrap().as_str(), "a");
        assert_eq!(VirtualPath::parse("a/b/c").unwrap().extension(), None);
        assert_eq!(VirtualPath::parse("a.b.c").unwrap().extension(), Some("c"));
        assert_eq!(VirtualPath::parse("noext.").unwrap().extension(), None);
    }

    #[test]
    fn virtual_paths_refuse_to_leave_their_mount() {
        // Directive section 8: traversal is refused, in every spelling.
        for attempt in [
            "..",
            "../secret",
            "a/../../secret",
            "a/b/../../../secret",
            "./../secret",
        ] {
            let error =
                VirtualPath::parse(attempt).expect_err("traversal must be refused: {attempt}");
            assert_eq!(error.code, "E2007", "input: {attempt}");
            assert_eq!(error.class, FailureClass::SecurityFailure);
        }
    }

    #[test]
    fn virtual_paths_refuse_absolute_forms() {
        assert_eq!(VirtualPath::parse("/etc/passwd").unwrap_err().code, "E2005");
        assert_eq!(VirtualPath::parse("/").unwrap_err().code, "E2005");
        assert_eq!(VirtualPath::parse("C:/Windows").unwrap_err().code, "E2005");
    }

    #[test]
    fn virtual_paths_refuse_ambiguous_or_hostile_spellings() {
        assert_eq!(VirtualPath::parse("").unwrap_err().code, "E2001");
        assert_eq!(VirtualPath::parse(".").unwrap_err().code, "E2001");
        assert_eq!(VirtualPath::parse("./.").unwrap_err().code, "E2001");
        assert_eq!(VirtualPath::parse("a//b").unwrap_err().code, "E2006");
        assert_eq!(VirtualPath::parse("a/").unwrap_err().code, "E2006");
        assert_eq!(VirtualPath::parse("a\\b").unwrap_err().code, "E2004");
        assert_eq!(VirtualPath::parse("a\u{0}b").unwrap_err().code, "E2003");
        assert_eq!(VirtualPath::parse("a\nb").unwrap_err().code, "E2003");
        assert_eq!(VirtualPath::parse("a\u{7f}b").unwrap_err().code, "E2003");
    }

    #[test]
    fn virtual_paths_are_bounded() {
        // Directive section 60: path explosion and deep nesting are refused
        // rather than merely being slow.
        let long_path = "a".repeat(super::vfs::MAX_PATH_BYTES + 1);
        assert_eq!(VirtualPath::parse(&long_path).unwrap_err().code, "E2002");

        let long_segment = format!("dir/{}", "b".repeat(super::vfs::MAX_SEGMENT_BYTES + 1));
        assert_eq!(VirtualPath::parse(&long_segment).unwrap_err().code, "E2008");

        let deep = vec!["d"; super::vfs::MAX_SEGMENTS + 1].join("/");
        assert_eq!(VirtualPath::parse(&deep).unwrap_err().code, "E2009");

        // Exactly at the limits is accepted; the bound is not off by one.
        let at_limit = vec!["d"; super::vfs::MAX_SEGMENTS].join("/");
        assert!(VirtualPath::parse(&at_limit).is_ok());
    }

    #[test]
    fn virtual_paths_keep_unicode_intact() {
        let path = VirtualPath::parse("Kaynak/Ana/Örnek türkçe.kt").unwrap();
        assert_eq!(path.file_name(), "Örnek türkçe.kt");
        assert_eq!(path.extension(), Some("kt"));
    }

    #[test]
    fn mounting_requires_a_real_directory_and_a_usable_name() {
        let root = temp_directory("mount");
        let mut vfs = VirtualFs::new(Quota::default());

        assert_eq!(
            vfs.mount("", &root, Access::ReadOnly).unwrap_err().code,
            "E2010"
        );
        assert_eq!(
            vfs.mount("a/b", &root, Access::ReadOnly).unwrap_err().code,
            "E2010"
        );
        assert_eq!(
            vfs.mount("missing", root.join("nowhere"), Access::ReadOnly)
                .unwrap_err()
                .code,
            "E2012"
        );

        vfs.mount("project", &root, Access::ReadOnly).unwrap();
        assert_eq!(
            vfs.mount("project", &root, Access::ReadOnly)
                .unwrap_err()
                .code,
            "E2011"
        );
        assert_eq!(vfs.mounts().len(), 1);
        assert_eq!(vfs.mounts()[0].name(), "project");
        assert_eq!(vfs.mounts()[0].access(), Access::ReadOnly);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reading_and_writing_round_trip_with_a_digest() {
        let root = temp_directory("roundtrip");
        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("output", &root, Access::ReadWrite).unwrap();

        let mut policy = policy_with(&[Capability::FsRead, Capability::FsWrite]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let path = VirtualPath::parse("nested/deep/file.txt").unwrap();
        let written = vfs
            .write_atomic(&mut ctx, "plugin.test", "output", &path, b"omni")
            .unwrap();
        assert_eq!(written, super::hash::sha256(b"omni"));

        let (bytes, digest) = vfs.read(&mut ctx, "plugin.test", "output", &path).unwrap();
        assert_eq!(bytes, b"omni");
        assert_eq!(digest, written);

        assert!(vfs
            .exists(&mut ctx, "plugin.test", "output", &path)
            .unwrap());
        let absent = VirtualPath::parse("nested/deep/absent.txt").unwrap();
        assert!(!vfs
            .exists(&mut ctx, "plugin.test", "output", &absent)
            .unwrap());

        let usage = vfs.usage();
        assert_eq!(usage.reads, 1);
        assert_eq!(usage.writes, 1);
        assert_eq!(usage.bytes_written, 4);
        assert_eq!(usage.bytes_read, 4);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_write_leaves_no_partial_file_behind() {
        // Directive section 59: a build must never publish something half
        // written, and must not litter the output with the evidence either.
        let root = temp_directory("atomic");
        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("output", &root, Access::ReadWrite).unwrap();

        let mut policy = policy_with(&[Capability::FsWrite]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let path = VirtualPath::parse("artifact.bin").unwrap();
        vfs.write_atomic(&mut ctx, "plugin.test", "output", &path, b"first")
            .unwrap();
        vfs.write_atomic(&mut ctx, "plugin.test", "output", &path, b"second")
            .unwrap();

        let leftovers: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("omni-partial"))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
        assert_eq!(std::fs::read(root.join("artifact.bin")).unwrap(), b"second");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_filesystem_is_unreachable_without_a_capability() {
        // Directive section 7: default deny, and the refusal is a diagnostic
        // rather than a silent empty result.
        let root = temp_directory("caps");
        std::fs::write(root.join("file.txt"), b"data").unwrap();

        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("project", &root, Access::ReadWrite).unwrap();

        let mut policy = Policy::new("empty");
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };
        let path = VirtualPath::parse("file.txt").unwrap();

        let read = vfs
            .read(&mut ctx, "plugin.test", "project", &path)
            .unwrap_err();
        assert_eq!(read.code, "E2030");
        assert_eq!(read.class, FailureClass::SecurityFailure);

        let write = vfs
            .write_atomic(&mut ctx, "plugin.test", "project", &path, b"x")
            .unwrap_err();
        assert_eq!(write.code, "E2030");

        // And the refusal was audited.
        assert!(policy.audit().iter().any(|record| {
            record.subject == "plugin.test" && record.decision == Decision::Deny
        }));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_read_only_mount_refuses_writes_even_with_the_capability() {
        let root = temp_directory("readonly");
        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("source", &root, Access::ReadOnly).unwrap();

        let mut policy = policy_with(&[Capability::FsWrite]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let path = VirtualPath::parse("file.txt").unwrap();
        let error = vfs
            .write_atomic(&mut ctx, "plugin.test", "source", &path, b"x")
            .unwrap_err();
        assert_eq!(error.code, "E2032");
        assert!(!root.join("file.txt").exists());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_unknown_mount_is_named_in_the_diagnostic() {
        let root = temp_directory("unknown");
        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("project", &root, Access::ReadOnly).unwrap();

        let mut policy = policy_with(&[Capability::FsRead]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let path = VirtualPath::parse("file.txt").unwrap();
        let error = vfs
            .read(&mut ctx, "plugin.test", "elsewhere", &path)
            .unwrap_err();
        assert_eq!(error.code, "E2031");
        assert!(error.context.iter().any(|line| line.contains("project")));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn quotas_are_enforced_on_the_file_and_on_the_build() {
        let root = temp_directory("quota");
        let mut vfs = VirtualFs::new(Quota {
            max_file_bytes: 8,
            max_written_bytes: 12,
        });
        vfs.mount("output", &root, Access::ReadWrite).unwrap();

        let mut policy = policy_with(&[Capability::FsRead, Capability::FsWrite]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let big = VirtualPath::parse("big.bin").unwrap();
        assert_eq!(
            vfs.write_atomic(&mut ctx, "p", "output", &big, &[0u8; 9])
                .unwrap_err()
                .code,
            "E2022"
        );

        let a = VirtualPath::parse("a.bin").unwrap();
        let b = VirtualPath::parse("b.bin").unwrap();
        vfs.write_atomic(&mut ctx, "p", "output", &a, &[0u8; 8])
            .unwrap();
        assert_eq!(
            vfs.write_atomic(&mut ctx, "p", "output", &b, &[0u8; 8])
                .unwrap_err()
                .code,
            "E2023"
        );
        assert_eq!(vfs.usage().bytes_written, 8);

        // Reading is bounded too.
        std::fs::write(root.join("large.bin"), [0u8; 9]).unwrap();
        let large = VirtualPath::parse("large.bin").unwrap();
        assert_eq!(
            vfs.read(&mut ctx, "p", "output", &large).unwrap_err().code,
            "E2021"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_cannot_smuggle_a_path_out_of_its_mount() {
        // Syntax alone cannot catch this: the path has no '..' in it. Only
        // resolving the link does.
        let root = temp_directory("symlink");
        let outside = temp_directory("symlink-outside");
        std::fs::write(outside.join("secret.txt"), b"not yours").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

        let mut vfs = VirtualFs::new(Quota::default());
        vfs.mount("project", &root, Access::ReadWrite).unwrap();

        let mut policy = policy_with(&[Capability::FsRead]);
        let mut sink = Sink::new();
        let mut ctx = super::plugin::Context {
            policy: &mut policy,
            diagnostics: &mut sink,
        };

        let path = VirtualPath::parse("escape/secret.txt").unwrap();
        let error = vfs
            .read(&mut ctx, "plugin.test", "project", &path)
            .unwrap_err();
        assert_eq!(error.code, "E2033");
        assert_eq!(error.class, FailureClass::SecurityFailure);
        assert!(vfs.usage().refusals > 0);

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    // --- SHA-256 (official NIST vectors) -------------------------------------

    #[test]
    fn sha256_matches_the_nist_published_vectors() {
        // FIPS 180-4 and the NIST Cryptographic Algorithm Validation Program
        // publish these. Directive section 30 makes passing them the condition
        // for using the primitive at all.
        let cases: &[(&[u8], &str)] = &[
            (
                b"",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            (
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
                  ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
                "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
            ),
        ];

        for (input, expected) in cases {
            // The fourth case uses a line continuation, so strip the indentation
            // the source layout introduced.
            let cleaned: Vec<u8> = input.iter().copied().filter(|b| *b != b' ').collect();
            assert_eq!(
                super::hash::sha256(&cleaned).to_hex(),
                *expected,
                "input: {:?}",
                String::from_utf8_lossy(&cleaned)
            );
        }
    }

    #[test]
    fn sha256_matches_the_one_million_a_vector() {
        // The long-message vector from FIPS 180-4 appendix B.3. It is the one
        // that catches a broken length counter or a broken padding boundary.
        let mut hasher = super::hash::Sha256::new();
        let chunk = vec![b'a'; 1_000];
        for _ in 0..1_000 {
            hasher.update(&chunk);
        }
        assert_eq!(
            hasher.finish().to_hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_is_insensitive_to_how_the_message_is_split() {
        // Streaming has to agree with the one-shot form, or a digest would
        // depend on the caller's buffer size rather than on the data.
        let message: Vec<u8> = (0u8..=255).cycle().take(1_000).collect();
        let one_shot = super::hash::sha256(&message);

        for split in [1usize, 7, 55, 56, 57, 63, 64, 65, 128, 999] {
            let mut hasher = super::hash::Sha256::new();
            for piece in message.chunks(split) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finish(), one_shot, "split at {split}");
        }
    }

    #[test]
    fn sha256_handles_every_padding_boundary() {
        // Lengths either side of the block and length-field boundaries are where
        // a padding bug hides.
        for length in [0usize, 1, 54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128] {
            let message = vec![b'x'; length];
            let streamed = {
                let mut hasher = super::hash::Sha256::new();
                hasher.update(&message);
                hasher.finish()
            };
            assert_eq!(streamed, super::hash::sha256(&message), "length {length}");
        }
    }

    #[test]
    fn digest_renders_as_lowercase_hex() {
        let digest = super::hash::sha256(b"abc");
        let hex = digest.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
        assert_eq!(digest.to_short_hex(4), "ba7816bf");
        assert_eq!(digest.to_short_hex(64).len(), 64);
        assert_eq!(format!("{digest:?}"), format!("sha256:{hex}"));
    }

    #[test]
    fn field_hashing_cannot_be_confused_by_moving_a_boundary() {
        // Without length prefixes these two would hash identically, and two
        // different cache keys would collide (directive section 11).
        let left = super::hash::sha256_fields(&[("a", b"ab"), ("b", b"c")]);
        let right = super::hash::sha256_fields(&[("a", b"a"), ("b", b"bc")]);
        assert_ne!(left, right);

        // Renaming a field changes the digest too.
        assert_ne!(
            super::hash::sha256_fields(&[("a", b"x")]),
            super::hash::sha256_fields(&[("b", b"x")])
        );

        // And the same fields always give the same answer (section 12).
        assert_eq!(
            super::hash::sha256_fields(&[("a", b"x"), ("b", b"y")]),
            super::hash::sha256_fields(&[("a", b"x"), ("b", b"y")])
        );
    }

    // --- C ABI --------------------------------------------------------------

    #[test]
    fn the_abi_reports_its_version() {
        assert_eq!(super::ffi::omni_abi_version(), super::ffi::OMNI_ABI_VERSION);
    }

    #[test]
    fn the_abi_exposes_a_static_nul_terminated_version() {
        let ptr = super::ffi::omni_core_version();
        assert!(!ptr.is_null());
        // SAFETY: the ABI guarantees a static NUL-terminated string.
        let version = unsafe { std::ffi::CStr::from_ptr(ptr) };
        assert_eq!(version.to_str().unwrap(), super::CORE_VERSION);
    }

    #[test]
    fn the_abi_accepts_null_as_an_empty_observation() {
        // SAFETY: null is an accepted argument per the ABI contract.
        let ptr = unsafe { super::ffi::omni_state_report(std::ptr::null()) };
        assert!(!ptr.is_null());
        // SAFETY: the pointer was produced by omni_state_report.
        let report = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap()
            .to_string();
        // SAFETY: releasing exactly once, as the contract requires.
        unsafe { super::ffi::omni_string_free(ptr) };
        assert!(is_structurally_valid(&report));
    }

    #[test]
    fn the_abi_round_trips_an_observation() {
        let input = std::ffi::CString::new("minSdk=28;targetSdk=36").unwrap();
        // SAFETY: a valid NUL-terminated string is passed.
        let ptr = unsafe { super::ffi::omni_state_report(input.as_ptr()) };
        assert!(!ptr.is_null());
        // SAFETY: the pointer was produced by omni_state_report.
        let report = unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap()
            .to_string();
        // SAFETY: releasing exactly once.
        unsafe { super::ffi::omni_string_free(ptr) };
        assert!(report.contains("\"state\":\"MATCH\""));
    }

    #[test]
    fn freeing_null_is_a_no_op() {
        // SAFETY: null is explicitly accepted.
        unsafe { super::ffi::omni_string_free(std::ptr::null_mut()) };
    }
}
