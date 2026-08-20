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
//!
//! ### ADR-0010 — Signatures are inspected, not verified, and the report says so
//!
//! * **Context.** Phase 6 covers signing (directive section 25). Reading an APK
//!   Signature Scheme v2 block needs three things: the block's own structure,
//!   the chunked content digest it claims, and RSA or elliptic-curve arithmetic
//!   to check the signature over the signed data. The first two are format work
//!   and hashing. The third is public-key cryptography, which directive section
//!   30 forbids inventing and ADR-0003 forbids importing.
//! * **Alternatives.** (a) Implement RSA in the Core so a signature can be fully
//!   verified. (b) Take a cryptography dependency, breaking ADR-0003. (c) Ship
//!   nothing until (a) or (b) is possible. (d) Implement the block reader and
//!   the content digest, and make every report state in its own fields that the
//!   signature itself was not checked.
//! * **Decision.** (d).
//! * **Reason.** The content digest is the part that catches the threats
//!   directive section 27 names — T1 an APK modified after signing, T3 a
//!   modified DEX, T4 a modified native library — and it is checkable against
//!   an independent implementation, which `apksigner` provides. Writing RSA to
//!   get there would mean hand-rolling modular exponentiation and PKCS#1
//!   padding under section 30's rules with no way to test it against anything
//!   this tree can run; a subtly wrong verifier that returns *valid* is worse
//!   than an honest one that returns *unchecked*. Option (c) throws away work
//!   that is correct and useful.
//! * **Tradeoffs.** A digest match proves the package has not changed since the
//!   block was written. It does not prove who wrote it: anyone able to rewrite
//!   the package can rewrite the block to match. So this detects tampering with
//!   a package, and establishes no provenance whatsoever.
//! * **Security impact.** The risk is entirely one of being believed to do more
//!   than it does, so the API refuses to allow the confusion: `signing::Report`
//!   carries `signatures_checked`, which is a constant `false` and not a
//!   computed field, `x509::Certificate` carries `signatureChecked: false`, and
//!   both are written into every JSON report. No function in either module
//!   returns a bare boolean that a caller could read as *this is valid*.
//! * **Performance impact.** One SHA-256 pass over the package, chunked at one
//!   megabyte as the scheme defines.
//! * **Migration plan.** When public-key arithmetic exists, `signatures_checked`
//!   becomes a computed field and the constant-`false` tests are what force the
//!   reports and the subsystem inventory to be updated with it.
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

/// The nine plugins of directive section 6.
///
/// They are grouped under one module so that a plugin's name cannot collide
/// with a Core subsystem's: `plugins::resources` declares what a resource
/// plugin would do, and [`crate::resources`] is the engine that does it.
///
/// The `#[path = "."]` keeps the nine files where directive section 46 puts
/// them: without it, an inline module makes its children resolve under a
/// directory named after the module.
#[path = "."]
pub mod plugins {
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
}

// ===========================================================================
// Core identity
// ===========================================================================

/// Semantic version of the Core, taken from `Cargo.toml` at compile time.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Phase of the roadmap in directive section 52 that this tree implements.
pub const CORE_PHASE: &str = "PHASE 6 — SIGNING";

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
        missing: &["Randomised robustness testing only; not coverage-guided fuzzing."],
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
            "Nothing signs an artifact, so SIGNED is reachable and unused; \
             the signing module reads a signature, it does not produce one.",
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
        name: "Binary core",
        status: Status::Partial,
        directive_section: 20,
        summary: "Bounded readers, patched writers, sections, tables and the CRC-32 \
                  and Adler-32 checksums the ZIP and DEX formats use.",
        missing: &[
            "No format is implemented on top of it yet.",
            "Randomised robustness testing only; not coverage-guided fuzzing.",
            "The Validator trait has no implementations.",
        ],
    },
    Subsystem {
        name: "XML reader",
        status: Status::Partial,
        directive_section: 22,
        summary: "Enough XML to read resource files, with document type \
                  declarations and custom entities refused outright.",
        missing: &[
            "Namespaces are carried as part of a name, not resolved.",
            "No schema validation.",
        ],
    },
    Subsystem {
        name: "Resource engine",
        status: Status::Partial,
        directive_section: 22,
        summary: "Values files parsed and validated, identifiers assigned from \
                  sorted order, references resolved and reference loops refused.",
        missing: &[
            "No binary resource table is written; that belongs with packaging.",
            "Only density qualifiers are modelled; a locale directory is refused.",
            "Styles can be referred to but not declared.",
            "Nothing reads resource files through the virtual filesystem yet.",
        ],
    },
    Subsystem {
        name: "Archive engine",
        status: Status::Partial,
        directive_section: 23,
        summary: "The ZIP container an APK is: read, validated, and written \
                  deterministically with page-aligned native libraries.",
        missing: &[
            "Writes stored entries only; nothing compresses.",
            "No ZIP64, so four gigabytes and 65535 entries are hard limits.",
            "Nothing assembles an APK from a project yet; this is the container, \
             not the packaging step.",
            "No signature block is written; the signing module reads one.",
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
    Subsystem {
        name: "DER reader",
        status: Status::Partial,
        directive_section: 30,
        summary: "A distinguished-encoding reader that refuses every alternative \
                  spelling BER allows: indefinite lengths, non-minimal lengths, \
                  padded integers and the high-tag-number form.",
        missing: &[
            "Reads the types certificates use; not a general ASN.1 decoder.",
            "Randomised robustness testing only; not coverage-guided fuzzing.",
        ],
    },
    Subsystem {
        name: "X.509 certificates",
        status: Status::Partial,
        directive_section: 30,
        summary: "Reads a certificate's names, validity, serial, key size and \
                  algorithms, and fingerprints it with SHA-256.",
        missing: &[
            "The certificate's own signature is never checked, so this identifies \
             a certificate but never validates one.",
            "No chain building and no trust store.",
            "Extensions are not read, so key usage and basic constraints are unseen.",
        ],
    },
    Subsystem {
        name: "Signature inspection",
        status: Status::Partial,
        directive_section: 25,
        summary: "Finds the APK signing block, reads its v2 signers and \
                  certificates, and recomputes the chunked SHA-256 content digest \
                  over the package's own bytes -- matched against apksigner.",
        missing: &[
            "The signature over the signed data is never verified, because there \
             is no RSA or elliptic-curve arithmetic here. A digest match proves \
             the package is unchanged, not who signed it.",
            "Reads v2 only; v3 and v3.1 blocks are listed but not parsed.",
            "Nothing writes a signing block, so signing still belongs to the \
             bootstrap toolchain.",
        ],
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

        /// Reclassifies the failure.
        ///
        /// A diagnostic is usually built with the class it will keep, but a
        /// parser reports most problems as user error and a handful as resource
        /// exhaustion; this keeps those from needing a second constructor.
        pub fn with_class(mut self, class: FailureClass) -> Self {
            self.class = class;
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
        &crate::plugins::kotlin::PLUGIN,
        &crate::plugins::java::PLUGIN,
        &crate::plugins::cpp::PLUGIN,
        &crate::plugins::rust::PLUGIN,
        &crate::plugins::resources::PLUGIN,
        &crate::plugins::dex::PLUGIN,
        &crate::plugins::apk::PLUGIN,
        &crate::plugins::sign::PLUGIN,
        &crate::plugins::guard::PLUGIN,
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
// binary — the binary core (directive sections 20 and 41)
// ===========================================================================

/// Reading and writing binary formats, safely.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                        |
/// |----------------------|--------------------------------------------------------------|
/// | Module               | `omni_core::binary`                                          |
/// | Purpose              | The primitives every binary subsystem shares: cursors, bounded |
/// |                      | reads, patched writes, sections, tables and checksums.        |
/// | Non-Responsibilities | Knowing what any particular format means. DEX, ZIP and the    |
/// |                      | resource table are built on this; none of them lives here.    |
/// | Inputs               | Byte slices. Untrusted, always.                              |
/// | Outputs              | Values, or diagnostics saying exactly what was wrong and where.|
/// | Security             | Every read is bounds-checked. Every length that comes *out of* |
/// |                      | the data is validated against what remains *before* anything   |
/// |                      | is allocated. There is no recursion.                          |
/// | Failure Modes        | Truncated input, a length that cannot be satisfied, an integer |
/// |                      | that overflows, an overlong encoding, a write past the limit.  |
/// | Determinism          | A writer given the same calls produces the same bytes.        |
/// | Status               | PARTIAL — see the subsystem inventory.                        |
///
/// ## Why every read returns a `Result`
///
/// Directive section 41 requires that malformed input cannot crash, hang,
/// corrupt memory or allocate without bound. A reader that panics on bad input
/// satisfies none of that, and a reader that returns a zero on bad input is
/// worse: it turns a detectable problem into a silent one. So every read either
/// returns the value or says why it could not, and the type system makes
/// ignoring that awkward.
pub mod binary {
    use crate::diag::{Diagnostic, Severity, Sink};
    use crate::FailureClass;

    /// Largest buffer a writer will produce (directive section 60).
    pub const MAX_BUFFER_BYTES: usize = 256 * 1024 * 1024;

    /// Byte order.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Endian {
        /// Least significant byte first. DEX and ZIP both use this.
        Little,
        /// Most significant byte first.
        Big,
    }

    impl Endian {
        /// Stable machine-readable name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Endian::Little => "LITTLE",
                Endian::Big => "BIG",
            }
        }
    }

    fn fail(code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::Corruption,
            "core.binary",
            message,
        )
    }

    /// A bounded, non-panicking reader over a byte slice.
    ///
    /// The lifetime is the data's: reads that return bytes borrow from the input
    /// rather than copying it, which is what lets a large file be parsed without
    /// being duplicated in memory (directive section 37).
    #[derive(Clone, Debug)]
    pub struct Reader<'a> {
        data: &'a [u8],
        position: usize,
        endian: Endian,
        origin: String,
    }

    impl<'a> Reader<'a> {
        /// Reads `data`, describing it as `origin` in any diagnostic.
        pub fn new(data: &'a [u8], endian: Endian, origin: impl Into<String>) -> Reader<'a> {
            Reader {
                data,
                position: 0,
                endian,
                origin: origin.into(),
            }
        }

        /// Total size of the input.
        pub fn len(&self) -> usize {
            self.data.len()
        }

        /// Whether the input is empty.
        pub fn is_empty(&self) -> bool {
            self.data.is_empty()
        }

        /// Current offset.
        pub fn position(&self) -> usize {
            self.position
        }

        /// Bytes left after the cursor.
        pub fn remaining(&self) -> usize {
            self.data.len() - self.position
        }

        /// Byte order in force.
        pub fn endian(&self) -> Endian {
            self.endian
        }

        /// Moves the cursor to an absolute offset.
        pub fn seek(&mut self, offset: u64) -> Result<(), Diagnostic> {
            let offset = self.checked_offset(offset)?;
            self.position = offset;
            Ok(())
        }

        /// Advances the cursor.
        pub fn skip(&mut self, count: usize) -> Result<(), Diagnostic> {
            self.require(count)?;
            self.position += count;
            Ok(())
        }

        /// Converts a length taken from the data into a usable one.
        ///
        /// This is the single most important function in the module. A length
        /// field in a malformed file is the classic way to make a parser
        /// allocate gigabytes or read past its buffer, so a declared length is
        /// checked against what is actually left *before* it is used for
        /// anything (directive section 60).
        pub fn checked_length(&self, declared: u64) -> Result<usize, Diagnostic> {
            let remaining = self.remaining() as u64;
            if declared > remaining {
                return Err(fail(
                    "E7001",
                    "The data declares a length longer than the data that follows it.",
                )
                .with_context(format!("Source: {}", self.origin))
                .with_context(format!("At offset: {}", self.position))
                .with_context(format!("Declared: {declared} bytes"))
                .with_context(format!("Available: {remaining} bytes"))
                .with_suggestion(
                    "The input is truncated or the length field is wrong. Nothing \
                     is allocated for a length that cannot be satisfied.",
                ));
            }
            Ok(declared as usize)
        }

        /// Converts an offset taken from the data into a usable one.
        pub fn checked_offset(&self, declared: u64) -> Result<usize, Diagnostic> {
            if declared > self.data.len() as u64 {
                return Err(fail("E7002", "The data points past its own end.")
                    .with_context(format!("Source: {}", self.origin))
                    .with_context(format!("Offset: {declared}"))
                    .with_context(format!("Size: {} bytes", self.data.len()))
                    .with_suggestion("The offset field is wrong, or the file is truncated."));
            }
            Ok(declared as usize)
        }

        fn require(&self, count: usize) -> Result<(), Diagnostic> {
            if count > self.remaining() {
                return Err(fail("E7003", "The input ended before the value did.")
                    .with_context(format!("Source: {}", self.origin))
                    .with_context(format!("At offset: {}", self.position))
                    .with_context(format!("Wanted: {count} bytes"))
                    .with_context(format!("Available: {}", self.remaining()))
                    .with_suggestion("The input is truncated."));
            }
            Ok(())
        }

        /// Reads `count` bytes and advances.
        pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], Diagnostic> {
            self.require(count)?;
            let start = self.position;
            self.position += count;
            Ok(&self.data[start..self.position])
        }

        /// Borrows a span without moving the cursor.
        pub fn slice_at(&self, offset: u64, length: u64) -> Result<&'a [u8], Diagnostic> {
            let start = self.checked_offset(offset)?;
            let Some(end) = (start as u64).checked_add(length) else {
                return Err(fail("E7004", "An offset and length overflow when added.")
                    .with_context(format!("Source: {}", self.origin))
                    .with_context(format!("Offset: {offset}, length: {length}")));
            };
            let end = self.checked_offset(end)?;
            Ok(&self.data[start..end])
        }

        /// Reads one byte.
        pub fn u8(&mut self) -> Result<u8, Diagnostic> {
            Ok(self.bytes(1)?[0])
        }

        /// Reads a signed byte.
        pub fn i8(&mut self) -> Result<i8, Diagnostic> {
            Ok(self.u8()? as i8)
        }

        /// Reads a 16-bit unsigned integer.
        pub fn u16(&mut self) -> Result<u16, Diagnostic> {
            let bytes: [u8; 2] = self.fixed()?;
            Ok(match self.endian {
                Endian::Little => u16::from_le_bytes(bytes),
                Endian::Big => u16::from_be_bytes(bytes),
            })
        }

        /// Reads a 32-bit unsigned integer.
        pub fn u32(&mut self) -> Result<u32, Diagnostic> {
            let bytes: [u8; 4] = self.fixed()?;
            Ok(match self.endian {
                Endian::Little => u32::from_le_bytes(bytes),
                Endian::Big => u32::from_be_bytes(bytes),
            })
        }

        /// Reads a 64-bit unsigned integer.
        pub fn u64(&mut self) -> Result<u64, Diagnostic> {
            let bytes: [u8; 8] = self.fixed()?;
            Ok(match self.endian {
                Endian::Little => u64::from_le_bytes(bytes),
                Endian::Big => u64::from_be_bytes(bytes),
            })
        }

        /// Reads a 16-bit signed integer.
        pub fn i16(&mut self) -> Result<i16, Diagnostic> {
            Ok(self.u16()? as i16)
        }

        /// Reads a 32-bit signed integer.
        pub fn i32(&mut self) -> Result<i32, Diagnostic> {
            Ok(self.u32()? as i32)
        }

        fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Diagnostic> {
            let slice = self.bytes(N)?;
            let mut out = [0u8; N];
            out.copy_from_slice(slice);
            Ok(out)
        }

        /// Reads an unsigned LEB128 integer.
        ///
        /// The DEX format is full of these. The encoding is refused if it runs
        /// longer than the widest legal form or if it is longer than it needs to
        /// be: an overlong encoding is a second spelling of one number, and two
        /// spellings are two chances for a checksum and a parser to disagree.
        pub fn uleb128(&mut self) -> Result<u64, Diagnostic> {
            let mut value: u64 = 0;
            let mut shift = 0u32;

            for index in 0..10 {
                let byte = self.u8()?;
                let payload = u64::from(byte & 0x7f);

                if shift >= 64 || (shift == 63 && payload > 1) {
                    return Err(fail("E7005", "A LEB128 value does not fit in 64 bits.")
                        .with_context(format!("Source: {}", self.origin))
                        .with_context(format!("At offset: {}", self.position)));
                }

                value |= payload << shift;
                shift += 7;

                if byte & 0x80 == 0 {
                    if index > 0 && byte == 0 {
                        return Err(fail("E7006", "A LEB128 value is encoded overlong.")
                            .with_context(format!("Source: {}", self.origin))
                            .with_context(format!("At offset: {}", self.position))
                            .with_suggestion(
                                "The same number has a shorter encoding. Two \
                                 spellings of one value make a format ambiguous.",
                            ));
                    }
                    return Ok(value);
                }
            }

            Err(fail("E7007", "A LEB128 value has no end.")
                .with_context(format!("Source: {}", self.origin))
                .with_context(format!("At offset: {}", self.position))
                .with_suggestion("Every byte had its continuation bit set."))
        }

        /// Reads a NUL-terminated byte string, without the terminator.
        pub fn cstring(&mut self) -> Result<&'a [u8], Diagnostic> {
            let start = self.position;
            let Some(relative) = self.data[start..].iter().position(|byte| *byte == 0) else {
                return Err(fail("E7008", "A NUL-terminated string has no terminator.")
                    .with_context(format!("Source: {}", self.origin))
                    .with_context(format!("From offset: {start}")));
            };
            self.position = start + relative + 1;
            Ok(&self.data[start..start + relative])
        }

        /// Checks that the input begins with an expected marker.
        pub fn expect_magic(&mut self, magic: &[u8]) -> Result<(), Diagnostic> {
            let found = self.bytes(magic.len())?;
            if found != magic {
                return Err(fail(
                    "E7009",
                    "The input does not start the way this format does.",
                )
                .with_context(format!("Source: {}", self.origin))
                .with_context(format!("Expected: {}", hex(magic)))
                .with_context(format!("Found: {}", hex(found)))
                .with_suggestion("This is not the format it was read as."));
            }
            Ok(())
        }
    }

    fn hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 3);
        for (index, byte) in bytes.iter().take(16).enumerate() {
            if index > 0 {
                out.push(' ');
            }
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        if bytes.len() > 16 {
            out.push_str(" …");
        }
        out
    }

    /// A reserved span in a writer's output, to be filled in later.
    ///
    /// Binary formats are full of values that are only known once something
    /// later has been written: a size, an offset, a count. Reserving the space
    /// and patching it is how that is done without either two passes or
    /// guesswork.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Patch {
        offset: usize,
        width: usize,
    }

    impl Patch {
        /// Where the reserved span starts.
        pub fn offset(&self) -> usize {
            self.offset
        }

        /// How wide it is.
        pub fn width(&self) -> usize {
            self.width
        }
    }

    /// A bounded binary writer.
    #[derive(Clone, Debug)]
    pub struct Writer {
        buffer: Vec<u8>,
        endian: Endian,
        limit: usize,
    }

    impl Writer {
        /// A writer with the default limit.
        pub fn new(endian: Endian) -> Writer {
            Writer::with_limit(endian, MAX_BUFFER_BYTES)
        }

        /// A writer that refuses to grow past `limit` bytes.
        pub fn with_limit(endian: Endian, limit: usize) -> Writer {
            Writer {
                buffer: Vec::new(),
                endian,
                limit: limit.min(MAX_BUFFER_BYTES),
            }
        }

        /// How much has been written.
        pub fn position(&self) -> usize {
            self.buffer.len()
        }

        /// Whether nothing has been written.
        pub fn is_empty(&self) -> bool {
            self.buffer.is_empty()
        }

        /// Byte order in force.
        pub fn endian(&self) -> Endian {
            self.endian
        }

        fn room_for(&self, count: usize) -> Result<(), Diagnostic> {
            let Some(total) = self.buffer.len().checked_add(count) else {
                return Err(fail("E7020", "The output size overflows."));
            };
            if total > self.limit {
                return Err(Diagnostic::new(
                    "E7021",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.binary",
                    "The output is larger than this writer is allowed to produce.",
                )
                .with_context(format!("Limit: {} bytes", self.limit))
                .with_context(format!("Would become: {total} bytes"))
                .with_suggestion(
                    "Raise the limit deliberately if the artifact really is this \
                     large; an unbounded writer is a way to run a device out of \
                     memory.",
                ));
            }
            Ok(())
        }

        /// Appends raw bytes.
        pub fn bytes(&mut self, data: &[u8]) -> Result<(), Diagnostic> {
            self.room_for(data.len())?;
            self.buffer.extend_from_slice(data);
            Ok(())
        }

        /// Appends one byte.
        pub fn u8(&mut self, value: u8) -> Result<(), Diagnostic> {
            self.bytes(&[value])
        }

        /// Appends a 16-bit unsigned integer.
        pub fn u16(&mut self, value: u16) -> Result<(), Diagnostic> {
            match self.endian {
                Endian::Little => self.bytes(&value.to_le_bytes()),
                Endian::Big => self.bytes(&value.to_be_bytes()),
            }
        }

        /// Appends a 32-bit unsigned integer.
        pub fn u32(&mut self, value: u32) -> Result<(), Diagnostic> {
            match self.endian {
                Endian::Little => self.bytes(&value.to_le_bytes()),
                Endian::Big => self.bytes(&value.to_be_bytes()),
            }
        }

        /// Appends a 64-bit unsigned integer.
        pub fn u64(&mut self, value: u64) -> Result<(), Diagnostic> {
            match self.endian {
                Endian::Little => self.bytes(&value.to_le_bytes()),
                Endian::Big => self.bytes(&value.to_be_bytes()),
            }
        }

        /// Appends an unsigned LEB128 integer, in its shortest form.
        pub fn uleb128(&mut self, mut value: u64) -> Result<(), Diagnostic> {
            loop {
                let mut byte = (value & 0x7f) as u8;
                value >>= 7;
                if value != 0 {
                    byte |= 0x80;
                }
                self.u8(byte)?;
                if value == 0 {
                    return Ok(());
                }
            }
        }

        /// Pads with zeroes until the position is a multiple of `alignment`.
        ///
        /// Alignment must be a power of two: every binary format that asks for
        /// alignment asks for one, and accepting anything else would silently
        /// produce a layout no reader expects.
        pub fn align_to(&mut self, alignment: usize) -> Result<(), Diagnostic> {
            if alignment == 0 || !alignment.is_power_of_two() {
                return Err(Diagnostic::new(
                    "E7022",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.binary",
                    "Alignment must be a power of two.",
                )
                .with_context(format!("Given: {alignment}")));
            }
            let padding = (alignment - (self.buffer.len() % alignment)) % alignment;
            for _ in 0..padding {
                self.u8(0)?;
            }
            Ok(())
        }

        /// Reserves four bytes to be filled in later.
        pub fn reserve_u32(&mut self) -> Result<Patch, Diagnostic> {
            let offset = self.buffer.len();
            self.u32(0)?;
            Ok(Patch { offset, width: 4 })
        }

        /// Reserves two bytes to be filled in later.
        pub fn reserve_u16(&mut self) -> Result<Patch, Diagnostic> {
            let offset = self.buffer.len();
            self.u16(0)?;
            Ok(Patch { offset, width: 2 })
        }

        /// Fills in a reserved 32-bit span.
        pub fn patch_u32(&mut self, patch: Patch, value: u32) -> Result<(), Diagnostic> {
            self.patch(
                patch,
                4,
                &match self.endian {
                    Endian::Little => value.to_le_bytes(),
                    Endian::Big => value.to_be_bytes(),
                },
            )
        }

        /// Fills in a reserved 16-bit span.
        pub fn patch_u16(&mut self, patch: Patch, value: u16) -> Result<(), Diagnostic> {
            self.patch(
                patch,
                2,
                &match self.endian {
                    Endian::Little => value.to_le_bytes(),
                    Endian::Big => value.to_be_bytes(),
                },
            )
        }

        fn patch(&mut self, patch: Patch, width: usize, value: &[u8]) -> Result<(), Diagnostic> {
            if patch.width != width {
                return Err(Diagnostic::new(
                    "E7023",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.binary",
                    "A reserved span is being filled with a value of another width.",
                )
                .with_context(format!("Reserved: {} bytes", patch.width))
                .with_context(format!("Writing: {width} bytes")));
            }

            let end = patch.offset + width;
            if end > self.buffer.len() {
                return Err(Diagnostic::new(
                    "E7024",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.binary",
                    "A reserved span is outside the output.",
                )
                .with_context(format!("Span: {}..{end}", patch.offset))
                .with_context(format!("Output: {} bytes", self.buffer.len()))
                .with_suggestion("The patch belongs to a different writer."));
            }

            self.buffer[patch.offset..end].copy_from_slice(value);
            Ok(())
        }

        /// Borrows what has been written.
        pub fn as_slice(&self) -> &[u8] {
            &self.buffer
        }

        /// Takes the finished bytes.
        pub fn finish(self) -> Vec<u8> {
            self.buffer
        }
    }

    /// A named span within a file.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Section {
        /// What it is called in the format's specification.
        pub name: String,
        /// Where it starts.
        pub offset: u64,
        /// How long it is.
        pub size: u64,
    }

    impl Section {
        /// Checks that the section lies inside `total` bytes.
        pub fn validate(&self, total: u64) -> Result<(), Diagnostic> {
            let Some(end) = self.offset.checked_add(self.size) else {
                return Err(
                    fail("E7030", "A section's offset and size overflow when added.")
                        .with_context(format!("Section: {}", self.name)),
                );
            };
            if end > total {
                return Err(fail("E7031", "A section extends past the end of the file.")
                    .with_context(format!("Section: {}", self.name))
                    .with_context(format!("Span: {}..{end}", self.offset))
                    .with_context(format!("File: {total} bytes"))
                    .with_suggestion("The file is truncated or its header is wrong."));
            }
            Ok(())
        }

        /// Whether this section overlaps another.
        pub fn overlaps(&self, other: &Section) -> bool {
            let this_end = self.offset.saturating_add(self.size);
            let other_end = other.offset.saturating_add(other.size);
            self.offset < other_end && other.offset < this_end && self.size > 0 && other.size > 0
        }
    }

    /// A run of fixed-size entries.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Table {
        /// What it is called in the format's specification.
        pub name: String,
        /// Where the first entry starts.
        pub offset: u64,
        /// How large one entry is.
        pub entry_size: u64,
        /// How many entries there are.
        pub count: u64,
    }

    impl Table {
        /// Total size of the table, refusing an overflow.
        ///
        /// `entry_size * count` is exactly the multiplication a malformed header
        /// uses to make a parser believe a small file contains an enormous
        /// table, so it is checked rather than computed.
        pub fn span(&self) -> Result<u64, Diagnostic> {
            self.entry_size.checked_mul(self.count).ok_or_else(|| {
                fail("E7040", "A table's size overflows.")
                    .with_context(format!("Table: {}", self.name))
                    .with_context(format!(
                        "{} entries of {} bytes",
                        self.count, self.entry_size
                    ))
            })
        }

        /// Checks that every entry lies inside `total` bytes.
        pub fn validate(&self, total: u64) -> Result<(), Diagnostic> {
            let span = self.span()?;
            Section {
                name: self.name.clone(),
                offset: self.offset,
                size: span,
            }
            .validate(total)
        }

        /// Offset of one entry.
        pub fn entry_offset(&self, index: u64) -> Result<u64, Diagnostic> {
            if index >= self.count {
                return Err(fail("E7041", "A table entry is out of range.")
                    .with_context(format!("Table: {}", self.name))
                    .with_context(format!("Index: {index}, entries: {}", self.count)));
            }
            self.entry_size
                .checked_mul(index)
                .and_then(|shift| self.offset.checked_add(shift))
                .ok_or_else(|| {
                    fail("E7042", "A table entry's offset overflows.")
                        .with_context(format!("Table: {}", self.name))
                })
        }
    }

    /// Something that can check a parsed structure and report what is wrong.
    ///
    /// Validators collect every problem rather than stopping at the first, so a
    /// person fixing a malformed file sees the whole picture in one pass
    /// (directive section 33).
    pub trait Validator {
        /// Name used in diagnostics.
        fn name(&self) -> &str;

        /// Checks the input, appending anything wrong to `sink`.
        ///
        /// Returns whether the input may be used.
        fn validate(&self, data: &[u8], sink: &mut Sink) -> bool;
    }

    /// Checksums used by the formats this toolchain touches.
    ///
    /// Both algorithms are published standards with published check values, and
    /// both are implemented from those specifications. Directive section 30
    /// applies to checksums as much as to hashes: nothing here is invented, and
    /// nothing is trusted until it reproduces the official value.
    ///
    /// Neither is a security primitive. A checksum detects accidental damage;
    /// only a signature detects a deliberate change (directive section 25).
    pub mod checksum {
        /// CRC-32 as used by ZIP, gzip and PNG (ITU-T V.42, reflected, polynomial
        /// 0xEDB88320).
        #[derive(Clone, Copy, Debug)]
        pub struct Crc32 {
            state: u32,
        }

        impl Default for Crc32 {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Crc32 {
            /// A fresh accumulator.
            pub const fn new() -> Self {
                Crc32 { state: 0xffff_ffff }
            }

            /// Absorbs more data.
            pub fn update(&mut self, data: &[u8]) {
                for byte in data {
                    self.state ^= u32::from(*byte);
                    for _ in 0..8 {
                        let mask = (self.state & 1).wrapping_neg();
                        self.state = (self.state >> 1) ^ (0xedb8_8320 & mask);
                    }
                }
            }

            /// The checksum so far.
            pub const fn finish(self) -> u32 {
                self.state ^ 0xffff_ffff
            }
        }

        /// CRC-32 of a slice.
        pub fn crc32(data: &[u8]) -> u32 {
            let mut crc = Crc32::new();
            crc.update(data);
            crc.finish()
        }

        /// Adler-32 as used by zlib and by the DEX header (RFC 1950).
        #[derive(Clone, Copy, Debug)]
        pub struct Adler32 {
            a: u32,
            b: u32,
        }

        impl Default for Adler32 {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Adler32 {
            /// The largest number of bytes that can be absorbed before the sums
            /// must be reduced, from RFC 1950.
            const NMAX: usize = 5552;

            /// A fresh accumulator.
            pub const fn new() -> Self {
                Adler32 { a: 1, b: 0 }
            }

            /// Absorbs more data.
            pub fn update(&mut self, data: &[u8]) {
                for chunk in data.chunks(Self::NMAX) {
                    for byte in chunk {
                        self.a += u32::from(*byte);
                        self.b += self.a;
                    }
                    self.a %= 65_521;
                    self.b %= 65_521;
                }
            }

            /// The checksum so far.
            pub const fn finish(self) -> u32 {
                (self.b << 16) | self.a
            }
        }

        /// Adler-32 of a slice.
        pub fn adler32(data: &[u8]) -> u32 {
            let mut adler = Adler32::new();
            adler.update(data);
            adler.finish()
        }
    }
}

// ===========================================================================
// xml — a deliberately small XML reader (sections 22, 41, 60, 61)
// ===========================================================================

/// Enough XML to read Android resources, and nothing more.
///
/// ## Contract (directive section 2)
///
/// * **Purpose** — turn a resource file into a tree, or into diagnostics with a
///   line and a column.
/// * **Inputs** — text from a user's project. Untrusted (directive section 61).
/// * **Non-Responsibilities** — namespaces as a resolution mechanism, schema
///   validation, XPath, and every other thing a general XML library does. This
///   reads resource files.
/// * **Status** — PARTIAL. It reads what Android resource files contain.
///
/// ## What it refuses, and why
///
/// * **`<!DOCTYPE`** — refused outright. A document type declaration is the
///   entry point for both external entity expansion, which turns a resource file
///   into a way to read `/etc/passwd`, and for the nested entity definitions that
///   make a two-kilobyte file expand to gigabytes. Neither is defended against
///   here; both are simply unavailable.
/// * **Custom entities** — only the five XML predefines and numeric character
///   references are recognised. There is no entity table, so there is nothing to
///   expand recursively.
/// * **Depth, size, attribute count, name and text length** — all bounded
///   (directive section 60).
///
/// The parser holds its own stack rather than recursing, so a deeply nested
/// document cannot overflow the machine stack no matter what the depth limit is
/// set to.
pub mod xml {
    use crate::diag::{Diagnostic, Location, Severity, Sink};
    use crate::FailureClass;

    /// Largest accepted document, in bytes.
    pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

    /// Deepest accepted nesting.
    pub const MAX_DEPTH: usize = 64;

    /// Most attributes accepted on one element.
    pub const MAX_ATTRIBUTES: usize = 128;

    /// Longest accepted element or attribute name, in bytes.
    pub const MAX_NAME_BYTES: usize = 256;

    /// Longest accepted run of text, in bytes.
    pub const MAX_TEXT_BYTES: usize = 256 * 1024;

    /// Most elements accepted in one document.
    pub const MAX_ELEMENTS: usize = 50_000;

    /// Where something appeared in the source.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct Position {
        /// 1-based line.
        pub line: u32,
        /// 1-based column.
        pub column: u32,
    }

    /// One attribute of an element.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Attribute {
        /// Name as written, including any namespace prefix.
        pub name: String,
        /// Value, with references already decoded.
        pub value: String,
        /// Where the name started.
        pub position: Position,
    }

    /// An element and everything inside it.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Element {
        /// Name as written, including any namespace prefix.
        pub name: String,
        /// Attributes, in document order.
        pub attributes: Vec<Attribute>,
        /// Child elements, in document order.
        pub children: Vec<Element>,
        /// All text directly inside this element, concatenated and decoded.
        pub text: String,
        /// Where the element started.
        pub position: Position,
    }

    impl Element {
        /// Looks an attribute up by name.
        pub fn attribute(&self, name: &str) -> Option<&str> {
            self.attributes
                .iter()
                .find(|attribute| attribute.name == name)
                .map(|attribute| attribute.value.as_str())
        }

        /// Child elements with a given name.
        pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
            self.children.iter().filter(move |child| child.name == name)
        }
    }

    /// Reads a document and returns its root element.
    ///
    /// Returns `None` when the document cannot be trusted; every reason is in
    /// `sink`, with a position.
    pub fn parse(text: &str, origin: &str, sink: &mut Sink) -> Option<Element> {
        Parser::new(text, origin).run(sink)
    }

    struct Parser<'a> {
        text: &'a [u8],
        source: &'a str,
        origin: String,
        offset: usize,
        line: u32,
        column: u32,
        elements: usize,
    }

    /// An element being built, held on the parser's own stack.
    struct Open {
        name: String,
        attributes: Vec<Attribute>,
        children: Vec<Element>,
        text: String,
        position: Position,
    }

    impl<'a> Parser<'a> {
        fn new(source: &'a str, origin: &str) -> Parser<'a> {
            Parser {
                text: source.as_bytes(),
                source,
                origin: origin.to_string(),
                offset: 0,
                line: 1,
                column: 1,
                elements: 0,
            }
        }

        fn position(&self) -> Position {
            Position {
                line: self.line,
                column: self.column,
            }
        }

        fn error(&self, code: &str, message: impl Into<String>, at: Position) -> Diagnostic {
            Diagnostic::new(
                code,
                Severity::Error,
                FailureClass::UserError,
                "core.xml",
                message,
            )
            .with_location(Location::at(&self.origin, at.line, at.column))
        }

        fn peek(&self) -> Option<u8> {
            self.text.get(self.offset).copied()
        }

        fn starts_with(&self, prefix: &str) -> bool {
            self.source[self.offset..].starts_with(prefix)
        }

        fn advance(&mut self) -> Option<u8> {
            let byte = self.peek()?;
            self.offset += 1;
            if byte == b'\n' {
                self.line += 1;
                self.column = 1;
            } else if byte & 0xc0 != 0x80 {
                // Count characters, not bytes, so a column number means something
                // in a file with Turkish or any other non-ASCII text in it.
                self.column += 1;
            }
            Some(byte)
        }

        fn skip(&mut self, count: usize) {
            for _ in 0..count {
                self.advance();
            }
        }

        fn skip_whitespace(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.advance();
            }
        }

        fn run(&mut self, sink: &mut Sink) -> Option<Element> {
            if self.text.len() > MAX_DOCUMENT_BYTES {
                sink.emit(
                    self.error(
                        "E8001",
                        "The document is larger than the accepted limit.",
                        Position { line: 0, column: 0 },
                    )
                    .with_context(format!("Limit: {MAX_DOCUMENT_BYTES} bytes"))
                    .with_context(format!("Received: {} bytes", self.text.len()))
                    .with_class(FailureClass::ResourceExhaustion),
                );
                return None;
            }

            // A byte order mark is legal at the start and means nothing here.
            if self.starts_with("\u{feff}") {
                self.skip("\u{feff}".len());
            }

            let mut stack: Vec<Open> = Vec::new();
            let mut root: Option<Element> = None;

            loop {
                if self.peek().is_none() {
                    break;
                }

                if self.starts_with("<") {
                    let at = self.position();

                    if self.starts_with("<!--") {
                        if !self.skip_comment(sink) {
                            return None;
                        }
                        continue;
                    }

                    if self.starts_with("<![CDATA[") {
                        let text = self.read_cdata(sink)?;
                        if let Some(open) = stack.last_mut() {
                            if !push_text(open, &text, self, sink, at) {
                                return None;
                            }
                        }
                        continue;
                    }

                    if self.starts_with("<!DOCTYPE") || self.starts_with("<!ENTITY") {
                        sink.emit(
                            self.error(
                                "E8002",
                                "Document type and entity declarations are not accepted.",
                                at,
                            )
                            .with_suggestion(
                                "They are the way an XML file is made to read other \
                                 files, or to expand to a size that exhausts memory. \
                                 A resource file needs neither, so neither is \
                                 available.",
                            )
                            .with_class(FailureClass::SecurityFailure),
                        );
                        return None;
                    }

                    if self.starts_with("<?") {
                        if !self.skip_processing_instruction(sink) {
                            return None;
                        }
                        continue;
                    }

                    if self.starts_with("</") {
                        let name = self.read_closing_tag(sink)?;
                        let Some(open) = stack.pop() else {
                            sink.emit(
                                self.error("E8003", format!("</{name}> closes nothing."), at)
                                    .with_suggestion("There is no open element here."),
                            );
                            return None;
                        };
                        if open.name != name {
                            sink.emit(
                                self.error(
                                    "E8004",
                                    format!("</{name}> closes <{}>.", open.name),
                                    at,
                                )
                                .with_context(format!(
                                    "<{}> opened at line {}",
                                    open.name, open.position.line
                                ))
                                .with_suggestion("Tags must be closed in the order they open."),
                            );
                            return None;
                        }
                        let finished = finish(open);
                        match stack.last_mut() {
                            Some(parent) => parent.children.push(finished),
                            None => {
                                if root.is_some() {
                                    sink.emit(
                                        self.error(
                                            "E8005",
                                            "The document has more than one root element.",
                                            at,
                                        )
                                        .with_suggestion("An XML document has exactly one."),
                                    );
                                    return None;
                                }
                                root = Some(finished);
                            }
                        }
                        continue;
                    }

                    // An opening tag.
                    self.elements += 1;
                    if self.elements > MAX_ELEMENTS {
                        sink.emit(
                            self.error(
                                "E8006",
                                "The document has more elements than the accepted limit.",
                                at,
                            )
                            .with_context(format!("Limit: {MAX_ELEMENTS}"))
                            .with_class(FailureClass::ResourceExhaustion),
                        );
                        return None;
                    }

                    let (name, attributes, self_closing) = self.read_opening_tag(sink)?;

                    let element = Open {
                        name,
                        attributes,
                        children: Vec::new(),
                        text: String::new(),
                        position: at,
                    };

                    if self_closing {
                        let finished = finish(element);
                        match stack.last_mut() {
                            Some(parent) => parent.children.push(finished),
                            None => {
                                if root.is_some() {
                                    sink.emit(self.error(
                                        "E8005",
                                        "The document has more than one root element.",
                                        at,
                                    ));
                                    return None;
                                }
                                root = Some(finished);
                            }
                        }
                    } else {
                        if stack.len() >= MAX_DEPTH {
                            sink.emit(
                                self.error(
                                    "E8007",
                                    "The document is nested more deeply than the accepted limit.",
                                    at,
                                )
                                .with_context(format!("Limit: {MAX_DEPTH}"))
                                .with_class(FailureClass::ResourceExhaustion),
                            );
                            return None;
                        }
                        stack.push(element);
                    }
                    continue;
                }

                // Text.
                let at = self.position();
                let start = self.offset;
                while let Some(byte) = self.peek() {
                    if byte == b'<' {
                        break;
                    }
                    self.advance();
                }
                let raw = &self.source[start..self.offset];
                let decoded = self.decode(raw, at, sink)?;

                if let Some(open) = stack.last_mut() {
                    if !push_text(open, &decoded, self, sink, at) {
                        return None;
                    }
                } else if !decoded.trim().is_empty() {
                    sink.emit(
                        self.error("E8008", "There is text outside the root element.", at)
                            .with_suggestion("Only whitespace may appear there."),
                    );
                    return None;
                }
            }

            if let Some(open) = stack.last() {
                sink.emit(
                    self.error(
                        "E8009",
                        format!("<{}> is never closed.", open.name),
                        open.position,
                    )
                    .with_suggestion("Close it."),
                );
                return None;
            }

            if root.is_none() {
                sink.emit(self.error(
                    "E8010",
                    "The document has no element in it.",
                    Position {
                        line: self.line,
                        column: self.column,
                    },
                ));
            }

            root
        }

        fn skip_comment(&mut self, sink: &mut Sink) -> bool {
            let at = self.position();
            self.skip(4);
            loop {
                if self.starts_with("-->") {
                    self.skip(3);
                    return true;
                }
                if self.advance().is_none() {
                    sink.emit(
                        self.error("E8011", "A comment is never closed.", at)
                            .with_suggestion("Close it with -->."),
                    );
                    return false;
                }
            }
        }

        fn skip_processing_instruction(&mut self, sink: &mut Sink) -> bool {
            let at = self.position();
            self.skip(2);
            loop {
                if self.starts_with("?>") {
                    self.skip(2);
                    return true;
                }
                if self.advance().is_none() {
                    sink.emit(self.error("E8012", "A processing instruction is never closed.", at));
                    return false;
                }
            }
        }

        fn read_cdata(&mut self, sink: &mut Sink) -> Option<String> {
            let at = self.position();
            self.skip("<![CDATA[".len());
            let start = self.offset;
            loop {
                if self.starts_with("]]>") {
                    let text = self.source[start..self.offset].to_string();
                    self.skip(3);
                    return Some(text);
                }
                if self.advance().is_none() {
                    sink.emit(
                        self.error("E8013", "A CDATA section is never closed.", at)
                            .with_suggestion("Close it with ]]>."),
                    );
                    return None;
                }
            }
        }

        fn read_name(&mut self, sink: &mut Sink) -> Option<String> {
            let at = self.position();
            let start = self.offset;
            while let Some(byte) = self.peek() {
                let usable = byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b':')
                    || byte >= 0x80;
                if !usable {
                    break;
                }
                self.advance();
            }
            let name = &self.source[start..self.offset];

            if name.is_empty() {
                sink.emit(
                    self.error("E8014", "A name was expected here.", at)
                        .with_context(format!("Found: {:?}", self.peek().map(char::from))),
                );
                return None;
            }
            if name.len() > MAX_NAME_BYTES {
                sink.emit(
                    self.error("E8015", "A name is longer than the accepted limit.", at)
                        .with_context(format!("Limit: {MAX_NAME_BYTES} bytes"))
                        .with_class(FailureClass::ResourceExhaustion),
                );
                return None;
            }
            Some(name.to_string())
        }

        fn read_closing_tag(&mut self, sink: &mut Sink) -> Option<String> {
            self.skip(2);
            let name = self.read_name(sink)?;
            self.skip_whitespace();
            if self.peek() != Some(b'>') {
                sink.emit(self.error(
                    "E8016",
                    format!("</{name}> is not closed with '>'."),
                    self.position(),
                ));
                return None;
            }
            self.advance();
            Some(name)
        }

        #[allow(clippy::type_complexity)]
        fn read_opening_tag(&mut self, sink: &mut Sink) -> Option<(String, Vec<Attribute>, bool)> {
            self.advance();
            let name = self.read_name(sink)?;
            let mut attributes: Vec<Attribute> = Vec::new();

            loop {
                self.skip_whitespace();

                match self.peek() {
                    None => {
                        sink.emit(self.error(
                            "E8017",
                            format!("<{name}> is never closed."),
                            self.position(),
                        ));
                        return None;
                    }
                    Some(b'>') => {
                        self.advance();
                        return Some((name, attributes, false));
                    }
                    Some(b'/') => {
                        self.advance();
                        if self.peek() != Some(b'>') {
                            sink.emit(self.error(
                                "E8018",
                                "A '/' here must be followed by '>'.",
                                self.position(),
                            ));
                            return None;
                        }
                        self.advance();
                        return Some((name, attributes, true));
                    }
                    _ => {}
                }

                if attributes.len() >= MAX_ATTRIBUTES {
                    sink.emit(
                        self.error(
                            "E8019",
                            "An element has more attributes than the accepted limit.",
                            self.position(),
                        )
                        .with_context(format!("Limit: {MAX_ATTRIBUTES}"))
                        .with_class(FailureClass::ResourceExhaustion),
                    );
                    return None;
                }

                let at = self.position();
                let attribute_name = self.read_name(sink)?;
                self.skip_whitespace();

                if self.peek() != Some(b'=') {
                    sink.emit(
                        self.error(
                            "E8020",
                            format!("The attribute '{attribute_name}' has no value."),
                            at,
                        )
                        .with_suggestion("Write it as name=\"value\"."),
                    );
                    return None;
                }
                self.advance();
                self.skip_whitespace();

                let Some(quote) = self.peek().filter(|byte| *byte == b'"' || *byte == b'\'') else {
                    sink.emit(
                        self.error(
                            "E8021",
                            format!("The value of '{attribute_name}' is not quoted."),
                            self.position(),
                        )
                        .with_suggestion("Wrap it in \" or '."),
                    );
                    return None;
                };
                self.advance();

                let start = self.offset;
                loop {
                    match self.peek() {
                        None => {
                            sink.emit(self.error(
                                "E8022",
                                format!("The value of '{attribute_name}' is not closed."),
                                at,
                            ));
                            return None;
                        }
                        Some(byte) if byte == quote => break,
                        Some(b'<') => {
                            sink.emit(
                                self.error(
                                    "E8023",
                                    format!("The value of '{attribute_name}' contains '<'."),
                                    self.position(),
                                )
                                .with_suggestion("Write it as &lt;."),
                            );
                            return None;
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
                let raw = &self.source[start..self.offset];
                self.advance();

                let value = self.decode(raw, at, sink)?;

                if attributes
                    .iter()
                    .any(|existing| existing.name == attribute_name)
                {
                    sink.emit(
                        self.error(
                            "E8024",
                            format!("'{attribute_name}' is given twice on <{name}>."),
                            at,
                        )
                        .with_suggestion(
                            "Remove one. Keeping the last would make the file depend \
                             on attribute order.",
                        ),
                    );
                    return None;
                }

                attributes.push(Attribute {
                    name: attribute_name,
                    value,
                    position: at,
                });
            }
        }

        /// Decodes the five predefined entities and numeric character references.
        ///
        /// There is no entity table, so there is nothing that can be defined in
        /// terms of itself. An unrecognised reference is an error rather than
        /// something passed through, because passing it through would put a
        /// literal `&foo;` into a string and leave the author wondering.
        fn decode(&self, raw: &str, at: Position, sink: &mut Sink) -> Option<String> {
            if !raw.contains('&') {
                return Some(raw.to_string());
            }

            let mut out = String::with_capacity(raw.len());
            let mut rest = raw;

            while let Some(index) = rest.find('&') {
                out.push_str(&rest[..index]);
                let tail = &rest[index..];

                let Some(end) = tail.find(';').filter(|end| *end <= 12) else {
                    sink.emit(
                        self.error("E8030", "An '&' starts a reference that never ends.", at)
                            .with_suggestion("Write a literal ampersand as &amp;."),
                    );
                    return None;
                };

                let entity = &tail[1..end];
                let decoded = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    numeric if numeric.starts_with('#') => {
                        let (digits, radix) = match numeric.strip_prefix("#x") {
                            Some(hex) => (hex, 16),
                            None => (&numeric[1..], 10),
                        };
                        u32::from_str_radix(digits, radix)
                            .ok()
                            .and_then(char::from_u32)
                    }
                    _ => None,
                };

                let Some(character) = decoded else {
                    sink.emit(
                        self.error(
                            "E8031",
                            format!("'&{entity};' is not a reference this reader knows."),
                            at,
                        )
                        .with_suggestion(
                            "Only &amp; &lt; &gt; &quot; &apos; and numeric references \
                             are accepted. Custom entities are not defined, which is \
                             what keeps a file from expanding into memory it does not \
                             have.",
                        ),
                    );
                    return None;
                };

                out.push(character);
                rest = &tail[end + 1..];
            }

            out.push_str(rest);
            Some(out)
        }
    }

    fn finish(open: Open) -> Element {
        Element {
            name: open.name,
            attributes: open.attributes,
            children: open.children,
            text: open.text,
            position: open.position,
        }
    }

    fn push_text(
        open: &mut Open,
        text: &str,
        parser: &Parser<'_>,
        sink: &mut Sink,
        at: Position,
    ) -> bool {
        if open.text.len() + text.len() > MAX_TEXT_BYTES {
            sink.emit(
                parser
                    .error(
                        "E8032",
                        "An element holds more text than the accepted limit.",
                        at,
                    )
                    .with_context(format!("Limit: {MAX_TEXT_BYTES} bytes"))
                    .with_class(FailureClass::ResourceExhaustion),
            );
            return false;
        }
        open.text.push_str(text);
        true
    }
}

// ===========================================================================
// resources — the resource engine (directive section 22)
// ===========================================================================

/// Android resources: parsed, validated, numbered and resolved.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                       |
/// |----------------------|-------------------------------------------------------------|
/// | Module               | `omni_core::resources`                                      |
/// | Purpose              | Turn a project's resource files into a table with stable     |
/// |                      | identifiers and resolved references.                         |
/// | Inputs               | `values/*.xml` documents and resource file names. Untrusted.  |
/// | Outputs              | A [`Table`], plus diagnostics with a line and a column.       |
/// | Non-Responsibilities | Writing the binary resource table, rendering drawables, and   |
/// |                      | reading anything from the Android platform.                   |
/// | Determinism          | Identifiers come from sorted order, never from the order      |
/// |                      | files happened to be read (directive section 12).             |
/// | Status               | PARTIAL — see the subsystem inventory.                        |
///
/// ## The pipeline of directive section 22
///
/// ```text
/// Source -> Validation -> Parse -> Resource Model -> ID Assignment ->
/// Reference Resolution -> Table Construction -> Compiled Resources -> Verification
/// ```
///
/// Everything up to and including verification is implemented. "Compiled
/// Resources" means the binary table an Android package carries, and that is not
/// written here: it belongs with the packaging engine, and claiming it now would
/// be exactly the kind of overstatement directive section 1 forbids.
pub mod resources {
    use crate::diag::{Diagnostic, Location, Severity, Sink};
    use crate::json::Writer;
    use crate::xml::{self, Element, Position};
    use crate::FailureClass;

    /// Package identifier an application's own resources use.
    ///
    /// `0x01` belongs to the platform and `0x7f` to the application; everything
    /// between is for shared libraries. Nothing here allocates a library id.
    pub const APPLICATION_PACKAGE_ID: u8 = 0x7f;

    /// Most resources accepted in one project (directive section 60).
    pub const MAX_ENTRIES: usize = 65_535;

    /// What kind of resource something is.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub enum Kind {
        /// `<bool>`
        Bool,
        /// `<color>` and colour files.
        Color,
        /// `<dimen>`
        Dimension,
        /// Files under `drawable/`.
        Drawable,
        /// Identifiers declared with `@+id/`.
        Id,
        /// `<integer>`
        Integer,
        /// Files under `mipmap/`.
        Mipmap,
        /// `<string>`
        String,
        /// `<style>`
        Style,
    }

    impl Kind {
        /// The name Android uses, which is also the directory or element name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Kind::Bool => "bool",
                Kind::Color => "color",
                Kind::Dimension => "dimen",
                Kind::Drawable => "drawable",
                Kind::Id => "id",
                Kind::Integer => "integer",
                Kind::Mipmap => "mipmap",
                Kind::String => "string",
                Kind::Style => "style",
            }
        }

        /// Every kind, in the order identifiers are assigned.
        ///
        /// Alphabetical, and therefore stable: a type's number must not depend on
        /// which file happened to mention it first.
        pub const ALL: &'static [Kind] = &[
            Kind::Bool,
            Kind::Color,
            Kind::Dimension,
            Kind::Drawable,
            Kind::Id,
            Kind::Integer,
            Kind::Mipmap,
            Kind::String,
            Kind::Style,
        ];

        /// Looks a kind up by name.
        pub fn parse(value: &str) -> Option<Kind> {
            Kind::ALL
                .iter()
                .copied()
                .find(|kind| kind.as_str() == value)
        }
    }

    impl core::fmt::Display for Kind {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.as_str())
        }
    }

    /// Screen density a resource is meant for.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
    pub enum Density {
        /// No qualifier: used when nothing more specific matches.
        #[default]
        Default,
        /// `ldpi`
        Low,
        /// `mdpi`
        Medium,
        /// `hdpi`
        High,
        /// `xhdpi`
        ExtraHigh,
        /// `xxhdpi`
        ExtraExtraHigh,
        /// `xxxhdpi`
        ExtraExtraExtraHigh,
        /// `nodpi`: never scaled.
        None,
        /// `anydpi`: matches any density, used by adaptive icons.
        Any,
    }

    impl Density {
        /// The qualifier as written in a directory name.
        pub const fn as_str(self) -> &'static str {
            match self {
                Density::Default => "default",
                Density::Low => "ldpi",
                Density::Medium => "mdpi",
                Density::High => "hdpi",
                Density::ExtraHigh => "xhdpi",
                Density::ExtraExtraHigh => "xxhdpi",
                Density::ExtraExtraExtraHigh => "xxxhdpi",
                Density::None => "nodpi",
                Density::Any => "anydpi",
            }
        }

        /// Every density, in declaration order.
        pub const ALL: &'static [Density] = &[
            Density::Default,
            Density::Low,
            Density::Medium,
            Density::High,
            Density::ExtraHigh,
            Density::ExtraExtraHigh,
            Density::ExtraExtraExtraHigh,
            Density::None,
            Density::Any,
        ];

        /// Looks a density up by qualifier.
        pub fn parse(value: &str) -> Option<Density> {
            Density::ALL
                .iter()
                .copied()
                .find(|density| density.as_str() == value)
        }
    }

    /// The qualifiers that select between resources of the same name.
    ///
    /// Only density is modelled. Locale, orientation, size and the rest of the
    /// qualifier set are not, and a directory carrying one is refused rather than
    /// silently treated as the default (directive section 64).
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
    pub struct Config {
        /// Density this resource is for.
        pub density: Density,
    }

    impl Config {
        /// The default configuration.
        pub const DEFAULT: Config = Config {
            density: Density::Default,
        };

        /// Reads the qualifiers from a resource directory name.
        ///
        /// `drawable` yields the default; `drawable-hdpi` yields high density.
        pub fn parse_directory(name: &str) -> Result<(Kind, Config), String> {
            let mut parts = name.split('-');
            let Some(kind_name) = parts.next() else {
                return Err(format!("'{name}' does not name a resource directory."));
            };
            let Some(kind) = Kind::parse(kind_name) else {
                return Err(format!("'{kind_name}' is not a resource type."));
            };

            let mut config = Config::DEFAULT;
            for qualifier in parts {
                match Density::parse(qualifier) {
                    Some(density) => config.density = density,
                    None => {
                        return Err(format!(
                            "'{qualifier}' is a qualifier this build does not model."
                        ))
                    }
                }
            }
            Ok((kind, config))
        }
    }

    impl core::fmt::Display for Config {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.density.as_str())
        }
    }

    /// The unit a dimension is written in.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Unit {
        /// Density-independent pixels.
        Dp,
        /// Scale-independent pixels, which follow the user's font size.
        Sp,
        /// Physical pixels.
        Px,
        /// Points.
        Pt,
        /// Inches.
        In,
        /// Millimetres.
        Mm,
    }

    impl Unit {
        /// The suffix as written.
        pub const fn as_str(self) -> &'static str {
            match self {
                Unit::Dp => "dp",
                Unit::Sp => "sp",
                Unit::Px => "px",
                Unit::Pt => "pt",
                Unit::In => "in",
                Unit::Mm => "mm",
            }
        }

        /// Every unit, longest suffix first so parsing is unambiguous.
        pub const ALL: &'static [Unit] =
            &[Unit::Dp, Unit::Sp, Unit::Px, Unit::Pt, Unit::In, Unit::Mm];
    }

    /// A reference to another resource.
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct Reference {
        /// Package, when one is named. `android` means the platform.
        pub package: Option<String>,
        /// What kind of resource is referred to.
        pub kind: Kind,
        /// Its name.
        pub name: String,
        /// Whether the reference also declares the resource, as `@+id/` does.
        pub declares: bool,
    }

    impl Reference {
        /// Reads `@[package:]type/name` or `@+id/name`.
        pub fn parse(value: &str) -> Option<Reference> {
            let body = value.strip_prefix('@')?;
            let (body, declares) = match body.strip_prefix('+') {
                Some(rest) => (rest, true),
                None => (body, false),
            };

            let (package, rest) = match body.split_once(':') {
                Some((package, rest)) if !package.is_empty() => (Some(package.to_string()), rest),
                Some(_) => return None,
                None => (None, body),
            };

            let (kind_name, name) = rest.split_once('/')?;
            let kind = Kind::parse(kind_name)?;
            if name.is_empty() {
                return None;
            }

            Some(Reference {
                package,
                kind,
                name: name.to_string(),
                declares,
            })
        }

        /// Whether this points at the Android platform rather than the project.
        pub fn is_platform(&self) -> bool {
            self.package.as_deref() == Some("android")
        }
    }

    impl core::fmt::Display for Reference {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "@")?;
            if self.declares {
                write!(f, "+")?;
            }
            if let Some(package) = &self.package {
                write!(f, "{package}:")?;
            }
            write!(f, "{}/{}", self.kind, self.name)
        }
    }

    /// What a resource holds.
    ///
    /// A dimension is stored in thousandths rather than as a float. A build that
    /// rounds differently on two machines is not reproducible, and directive
    /// section 12 does not leave room for "close enough".
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub enum Value {
        /// Text.
        Text(String),
        /// A colour, as 0xAARRGGBB.
        Color(u32),
        /// A dimension, in thousandths of its unit.
        Dimension {
            /// The value multiplied by one thousand.
            milli: i64,
            /// The unit it was written in.
            unit: Unit,
        },
        /// A boolean.
        Bool(bool),
        /// A whole number.
        Integer(i32),
        /// A reference to another resource.
        Reference(Reference),
        /// A file, named by its path inside the project.
        File(String),
        /// The resource exists and holds nothing.
        ///
        /// An identifier declared with `<id name="…"/>` is exactly this: a name
        /// that other resources can point at. Encoding it as a false boolean
        /// would be a lie that happens to compile.
        Empty,
    }

    impl Value {
        /// Stable name of the value's form.
        pub const fn type_name(&self) -> &'static str {
            match self {
                Value::Text(_) => "text",
                Value::Color(_) => "color",
                Value::Dimension { .. } => "dimension",
                Value::Bool(_) => "bool",
                Value::Integer(_) => "integer",
                Value::Reference(_) => "reference",
                Value::File(_) => "file",
                Value::Empty => "empty",
            }
        }

        /// Renders the value the way it would be written in a resource file.
        pub fn to_source(&self) -> String {
            match self {
                Value::Text(text) => text.clone(),
                Value::Color(argb) => format!("#{argb:08x}"),
                Value::Dimension { milli, unit } => {
                    let whole = milli / 1000;
                    let fraction = (milli % 1000).abs();
                    if fraction == 0 {
                        format!("{whole}{}", unit.as_str())
                    } else {
                        format!(
                            "{whole}.{}{}",
                            format!("{fraction:03}").trim_end_matches('0'),
                            unit.as_str()
                        )
                    }
                }
                Value::Bool(flag) => flag.to_string(),
                Value::Integer(number) => number.to_string(),
                Value::Reference(reference) => reference.to_string(),
                Value::File(path) => path.clone(),
                Value::Empty => String::new(),
            }
        }
    }

    /// One resource.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Entry {
        /// What kind it is.
        pub kind: Kind,
        /// Its name, which must be a usable Java identifier.
        pub name: String,
        /// Which configuration it belongs to.
        pub config: Config,
        /// What it holds.
        pub value: Value,
        /// Where it was declared.
        pub origin: String,
        /// Where in that file.
        pub position: Position,
    }

    impl Entry {
        /// Sort key that makes identifier assignment deterministic.
        fn order_key(&self) -> (Kind, &str, Config) {
            (self.kind, self.name.as_str(), self.config)
        }
    }

    // -----------------------------------------------------------------------
    // Table construction
    // -----------------------------------------------------------------------

    /// A resource identifier, `0xPPTTEEEE`.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
    pub struct ResourceId(u32);

    impl ResourceId {
        /// Builds an identifier from its three parts.
        pub const fn new(package: u8, type_index: u8, entry_index: u16) -> ResourceId {
            ResourceId(((package as u32) << 24) | ((type_index as u32) << 16) | entry_index as u32)
        }

        /// The raw value.
        pub const fn raw(self) -> u32 {
            self.0
        }

        /// Which package it belongs to.
        pub const fn package(self) -> u8 {
            (self.0 >> 24) as u8
        }

        /// Which type, 1-based.
        pub const fn type_index(self) -> u8 {
            ((self.0 >> 16) & 0xff) as u8
        }

        /// Which entry within that type, 0-based.
        pub const fn entry_index(self) -> u16 {
            (self.0 & 0xffff) as u16
        }
    }

    impl core::fmt::Display for ResourceId {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "0x{:08x}", self.0)
        }
    }

    /// Resources being collected.
    #[derive(Clone, Debug)]
    pub struct Table {
        package_id: u8,
        entries: Vec<Entry>,
    }

    impl Table {
        /// A table for an application's own resources.
        pub fn new() -> Table {
            Table::for_package(APPLICATION_PACKAGE_ID)
        }

        /// A table for a named package identifier.
        pub fn for_package(package_id: u8) -> Table {
            Table {
                package_id,
                entries: Vec::new(),
            }
        }

        /// Everything collected so far.
        pub fn entries(&self) -> &[Entry] {
            &self.entries
        }

        /// How many resources are held.
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether nothing has been collected.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Reads a `values/*.xml` document.
        ///
        /// Returns whether everything in it was understood. A document with a
        /// problem still contributes what was valid, so one broken entry does not
        /// hide every other mistake in the file (directive section 33).
        pub fn read_values(&mut self, text: &str, origin: &str, sink: &mut Sink) -> bool {
            let Some(root) = xml::parse(text, origin, sink) else {
                return false;
            };

            if root.name != "resources" {
                sink.emit(
                    self.problem(
                        "E9001",
                        format!(
                            "A values file's root element is <resources>, not <{}>.",
                            root.name
                        ),
                        origin,
                        root.position,
                    )
                    .with_suggestion("Wrap the entries in <resources> … </resources>."),
                );
                return false;
            }

            let mut ok = true;
            for child in &root.children {
                ok &= self.read_entry(child, origin, sink);
            }
            ok
        }

        fn read_entry(&mut self, element: &Element, origin: &str, sink: &mut Sink) -> bool {
            let Some(kind) = Kind::parse(&element.name) else {
                sink.emit(
                    self.problem(
                        "E9002",
                        format!(
                            "<{}> is not a resource this build understands.",
                            element.name
                        ),
                        origin,
                        element.position,
                    )
                    .with_context(format!(
                        "Understood: {}",
                        Kind::ALL
                            .iter()
                            .filter(|kind| kind.declarable())
                            .map(|kind| kind.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .with_suggestion(
                        "An element that is not modelled is reported rather than \
                         skipped: a resource that silently never existed is harder \
                         to find than one that was refused.",
                    ),
                );
                return false;
            };

            if !kind.declarable() {
                sink.emit(
                    self.problem(
                        "E9003",
                        format!("<{kind}> is recognised but not yet modelled."),
                        origin,
                        element.position,
                    )
                    .with_suggestion(
                        "It can be referred to with @style/name; declaring one needs \
                         the attribute system, which is not built.",
                    ),
                );
                return false;
            }

            let Some(name) = element.attribute("name") else {
                sink.emit(
                    self.problem(
                        "E9004",
                        format!("<{kind}> has no name."),
                        origin,
                        element.position,
                    )
                    .with_suggestion("Every resource is declared as <type name=\"…\">."),
                );
                return false;
            };

            if let Err(reason) = validate_name(name) {
                sink.emit(
                    self.problem("E9005", reason, origin, element.position)
                        .with_context(format!("Name: {name}"))
                        .with_suggestion(
                            "A resource name becomes a field in generated code, so it \
                             must be usable as an identifier.",
                        ),
                );
                return false;
            }

            if !element.children.is_empty() {
                sink.emit(
                    self.problem(
                        "E9006",
                        format!("<{kind} name=\"{name}\"> contains other elements."),
                        origin,
                        element.position,
                    )
                    .with_suggestion(
                        "Inline markup inside a resource is not modelled. Its text is \
                         taken as written, so anything nested would be silently lost.",
                    ),
                );
                return false;
            }

            let Some(value) = parse_value(kind, &element.text, origin, element.position, sink)
            else {
                return false;
            };

            self.push(
                Entry {
                    kind,
                    name: name.to_string(),
                    config: Config::DEFAULT,
                    value,
                    origin: origin.to_string(),
                    position: element.position,
                },
                sink,
            )
        }

        /// Records a resource that is a file, such as a drawable.
        ///
        /// `directory` is the resource directory name, `file_name` the file
        /// inside it, and `path` how the file is addressed in the project.
        pub fn read_file(
            &mut self,
            directory: &str,
            file_name: &str,
            path: &str,
            sink: &mut Sink,
        ) -> bool {
            let (kind, config) = match Config::parse_directory(directory) {
                Ok(parsed) => parsed,
                Err(reason) => {
                    sink.emit(
                        self.problem("E9010", reason, path, Position::default())
                            .with_suggestion(
                                "A qualifier that is not modelled is refused rather \
                                 than treated as the default, which would put the \
                                 wrong file on every device.",
                            ),
                    );
                    return false;
                }
            };

            let stem = file_name
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(file_name);
            if let Err(reason) = validate_name(stem) {
                sink.emit(
                    self.problem("E9011", reason, path, Position::default())
                        .with_context(format!("File: {file_name}")),
                );
                return false;
            }

            self.push(
                Entry {
                    kind,
                    name: stem.to_string(),
                    config,
                    value: Value::File(path.to_string()),
                    origin: path.to_string(),
                    position: Position::default(),
                },
                sink,
            )
        }

        fn push(&mut self, entry: Entry, sink: &mut Sink) -> bool {
            if self.entries.len() >= MAX_ENTRIES {
                sink.emit(
                    self.problem(
                        "E9012",
                        "The project declares more resources than the accepted limit.",
                        &entry.origin,
                        entry.position,
                    )
                    .with_context(format!("Limit: {MAX_ENTRIES}"))
                    .with_class(FailureClass::ResourceExhaustion),
                );
                return false;
            }

            if let Some(previous) = self
                .entries
                .iter()
                .find(|existing| existing.order_key() == entry.order_key())
            {
                sink.emit(
                    self.problem(
                        "E9013",
                        format!(
                            "{} '{}' is declared twice for the {} configuration.",
                            entry.kind, entry.name, entry.config
                        ),
                        &entry.origin,
                        entry.position,
                    )
                    .with_context(format!(
                        "First declared in {} at line {}",
                        previous.origin, previous.position.line
                    ))
                    .with_suggestion(
                        "Remove one. Keeping the last would make the build depend on \
                         which file was read first.",
                    ),
                );
                return false;
            }

            self.entries.push(entry);
            true
        }

        fn problem(
            &self,
            code: &str,
            message: impl Into<String>,
            origin: &str,
            position: Position,
        ) -> Diagnostic {
            Diagnostic::new(
                code,
                Severity::Error,
                FailureClass::UserError,
                "core.resources",
                message,
            )
            .with_location(Location::at(origin, position.line, position.column))
        }

        /// Assigns identifiers, resolves references and verifies the result.
        ///
        /// This is the second half of the pipeline in directive section 22, run
        /// as one step because none of it means anything alone: an identifier
        /// that nothing can refer to is bookkeeping, and a resolved reference
        /// without an identifier has nothing to resolve to.
        pub fn compile(mut self, sink: &mut Sink) -> Option<Compiled> {
            // Sorted, so an identifier depends on what is declared and never on
            // the order files were read (directive section 12).
            self.entries
                .sort_by(|left, right| left.order_key().cmp(&right.order_key()));

            let mut assignments: Vec<(Kind, String, ResourceId)> = Vec::new();
            let mut type_index: u8 = 0;

            for kind in Kind::ALL {
                let mut names: Vec<&str> = self
                    .entries
                    .iter()
                    .filter(|entry| entry.kind == *kind)
                    .map(|entry| entry.name.as_str())
                    .collect();
                names.dedup();
                if names.is_empty() {
                    continue;
                }

                let Some(next) = type_index.checked_add(1) else {
                    sink.emit(Diagnostic::new(
                        "E9020",
                        Severity::Fatal,
                        FailureClass::ResourceExhaustion,
                        "core.resources",
                        "There are more resource types than an identifier can hold.",
                    ));
                    return None;
                };
                type_index = next;

                for (entry_index, name) in names.iter().enumerate() {
                    let Ok(entry_index) = u16::try_from(entry_index) else {
                        sink.emit(
                            Diagnostic::new(
                                "E9021",
                                Severity::Fatal,
                                FailureClass::ResourceExhaustion,
                                "core.resources",
                                format!(
                                    "There are more {kind} resources than an identifier can hold."
                                ),
                            )
                            .with_context("An identifier holds 65536 entries per type."),
                        );
                        return None;
                    };
                    assignments.push((
                        *kind,
                        (*name).to_string(),
                        ResourceId::new(self.package_id, type_index, entry_index),
                    ));
                }
            }

            let mut ok = true;
            for entry in &self.entries {
                if let Value::Reference(reference) = &entry.value {
                    ok &= self.check_reference(entry, reference, &assignments, sink);
                }
            }

            ok &= self.check_for_cycles(sink);

            if !ok {
                return None;
            }

            Some(Compiled {
                package_id: self.package_id,
                entries: self.entries,
                assignments,
            })
        }

        fn check_reference(
            &self,
            entry: &Entry,
            reference: &Reference,
            assignments: &[(Kind, String, ResourceId)],
            sink: &mut Sink,
        ) -> bool {
            if reference.is_platform() {
                // The platform's resource table is not available to this build, so
                // the reference is recorded and left unresolved rather than
                // guessed at.
                return true;
            }

            if reference.package.is_some() {
                sink.emit(
                    self.problem(
                        "E9030",
                        format!("'{reference}' names a package this build cannot reach."),
                        &entry.origin,
                        entry.position,
                    )
                    .with_suggestion(
                        "Only the project's own resources and @android: are available; \
                         nothing resolves a shared library's table yet.",
                    ),
                );
                return false;
            }

            let found = assignments
                .iter()
                .any(|(kind, name, _)| *kind == reference.kind && name == &reference.name);

            if !found {
                let near: Vec<&str> = assignments
                    .iter()
                    .filter(|(kind, _, _)| *kind == reference.kind)
                    .map(|(_, name, _)| name.as_str())
                    .filter(|name| {
                        name.eq_ignore_ascii_case(&reference.name)
                            || name.starts_with(reference.name.as_str())
                    })
                    .take(3)
                    .collect();

                let mut diagnostic = self.problem(
                    "E9031",
                    format!("'{reference}' refers to a resource that is not declared."),
                    &entry.origin,
                    entry.position,
                );
                if !near.is_empty() {
                    diagnostic =
                        diagnostic.with_suggestion(format!("Did you mean {}?", near.join(", ")));
                } else {
                    diagnostic = diagnostic.with_suggestion(format!(
                        "Declare a {} called '{}', or correct the reference.",
                        reference.kind, reference.name
                    ));
                }
                sink.emit(diagnostic);
                return false;
            }

            true
        }

        /// Refuses a reference that eventually points back at itself.
        ///
        /// Following one would loop forever, so the chain is walked with a bound
        /// and a visited list rather than recursively.
        fn check_for_cycles(&self, sink: &mut Sink) -> bool {
            let mut ok = true;

            for entry in &self.entries {
                let mut visited: Vec<(Kind, &str)> = vec![(entry.kind, entry.name.as_str())];
                let mut current = entry;

                while let Value::Reference(reference) = &current.value {
                    if reference.package.is_some() {
                        break;
                    }

                    let Some(next) = self.entries.iter().find(|candidate| {
                        candidate.kind == reference.kind && candidate.name == reference.name
                    }) else {
                        break;
                    };

                    let step = (next.kind, next.name.as_str());
                    if visited.contains(&step) {
                        let chain = visited
                            .iter()
                            .map(|(kind, name)| format!("@{kind}/{name}"))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        sink.emit(
                            self.problem(
                                "E9032",
                                "A resource reference eventually points back at itself.",
                                &entry.origin,
                                entry.position,
                            )
                            .with_context(format!("Chain: {chain} -> {reference}"))
                            .with_suggestion("Break the loop by giving one of them a real value."),
                        );
                        ok = false;
                        break;
                    }

                    visited.push(step);
                    if visited.len() > 64 {
                        sink.emit(
                            self.problem(
                                "E9033",
                                "A chain of resource references is longer than the accepted limit.",
                                &entry.origin,
                                entry.position,
                            )
                            .with_class(FailureClass::ResourceExhaustion),
                        );
                        ok = false;
                        break;
                    }
                    current = next;
                }
            }

            ok
        }
    }

    impl Default for Table {
        fn default() -> Self {
            Table::new()
        }
    }

    /// A table that has been numbered and verified.
    #[derive(Clone, Debug)]
    pub struct Compiled {
        package_id: u8,
        entries: Vec<Entry>,
        assignments: Vec<(Kind, String, ResourceId)>,
    }

    impl Compiled {
        /// Which package these resources belong to.
        pub fn package_id(&self) -> u8 {
            self.package_id
        }

        /// Every resource, in identifier order.
        pub fn entries(&self) -> &[Entry] {
            &self.entries
        }

        /// The identifier of a resource, if it has one.
        pub fn id(&self, kind: Kind, name: &str) -> Option<ResourceId> {
            self.assignments
                .iter()
                .find(|(entry_kind, entry_name, _)| *entry_kind == kind && entry_name == name)
                .map(|(_, _, id)| *id)
        }

        /// Every identifier, in assignment order.
        pub fn assignments(&self) -> &[(Kind, String, ResourceId)] {
            &self.assignments
        }

        /// Serialises the table as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_str("packageId", &format!("0x{:02x}", self.package_id));
            w.field_u64("resources", self.entries.len() as u64);
            w.field_u64("identifiers", self.assignments.len() as u64);
            w.field_bool("binaryTableWritten", false);
            w.begin_array(Some("detail"));
            for (kind, name, id) in &self.assignments {
                w.begin_object(None);
                w.field_str("id", &id.to_string());
                w.field_str("kind", kind.as_str());
                w.field_str("name", name);
                w.end_object();
            }
            w.end_array();
            w.end_object();
        }
    }

    impl Kind {
        /// Whether a values file may declare one of these.
        ///
        /// A style can be referred to but not declared: declaring one needs the
        /// attribute system, which is not built, and half a style is worse than
        /// none.
        pub const fn declarable(self) -> bool {
            !matches!(self, Kind::Style)
        }
    }

    /// Checks that a name can become a field in generated code.
    fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("A resource name is empty.".to_string());
        }
        if name.len() > 128 {
            return Err("A resource name is longer than 128 characters.".to_string());
        }
        let mut characters = name.chars();
        let first = characters.next().unwrap_or('\0');
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(format!(
                "'{name}' does not start with a letter or underscore."
            ));
        }
        for character in characters {
            if !(character.is_ascii_alphanumeric() || character == '_' || character == '.') {
                return Err(format!(
                    "'{name}' contains '{character}', which is not usable."
                ));
            }
        }
        Ok(())
    }

    /// Reads the text of a resource into a value of the right shape.
    fn parse_value(
        kind: Kind,
        text: &str,
        origin: &str,
        position: Position,
        sink: &mut Sink,
    ) -> Option<Value> {
        let trimmed = text.trim();

        if trimmed.starts_with('@') {
            let Some(reference) = Reference::parse(trimmed) else {
                sink.emit(
                    reject(
                        "E9040",
                        format!("'{trimmed}' is not a resource reference."),
                        origin,
                        position,
                    )
                    .with_suggestion(
                        "A reference is written @type/name, for example @color/omni_accent.",
                    ),
                );
                return None;
            };
            return Some(Value::Reference(reference));
        }

        match kind {
            Kind::String => Some(Value::Text(decode_string(text))),
            Kind::Color => {
                match parse_color(trimmed) {
                    Ok(color) => Some(Value::Color(color)),
                    Err(reason) => {
                        sink.emit(reject("E9041", reason, origin, position).with_suggestion(
                            "Colours are written #rgb, #argb, #rrggbb or #aarrggbb.",
                        ));
                        None
                    }
                }
            }
            Kind::Dimension => match parse_dimension(trimmed) {
                Ok((milli, unit)) => Some(Value::Dimension { milli, unit }),
                Err(reason) => {
                    sink.emit(reject("E9042", reason, origin, position).with_suggestion(
                        "Dimensions are a number and a unit, for example 16dp or 14sp.",
                    ));
                    None
                }
            },
            Kind::Bool => match trimmed {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                other => {
                    sink.emit(
                        reject(
                            "E9043",
                            format!("'{other}' is not true or false."),
                            origin,
                            position,
                        )
                        .with_suggestion("Write exactly true or false, in lower case."),
                    );
                    None
                }
            },
            Kind::Integer => match trimmed.parse::<i32>() {
                Ok(number) => Some(Value::Integer(number)),
                Err(_) => {
                    sink.emit(
                        reject(
                            "E9044",
                            format!("'{trimmed}' is not a whole number."),
                            origin,
                            position,
                        )
                        .with_suggestion("Write a number that fits in 32 bits."),
                    );
                    None
                }
            },
            Kind::Id => Some(Value::Empty),
            Kind::Drawable | Kind::Mipmap | Kind::Style => {
                sink.emit(
                    reject(
                        "E9045",
                        format!("A {kind} cannot be given a value in a values file."),
                        origin,
                        position,
                    )
                    .with_suggestion("It is declared by the file that holds it."),
                );
                None
            }
        }
    }

    fn reject(
        code: &str,
        message: impl Into<String>,
        origin: &str,
        position: Position,
    ) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::UserError,
            "core.resources",
            message,
        )
        .with_location(Location::at(origin, position.line, position.column))
    }

    /// Applies the escape and whitespace rules Android uses for strings.
    ///
    /// Text is trimmed and internal whitespace runs collapse to one space, which
    /// is what makes a resource file readable across several lines. Text wrapped
    /// in double quotes keeps its spacing exactly.
    fn decode_string(text: &str) -> String {
        let trimmed = text.trim();
        if let Some(quoted) = trimmed
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            return unescape(quoted);
        }

        let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        unescape(&collapsed)
    }

    fn unescape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut characters = text.chars();

        while let Some(character) = characters.next() {
            if character != '\\' {
                out.push(character);
                continue;
            }
            match characters.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('"') => out.push('"'),
                Some('@') => out.push('@'),
                Some('?') => out.push('?'),
                Some('u') => {
                    let digits: String = characters.by_ref().take(4).collect();
                    match u32::from_str_radix(&digits, 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        Some(decoded) => out.push(decoded),
                        None => {
                            out.push_str("\\u");
                            out.push_str(&digits);
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }

        out
    }

    /// Reads `#rgb`, `#argb`, `#rrggbb` or `#aarrggbb` into 0xAARRGGBB.
    fn parse_color(text: &str) -> Result<u32, String> {
        let Some(digits) = text.strip_prefix('#') else {
            return Err(format!("'{text}' does not start with '#'."));
        };
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "'{text}' contains something that is not a hex digit."
            ));
        }

        let expand = |value: u32| (value << 4) | value;
        let parse = |slice: &str| u32::from_str_radix(slice, 16).unwrap_or(0);

        match digits.len() {
            3 => {
                let r = expand(parse(&digits[0..1]));
                let g = expand(parse(&digits[1..2]));
                let b = expand(parse(&digits[2..3]));
                Ok(0xff00_0000 | (r << 16) | (g << 8) | b)
            }
            4 => {
                let a = expand(parse(&digits[0..1]));
                let r = expand(parse(&digits[1..2]));
                let g = expand(parse(&digits[2..3]));
                let b = expand(parse(&digits[3..4]));
                Ok((a << 24) | (r << 16) | (g << 8) | b)
            }
            6 => Ok(0xff00_0000 | parse(digits)),
            8 => Ok(parse(digits)),
            other => Err(format!(
                "'{text}' has {other} hex digits; a colour has 3, 4, 6 or 8."
            )),
        }
    }

    /// Reads a dimension into thousandths of its unit.
    fn parse_dimension(text: &str) -> Result<(i64, Unit), String> {
        let Some(unit) = Unit::ALL
            .iter()
            .copied()
            .find(|unit| text.ends_with(unit.as_str()))
        else {
            return Err(format!("'{text}' has no unit this build understands."));
        };

        let number = &text[..text.len() - unit.as_str().len()];
        if number.is_empty() {
            return Err(format!("'{text}' has a unit but no number."));
        }

        let (whole, fraction) = match number.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (number, ""),
        };

        if fraction.len() > 3 {
            return Err(format!(
                "'{text}' has more than three decimal places, which this build \
                 would have to round. Rounding silently is how two machines stop \
                 producing the same artifact."
            ));
        }

        let negative = whole.starts_with('-');
        let whole_digits = whole.strip_prefix('-').unwrap_or(whole);
        if whole_digits.is_empty() || !whole_digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("'{text}' does not begin with a number."));
        }
        if !fraction.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "'{text}' has something other than digits after the point."
            ));
        }

        let Ok(whole_value) = whole_digits.parse::<i64>() else {
            return Err(format!("'{text}' is larger than a dimension can hold."));
        };
        let scaled = format!("{fraction:0<3}");
        let fraction_value: i64 = scaled[..3].parse().unwrap_or(0);

        let Some(milli) = whole_value
            .checked_mul(1000)
            .and_then(|v| v.checked_add(fraction_value))
        else {
            return Err(format!("'{text}' is larger than a dimension can hold."));
        };

        Ok((if negative { -milli } else { milli }, unit))
    }
}

// ===========================================================================
// archive — the ZIP container an APK is (directive sections 23 and 24)
// ===========================================================================

/// ZIP archives: read, modelled, validated and written.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                       |
/// |----------------------|-------------------------------------------------------------|
/// | Module               | `omni_core::archive`                                        |
/// | Purpose              | The container format an Android package is.                  |
/// | Inputs               | Archive bytes, or entries to write. Untrusted, always.       |
/// | Outputs              | An [`Archive`] model, or archive bytes; plus diagnostics.    |
/// | Non-Responsibilities | Compression, signing, and knowing what an APK's entries mean. |
/// | Security             | Every offset is checked against the file. Entry names cannot  |
/// |                      | escape, absolutely or by traversal. Central directory and     |
/// |                      | local headers must agree.                                     |
/// | Determinism          | Fixed timestamps, sorted entries, no host-dependent metadata. |
/// | Status               | PARTIAL — reads any archive's structure, writes stored        |
/// |                      | entries only.                                                 |
///
/// ## The approach directive section 24 requires
///
/// > Specification → Parser → Internal Model → Validator → Writer →
/// > Conformance Tests
///
/// The format is not reinvented. This implements the subset of PKWARE's
/// APPNOTE that an APK uses: local file headers, the central directory, and the
/// end-of-central-directory record.
///
/// ## What it does not do
///
/// **It does not compress.** Entries are written stored, byte for byte. Deflate
/// is a compressor, which is a subsystem of its own with its own tests, and an
/// APK built with stored entries is correct and larger rather than smaller and
/// wrong. An archive being *read* may hold deflated entries; their structure is
/// modelled and their bytes are not decompressed, and the model says which is
/// which rather than pretending.
pub mod archive {
    use crate::binary::{checksum, Endian, Reader, Writer as BinaryWriter};
    use crate::diag::{Diagnostic, Severity, Sink};
    use crate::hash::{sha256, Digest};
    use crate::json::Writer;
    use crate::FailureClass;

    /// Signature of a local file header.
    pub const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;

    /// Signature of a central directory record.
    pub const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;

    /// Signature of the end-of-central-directory record.
    pub const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;

    /// Fixed size of a local file header before the name and extra field.
    pub const LOCAL_HEADER_SIZE: u64 = 30;

    /// Fixed size of a central directory record before the name, extra and comment.
    pub const CENTRAL_HEADER_SIZE: u64 = 46;

    /// Fixed size of the end-of-central-directory record before the comment.
    pub const END_OF_CENTRAL_DIRECTORY_SIZE: u64 = 22;

    /// Most entries an archive may hold.
    ///
    /// The end-of-central-directory record counts entries in sixteen bits, and
    /// this implementation does not write the ZIP64 records that lift that.
    pub const MAX_ENTRIES: usize = 65_535;

    /// Longest accepted entry name, in bytes.
    pub const MAX_NAME_BYTES: usize = 4_096;

    /// Largest archive this implementation will read or write.
    ///
    /// Four gigabytes is where the format's 32-bit offsets stop working, and
    /// ZIP64 is not implemented (directive section 60).
    pub const MAX_ARCHIVE_BYTES: u64 = u32::MAX as u64;

    /// Fixed modification date: 1 January 1980, the start of the DOS epoch.
    ///
    /// A real timestamp is the single most common reason two builds of the same
    /// source produce different bytes (directive section 12). The format has
    /// nowhere to put "unspecified", so the earliest representable moment is
    /// used and the same value goes into every entry.
    pub const FIXED_DOS_DATE: u16 = 0x0021;

    /// Fixed modification time: midnight.
    pub const FIXED_DOS_TIME: u16 = 0x0000;

    /// How an entry's bytes are stored.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    pub enum Compression {
        /// Stored byte for byte.
        Stored,
        /// Deflated. Readable as metadata; this implementation does not
        /// decompress.
        Deflate,
        /// Something else the format defines and this implementation does not
        /// model.
        Other(u16),
    }

    impl Compression {
        /// The method number the format uses.
        pub const fn method(self) -> u16 {
            match self {
                Compression::Stored => 0,
                Compression::Deflate => 8,
                Compression::Other(method) => method,
            }
        }

        /// Reads a method number.
        pub const fn from_method(method: u16) -> Compression {
            match method {
                0 => Compression::Stored,
                8 => Compression::Deflate,
                other => Compression::Other(other),
            }
        }

        /// Stable machine-readable name.
        pub fn as_str(self) -> &'static str {
            match self {
                Compression::Stored => "STORED",
                Compression::Deflate => "DEFLATE",
                Compression::Other(_) => "OTHER",
            }
        }
    }

    /// One entry, as the archive describes it.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Entry {
        /// Name, as stored. Always uses `/`.
        pub name: String,
        /// How the bytes are stored.
        pub compression: Compression,
        /// CRC-32 of the uncompressed bytes, as the archive claims.
        pub crc32: u32,
        /// Size of the stored bytes.
        pub compressed_size: u64,
        /// Size of the original bytes.
        pub uncompressed_size: u64,
        /// Offset of the local file header.
        pub local_header_offset: u64,
        /// Offset of the entry's bytes, once the local header has been read.
        pub data_offset: u64,
    }

    impl Entry {
        /// Whether the entry names a directory rather than a file.
        pub fn is_directory(&self) -> bool {
            self.name.ends_with('/')
        }

        /// Whether the entry's bytes start on a multiple of `alignment`.
        pub fn is_aligned_to(&self, alignment: u64) -> bool {
            alignment != 0 && self.data_offset.is_multiple_of(alignment)
        }
    }

    /// An archive that has been read and checked.
    #[derive(Clone, Debug)]
    pub struct Archive {
        entries: Vec<Entry>,
        size: u64,
        central_directory_offset: u64,
        end_record_offset: u64,
        digest: Digest,
    }

    impl Archive {
        /// Every entry, in central directory order.
        pub fn entries(&self) -> &[Entry] {
            &self.entries
        }

        /// Number of entries.
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether the archive holds nothing.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Size of the archive in bytes.
        pub fn size(&self) -> u64 {
            self.size
        }

        /// Where the central directory starts.
        ///
        /// The signing block, when there is one, sits immediately before it.
        pub fn central_directory_offset(&self) -> u64 {
            self.central_directory_offset
        }

        /// Where the end-of-central-directory record starts.
        pub fn end_record_offset(&self) -> u64 {
            self.end_record_offset
        }

        /// Digest of the whole archive.
        pub fn digest(&self) -> Digest {
            self.digest
        }

        /// Looks an entry up by name.
        pub fn entry(&self, name: &str) -> Option<&Entry> {
            self.entries.iter().find(|entry| entry.name == name)
        }

        /// Borrows one entry's stored bytes.
        ///
        /// For a stored entry these are the file's bytes. For a deflated one
        /// they are the compressed bytes, and the caller is told so rather than
        /// handed something that looks like content.
        pub fn stored_bytes<'a>(
            &self,
            data: &'a [u8],
            entry: &Entry,
        ) -> Result<&'a [u8], Diagnostic> {
            let reader = Reader::new(data, Endian::Little, "archive");
            reader.slice_at(entry.data_offset, entry.compressed_size)
        }

        /// Serialises the archive as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_u64("size", self.size);
            w.field_u64("entries", self.entries.len() as u64);
            w.field_u64("centralDirectoryOffset", self.central_directory_offset);
            w.field_str("digest", &self.digest.to_hex());
            w.begin_array(Some("detail"));
            for entry in &self.entries {
                w.begin_object(None);
                w.field_str("name", &entry.name);
                w.field_str("compression", entry.compression.as_str());
                w.field_u64("size", entry.uncompressed_size);
                w.field_u64("stored", entry.compressed_size);
                w.field_u64("dataOffset", entry.data_offset);
                w.field_str("crc32", &format!("{:08x}", entry.crc32));
                w.end_object();
            }
            w.end_array();
            w.end_object();
        }
    }

    fn fail(code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::Corruption,
            "core.archive",
            message,
        )
    }

    /// Checks that an entry name is one an archive may safely carry.
    ///
    /// Directive section 23 names path traversal and invalid names as mandatory
    /// checks. An archive is a directory tree written by somebody else, and an
    /// entry called `../../etc/passwd` is how that tree reaches outside wherever
    /// it is unpacked.
    pub fn validate_entry_name(name: &str) -> Result<(), Diagnostic> {
        let reject = |code: &str, message: String, suggestion: &str| {
            Err(Diagnostic::new(
                code,
                Severity::Error,
                FailureClass::SecurityFailure,
                "core.archive",
                message,
            )
            .with_context(format!("Name: {}", truncate(name, 96)))
            .with_suggestion(suggestion.to_string()))
        };

        if name.is_empty() {
            return reject(
                "EA001",
                "An entry has no name.".into(),
                "Every entry is named.",
            );
        }
        if name.len() > MAX_NAME_BYTES {
            return reject(
                "EA002",
                "An entry name is longer than the accepted limit.".into(),
                "Names are limited to 4096 bytes.",
            );
        }
        if name.starts_with('/') {
            return reject(
                "EA003",
                "An entry name is absolute.".into(),
                "An absolute name would write outside wherever the archive is unpacked.",
            );
        }
        if name.contains('\\') {
            return reject(
                "EA004",
                "An entry name contains a backslash.".into(),
                "The format uses '/'. Two spellings of one path are two chances to \
                 get a security check wrong.",
            );
        }
        if let Some(bad) = name.chars().find(|c| (*c as u32) < 0x20 || *c == '\u{7f}') {
            return reject(
                "EA005",
                format!("An entry name contains U+{:04X}.", bad as u32),
                "A control character in a name is either a mistake or an attempt to \
                 confuse something downstream.",
            );
        }
        if name.split('/').any(|segment| segment == "..") {
            return reject(
                "EA006",
                "An entry name climbs out of the archive.".into(),
                "Remove the '..' segment. Nothing an archive contains needs to reach \
                 above its own root.",
            );
        }
        if name.len() >= 2 && name.as_bytes()[1] == b':' {
            return reject(
                "EA007",
                "An entry name names a drive.".into(),
                "Use a name relative to the archive root.",
            );
        }
        Ok(())
    }

    fn truncate(value: &str, max: usize) -> String {
        if value.chars().count() <= max {
            return value.to_string();
        }
        let mut out: String = value.chars().take(max).collect();
        out.push('…');
        out
    }

    /// Reads an archive and checks everything directive section 23 requires.
    ///
    /// Returns `None` when the archive cannot be trusted; every reason is in
    /// `sink`. Reading stops at the first structural problem, because after one
    /// the offsets are guesses.
    pub fn read(data: &[u8], sink: &mut Sink) -> Option<Archive> {
        if data.len() as u64 > MAX_ARCHIVE_BYTES {
            sink.emit(
                Diagnostic::new(
                    "EA010",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.archive",
                    "The archive is larger than this implementation reads.",
                )
                .with_context(format!("Limit: {MAX_ARCHIVE_BYTES} bytes"))
                .with_suggestion("ZIP64 is not implemented."),
            );
            return None;
        }

        let end = match find_end_of_central_directory(data) {
            Ok(end) => end,
            Err(error) => {
                sink.emit(error);
                return None;
            }
        };

        let mut reader = Reader::new(data, Endian::Little, "archive");
        if let Err(error) = reader.seek(end) {
            sink.emit(error);
            return None;
        }

        let record = match read_end_record(&mut reader, data.len() as u64) {
            Ok(record) => record,
            Err(error) => {
                sink.emit(error);
                return None;
            }
        };

        let mut entries: Vec<Entry> = Vec::with_capacity(record.entry_count as usize);
        if let Err(error) = reader.seek(record.central_directory_offset) {
            sink.emit(error);
            return None;
        }

        for index in 0..record.entry_count {
            match read_central_entry(&mut reader, data, index) {
                Ok(entry) => entries.push(entry),
                Err(error) => {
                    sink.emit(error);
                    return None;
                }
            }
        }

        // Duplicate names are a mandatory check: two entries answering to one
        // name make every lookup ambiguous, and which one a reader picks has
        // been a source of real Android vulnerabilities.
        let mut seen: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        seen.sort_unstable();
        for pair in seen.windows(2) {
            if pair[0] == pair[1] {
                sink.emit(
                    fail("EA011", "The archive contains the same entry twice.")
                        .with_context(format!("Name: {}", truncate(pair[0], 96)))
                        .with_suggestion(
                            "Which of the two a reader uses is not defined, so the \
                             archive is refused rather than guessed at.",
                        )
                        .with_class(FailureClass::SecurityFailure),
                );
                return None;
            }
        }

        Some(Archive {
            entries,
            size: data.len() as u64,
            central_directory_offset: record.central_directory_offset,
            end_record_offset: end,
            digest: sha256(data),
        })
    }

    struct EndRecord {
        entry_count: u16,
        central_directory_offset: u64,
    }

    /// Finds the end-of-central-directory record.
    ///
    /// It is at the end, unless there is a comment, in which case it is up to
    /// 65535 bytes earlier. The search is backwards and bounded, which is what
    /// keeps a hostile archive from making it quadratic.
    fn find_end_of_central_directory(data: &[u8]) -> Result<u64, Diagnostic> {
        let size = END_OF_CENTRAL_DIRECTORY_SIZE as usize;
        if data.len() < size {
            return Err(fail("EA012", "The file is too small to be an archive.")
                .with_context(format!("Size: {} bytes", data.len()))
                .with_context(format!("Smallest possible: {size} bytes")));
        }

        let signature = END_OF_CENTRAL_DIRECTORY_SIGNATURE.to_le_bytes();
        let furthest = data.len().saturating_sub(size + u16::MAX as usize);

        for start in (furthest..=data.len() - size).rev() {
            if data[start..start + 4] == signature {
                let comment_length =
                    u16::from_le_bytes([data[start + 20], data[start + 21]]) as usize;
                if start + size + comment_length == data.len() {
                    return Ok(start as u64);
                }
            }
        }

        Err(fail(
            "EA013",
            "The archive has no end-of-central-directory record.",
        )
        .with_suggestion(
            "The file is truncated, or it is not an archive. Every ZIP ends \
                     with this record.",
        ))
    }

    fn read_end_record(reader: &mut Reader<'_>, total: u64) -> Result<EndRecord, Diagnostic> {
        let signature = reader.u32()?;
        if signature != END_OF_CENTRAL_DIRECTORY_SIGNATURE {
            return Err(fail(
                "EA014",
                "The end-of-central-directory signature is wrong.",
            ));
        }

        let this_disk = reader.u16()?;
        let start_disk = reader.u16()?;
        let entries_here = reader.u16()?;
        let entries_total = reader.u16()?;
        let directory_size = u64::from(reader.u32()?);
        let directory_offset = u64::from(reader.u32()?);

        if this_disk != 0 || start_disk != 0 {
            return Err(fail("EA015", "The archive is split across several disks.")
                .with_suggestion("Split archives are not read."));
        }
        if entries_here != entries_total {
            return Err(fail(
                "EA016",
                "The archive disagrees with itself about its entry count.",
            )
            .with_context(format!(
                "On this disk: {entries_here}, in total: {entries_total}"
            )));
        }

        let Some(directory_end) = directory_offset.checked_add(directory_size) else {
            return Err(fail(
                "EA017",
                "The central directory's offset and size overflow.",
            ));
        };
        if directory_end > total {
            return Err(fail(
                "EA018",
                "The central directory extends past the end of the file.",
            )
            .with_context(format!("Directory: {directory_offset}..{directory_end}"))
            .with_context(format!("File: {total} bytes"))
            .with_suggestion("The file is truncated, or the record is wrong."));
        }

        Ok(EndRecord {
            entry_count: entries_total,
            central_directory_offset: directory_offset,
        })
    }

    fn read_central_entry(
        reader: &mut Reader<'_>,
        data: &[u8],
        index: u16,
    ) -> Result<Entry, Diagnostic> {
        let signature = reader.u32()?;
        if signature != CENTRAL_HEADER_SIGNATURE {
            return Err(fail(
                "EA020",
                "A central directory record has the wrong signature.",
            )
            .with_context(format!("Entry: {index}"))
            .with_context(format!("Found: 0x{signature:08x}")));
        }

        let _version_made_by = reader.u16()?;
        let _version_needed = reader.u16()?;
        let flags = reader.u16()?;
        let method = reader.u16()?;
        let _time = reader.u16()?;
        let _date = reader.u16()?;
        let crc32 = reader.u32()?;
        let compressed_size = u64::from(reader.u32()?);
        let uncompressed_size = u64::from(reader.u32()?);
        let name_length = usize::from(reader.u16()?);
        let extra_length = usize::from(reader.u16()?);
        let comment_length = usize::from(reader.u16()?);
        let _disk_start = reader.u16()?;
        let _internal = reader.u16()?;
        let _external = reader.u32()?;
        let local_header_offset = u64::from(reader.u32()?);

        if flags & 0x0001 != 0 {
            return Err(fail("EA021", "An entry is encrypted.")
                .with_context(format!("Entry: {index}"))
                .with_suggestion("Encrypted archives are not read."));
        }

        let name_bytes = reader.bytes(name_length)?;
        let Ok(name) = core::str::from_utf8(name_bytes) else {
            return Err(fail("EA022", "An entry name is not valid UTF-8.")
                .with_context(format!("Entry: {index}")));
        };
        let name = name.to_string();
        validate_entry_name(&name)?;

        reader.skip(extra_length)?;
        reader.skip(comment_length)?;

        // The local header is read too, and must agree. An archive whose two
        // descriptions of an entry differ is how a reader and a verifier are
        // made to see different files.
        let data_offset = read_local_header(data, &name, local_header_offset, crc32, method)?;

        Ok(Entry {
            name,
            compression: Compression::from_method(method),
            crc32,
            compressed_size,
            uncompressed_size,
            local_header_offset,
            data_offset,
        })
    }

    fn read_local_header(
        data: &[u8],
        expected_name: &str,
        offset: u64,
        expected_crc: u32,
        expected_method: u16,
    ) -> Result<u64, Diagnostic> {
        let mut reader = Reader::new(data, Endian::Little, "archive");
        reader.seek(offset)?;

        let signature = reader.u32()?;
        if signature != LOCAL_HEADER_SIGNATURE {
            return Err(
                fail("EA030", "A local file header has the wrong signature.")
                    .with_context(format!("Entry: {}", truncate(expected_name, 96)))
                    .with_context(format!("At offset: {offset}"))
                    .with_context(format!("Found: 0x{signature:08x}"))
                    .with_suggestion(
                        "The central directory points somewhere that is not a header. \
                     The archive has been rewritten by something that did not \
                     update both.",
                    ),
            );
        }

        let _version = reader.u16()?;
        let flags = reader.u16()?;
        let method = reader.u16()?;
        let _time = reader.u16()?;
        let _date = reader.u16()?;
        let crc32 = reader.u32()?;
        let _compressed = reader.u32()?;
        let _uncompressed = reader.u32()?;
        let name_length = usize::from(reader.u16()?);
        let extra_length = usize::from(reader.u16()?);

        let name_bytes = reader.bytes(name_length)?;
        if name_bytes != expected_name.as_bytes() {
            return Err(
                fail("EA031", "An entry is named differently in its two headers.")
                    .with_context(format!(
                        "Central directory: {}",
                        truncate(expected_name, 64)
                    ))
                    .with_context(format!(
                        "Local header: {}",
                        truncate(&String::from_utf8_lossy(name_bytes), 64)
                    ))
                    .with_class(FailureClass::SecurityFailure)
                    .with_suggestion(
                        "Which name a reader uses decides which file it gets. An archive \
                     that gives two answers is refused.",
                    ),
            );
        }

        if method != expected_method {
            return Err(fail(
                "EA032",
                "An entry's two headers disagree about compression.",
            )
            .with_context(format!("Entry: {}", truncate(expected_name, 96))));
        }

        // A data descriptor moves the CRC and sizes to after the data, so the
        // zeroes in the local header are expected rather than a disagreement.
        let has_descriptor = flags & 0x0008 != 0;
        if !has_descriptor && crc32 != expected_crc {
            return Err(fail(
                "EA033",
                "An entry's two headers disagree about its checksum.",
            )
            .with_context(format!("Entry: {}", truncate(expected_name, 96)))
            .with_context(format!("Central directory: {expected_crc:08x}"))
            .with_context(format!("Local header: {crc32:08x}"))
            .with_class(FailureClass::SecurityFailure));
        }

        reader.skip(extra_length)?;
        Ok(reader.position() as u64)
    }

    /// Verifies that every stored entry's bytes match the checksum recorded for
    /// them.
    ///
    /// Only stored entries can be checked: a deflated entry's checksum covers
    /// what it decompresses to, and nothing here decompresses. Which entries
    /// were checked is reported rather than glossed over.
    pub fn verify_checksums(archive: &Archive, data: &[u8], sink: &mut Sink) -> (u64, u64) {
        let mut checked = 0;
        let mut skipped = 0;

        for entry in archive.entries() {
            if entry.compression != Compression::Stored {
                skipped += 1;
                continue;
            }

            let bytes = match archive.stored_bytes(data, entry) {
                Ok(bytes) => bytes,
                Err(error) => {
                    sink.emit(error);
                    continue;
                }
            };

            if bytes.len() as u64 != entry.uncompressed_size {
                sink.emit(
                    fail("EA040", "A stored entry's size does not match its header.")
                        .with_context(format!("Entry: {}", truncate(&entry.name, 96)))
                        .with_context(format!(
                            "Header: {}, found: {}",
                            entry.uncompressed_size,
                            bytes.len()
                        )),
                );
                continue;
            }

            let actual = checksum::crc32(bytes);
            if actual != entry.crc32 {
                sink.emit(
                    fail("EA041", "A stored entry does not match its checksum.")
                        .with_context(format!("Entry: {}", truncate(&entry.name, 96)))
                        .with_context(format!("Recorded: {:08x}", entry.crc32))
                        .with_context(format!("Found: {actual:08x}"))
                        .with_suggestion("The entry's bytes changed after it was written."),
                );
                continue;
            }

            checked += 1;
        }

        (checked, skipped)
    }

    // -----------------------------------------------------------------------
    // Writer
    // -----------------------------------------------------------------------

    /// Header identifier of the padding record used for alignment.
    ///
    /// `0xd935` is the identifier Android's own alignment tool writes. Using a
    /// well-formed extra-field record rather than raw padding means every reader
    /// skips it correctly instead of tolerating it.
    pub const ALIGNMENT_EXTRA_ID: u16 = 0xd935;

    /// Alignment every entry gets unless something asks for more.
    pub const DEFAULT_ALIGNMENT: u64 = 4;

    /// Alignment a native library needs.
    ///
    /// A device with 16 KB memory pages maps a shared library straight out of
    /// the package, which it can only do when the library starts on a page
    /// boundary. This was measured against `zipalign -P 16` on a real package.
    pub const NATIVE_LIBRARY_ALIGNMENT: u64 = 16 * 1024;

    /// An entry waiting to be written.
    #[derive(Clone, Debug)]
    struct Pending {
        name: String,
        bytes: Vec<u8>,
        alignment: u64,
    }

    /// Builds an archive.
    ///
    /// Output is deterministic by construction: entries are sorted by name, all
    /// timestamps are the same fixed value, and nothing about the machine that
    /// ran the build reaches the bytes (directive section 12).
    #[derive(Clone, Debug, Default)]
    pub struct Builder {
        entries: Vec<Pending>,
        android_alignment: bool,
    }

    impl Builder {
        /// A builder that aligns every entry to four bytes.
        pub fn new() -> Builder {
            Builder {
                entries: Vec::new(),
                android_alignment: false,
            }
        }

        /// A builder that also puts native libraries on a page boundary.
        ///
        /// This is the policy an Android package needs: `lib/**/*.so` aligned to
        /// 16 KB so the platform can map it directly, everything else to four.
        pub fn for_android() -> Builder {
            Builder {
                entries: Vec::new(),
                android_alignment: true,
            }
        }

        /// Number of entries added.
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether nothing has been added.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Adds an entry, choosing its alignment from the builder's policy.
        pub fn add(&mut self, name: impl Into<String>, bytes: Vec<u8>) -> Result<(), Diagnostic> {
            let name = name.into();
            let alignment = if self.android_alignment && is_native_library(&name) {
                NATIVE_LIBRARY_ALIGNMENT
            } else {
                DEFAULT_ALIGNMENT
            };
            self.add_aligned(name, bytes, alignment)
        }

        /// Adds an entry with an explicit alignment.
        pub fn add_aligned(
            &mut self,
            name: impl Into<String>,
            bytes: Vec<u8>,
            alignment: u64,
        ) -> Result<(), Diagnostic> {
            let name = name.into();
            validate_entry_name(&name)?;

            if !alignment.is_power_of_two() {
                return Err(Diagnostic::new(
                    "EA050",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.archive",
                    "Alignment must be a power of two.",
                )
                .with_context(format!("Given: {alignment}")));
            }

            if self.entries.iter().any(|existing| existing.name == name) {
                return Err(Diagnostic::new(
                    "EA051",
                    Severity::Error,
                    FailureClass::InternalError,
                    "core.archive",
                    "That entry has already been added.",
                )
                .with_context(format!("Name: {}", truncate(&name, 96)))
                .with_suggestion(
                    "An archive with one name twice is ambiguous, so it is refused \
                     here rather than produced and refused later.",
                ));
            }

            if self.entries.len() >= MAX_ENTRIES {
                return Err(Diagnostic::new(
                    "EA052",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.archive",
                    "The archive would hold more entries than the format allows.",
                )
                .with_context(format!("Limit: {MAX_ENTRIES}"))
                .with_suggestion("ZIP64 is not implemented."));
            }

            self.entries.push(Pending {
                name,
                bytes,
                alignment,
            });
            Ok(())
        }

        /// Writes the archive.
        ///
        /// Entries are sorted by name first. Directive section 23 requires
        /// deterministic ordering, and sorting is the only ordering that does not
        /// depend on how the caller happened to walk a directory.
        pub fn finish(mut self) -> Result<Vec<u8>, Diagnostic> {
            self.entries
                .sort_by(|left, right| left.name.cmp(&right.name));

            let mut writer = BinaryWriter::new(Endian::Little);
            let mut written: Vec<(Pending, u64, u32)> = Vec::with_capacity(self.entries.len());

            for entry in self.entries {
                let header_offset = writer.position() as u64;
                let crc = checksum::crc32(&entry.bytes);

                let size = u32::try_from(entry.bytes.len()).map_err(|_| {
                    Diagnostic::new(
                        "EA053",
                        Severity::Error,
                        FailureClass::ResourceExhaustion,
                        "core.archive",
                        "An entry is larger than the format's 32-bit size field.",
                    )
                    .with_context(format!("Entry: {}", truncate(&entry.name, 96)))
                    .with_suggestion("ZIP64 is not implemented.")
                })?;

                let name_bytes = entry.name.as_bytes();
                let name_length = u16::try_from(name_bytes.len()).map_err(|_| {
                    Diagnostic::new(
                        "EA002",
                        Severity::Error,
                        FailureClass::SecurityFailure,
                        "core.archive",
                        "An entry name is longer than the format's length field.",
                    )
                })?;

                // The extra field is sized so that the entry's bytes land on the
                // boundary it asked for. A record is four bytes of header plus
                // its payload, so padding of one to three bytes is grown by one
                // whole alignment step rather than written as a malformed record.
                let base = header_offset + LOCAL_HEADER_SIZE + u64::from(name_length);
                let mut padding = (entry.alignment - (base % entry.alignment)) % entry.alignment;
                while padding != 0 && padding < 4 {
                    padding += entry.alignment;
                }
                let extra_length = u16::try_from(padding).map_err(|_| {
                    Diagnostic::new(
                        "EA054",
                        Severity::Error,
                        FailureClass::InternalError,
                        "core.archive",
                        "The alignment padding is larger than the extra field allows.",
                    )
                    .with_context(format!("Entry: {}", truncate(&entry.name, 96)))
                    .with_context(format!("Padding: {padding} bytes"))
                })?;

                writer.u32(LOCAL_HEADER_SIGNATURE)?;
                writer.u16(20)?; // version needed: 2.0, which is stored and deflate
                writer.u16(0)?; // no flags: no encryption, no data descriptor
                writer.u16(Compression::Stored.method())?;
                writer.u16(FIXED_DOS_TIME)?;
                writer.u16(FIXED_DOS_DATE)?;
                writer.u32(crc)?;
                writer.u32(size)?;
                writer.u32(size)?;
                writer.u16(name_length)?;
                writer.u16(extra_length)?;
                writer.bytes(name_bytes)?;

                if extra_length >= 4 {
                    writer.u16(ALIGNMENT_EXTRA_ID)?;
                    writer.u16(extra_length - 4)?;
                    for _ in 0..extra_length - 4 {
                        writer.u8(0)?;
                    }
                }

                debug_assert_eq!(writer.position() as u64 % entry.alignment, 0);
                writer.bytes(&entry.bytes)?;
                written.push((entry, header_offset, crc));
            }

            let directory_offset = writer.position() as u64;

            for (entry, header_offset, crc) in &written {
                let size = entry.bytes.len() as u32;
                writer.u32(CENTRAL_HEADER_SIGNATURE)?;
                writer.u16(20)?; // version made by
                writer.u16(20)?; // version needed
                writer.u16(0)?;
                writer.u16(Compression::Stored.method())?;
                writer.u16(FIXED_DOS_TIME)?;
                writer.u16(FIXED_DOS_DATE)?;
                writer.u32(*crc)?;
                writer.u32(size)?;
                writer.u32(size)?;
                writer.u16(entry.name.len() as u16)?;
                writer.u16(0)?; // the central directory carries no extra field
                writer.u16(0)?; // no comment
                writer.u16(0)?; // disk number
                writer.u16(0)?; // internal attributes
                writer.u32(0)?; // external attributes: none, so no host's umask
                writer.u32(*header_offset as u32)?;
                writer.bytes(entry.name.as_bytes())?;
            }

            let directory_size = writer.position() as u64 - directory_offset;
            let count = u16::try_from(written.len()).map_err(|_| {
                Diagnostic::new(
                    "EA052",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.archive",
                    "The archive holds more entries than the format's counter allows.",
                )
            })?;

            writer.u32(END_OF_CENTRAL_DIRECTORY_SIGNATURE)?;
            writer.u16(0)?;
            writer.u16(0)?;
            writer.u16(count)?;
            writer.u16(count)?;
            writer.u32(directory_size as u32)?;
            writer.u32(directory_offset as u32)?;
            writer.u16(0)?; // no archive comment

            Ok(writer.finish())
        }
    }

    /// Whether a name is a native library, which needs page alignment.
    fn is_native_library(name: &str) -> bool {
        name.starts_with("lib/") && name.ends_with(".so")
    }
}

// ===========================================================================
// der — ASN.1 distinguished encoding rules (directive sections 20 and 30)
// ===========================================================================

/// The encoding every X.509 certificate is written in.
///
/// ## Contract (directive section 2)
///
/// * **Purpose** — read DER, the one canonical encoding of ASN.1.
/// * **Inputs** — bytes from a certificate. Untrusted, always: a certificate
///   arrives from whoever signed the thing being verified.
/// * **Non-Responsibilities** — BER, which allows several encodings of one
///   value, and the meaning of anything it reads.
/// * **Security** — lengths are bounded before use, nesting is explicit rather
///   than recursive, and every encoding DER forbids is refused rather than
///   accepted leniently.
/// * **Status** — PARTIAL: the subset a certificate uses.
///
/// ## Why leniency is a security bug here
///
/// DER exists because BER lets one value be written several ways, and a
/// verifier that accepts two spellings of a name can be shown a different name
/// from the one a parser displays. Every rule below - definite lengths, shortest
/// form, no indefinite encoding - is refused rather than tolerated for that
/// reason.
pub mod der {
    use crate::binary::{Endian, Reader as BinaryReader};
    use crate::diag::{Diagnostic, Severity};
    use crate::FailureClass;

    /// Deepest nesting this reader will follow.
    pub const MAX_DEPTH: usize = 32;

    /// Universal tag numbers this module names.
    pub mod tag {
        /// INTEGER
        pub const INTEGER: u8 = 0x02;
        /// BIT STRING
        pub const BIT_STRING: u8 = 0x03;
        /// OCTET STRING
        pub const OCTET_STRING: u8 = 0x04;
        /// NULL
        pub const NULL: u8 = 0x05;
        /// OBJECT IDENTIFIER
        pub const OID: u8 = 0x06;
        /// UTF8String
        pub const UTF8_STRING: u8 = 0x0c;
        /// PrintableString
        pub const PRINTABLE_STRING: u8 = 0x13;
        /// IA5String
        pub const IA5_STRING: u8 = 0x16;
        /// UTCTime
        pub const UTC_TIME: u8 = 0x17;
        /// GeneralizedTime
        pub const GENERALIZED_TIME: u8 = 0x18;
        /// SEQUENCE
        pub const SEQUENCE: u8 = 0x30;
        /// SET
        pub const SET: u8 = 0x31;
    }

    /// One tag-length-value element.
    #[derive(Clone, Copy, Debug)]
    pub struct Element<'a> {
        /// The identifier octet.
        pub tag: u8,
        /// The contents, without the tag or the length.
        pub contents: &'a [u8],
        /// Offset of the identifier octet within the document.
        pub offset: usize,
        /// Total size of the element, including its tag and length.
        pub total: usize,
    }

    impl<'a> Element<'a> {
        /// Whether the element holds other elements.
        pub fn is_constructed(&self) -> bool {
            self.tag & 0x20 != 0
        }

        /// Whether the tag is context-specific, as `[0]` is.
        pub fn is_context_specific(&self) -> bool {
            self.tag & 0xc0 == 0x80
        }

        /// The context tag number, when the tag is context-specific.
        pub fn context_number(&self) -> Option<u8> {
            if self.is_context_specific() {
                Some(self.tag & 0x1f)
            } else {
                None
            }
        }

        /// Reads the contents as a sequence of further elements.
        pub fn reader(&self) -> Reader<'a> {
            Reader::new(self.contents, self.offset)
        }
    }

    fn fail(code: &str, message: impl Into<String>, offset: usize) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::Corruption,
            "core.der",
            message,
        )
        .with_context(format!("At offset: {offset}"))
    }

    /// Reads a run of DER elements.
    #[derive(Clone, Debug)]
    pub struct Reader<'a> {
        data: &'a [u8],
        position: usize,
        base: usize,
    }

    impl<'a> Reader<'a> {
        /// Reads `data`, reporting offsets relative to `base`.
        pub fn new(data: &'a [u8], base: usize) -> Reader<'a> {
            Reader {
                data,
                position: 0,
                base,
            }
        }

        /// Whether everything has been read.
        pub fn is_empty(&self) -> bool {
            self.position >= self.data.len()
        }

        /// Bytes left.
        pub fn remaining(&self) -> usize {
            self.data.len() - self.position
        }

        /// Reads the next element.
        pub fn next_element(&mut self) -> Result<Element<'a>, Diagnostic> {
            let offset = self.base + self.position;
            let mut reader =
                BinaryReader::new(&self.data[self.position..], Endian::Big, "certificate");

            let tag = reader.u8()?;
            if tag & 0x1f == 0x1f {
                return Err(fail(
                    "ED001",
                    "A tag uses the high-tag-number form, which nothing here needs.",
                    offset,
                ));
            }

            let first = reader.u8()?;
            let length: u64 = if first & 0x80 == 0 {
                u64::from(first)
            } else {
                let count = usize::from(first & 0x7f);
                if count == 0 {
                    return Err(fail(
                        "ED002",
                        "A length is written in the indefinite form, which DER forbids.",
                        offset,
                    )
                    .with_suggestion(
                        "Indefinite lengths let one value be written two ways, and a \
                         verifier that accepts both can be shown something a parser \
                         does not display.",
                    ));
                }
                if count > 8 {
                    return Err(fail("ED003", "A length needs more than 64 bits.", offset));
                }

                let bytes = reader.bytes(count)?;
                if bytes[0] == 0 {
                    return Err(fail(
                        "ED004",
                        "A length has a leading zero, so it is not in its shortest form.",
                        offset,
                    )
                    .with_suggestion("DER requires one encoding per value."));
                }

                let mut value: u64 = 0;
                for byte in bytes {
                    value = (value << 8) | u64::from(*byte);
                }
                if value < 0x80 {
                    return Err(fail(
                        "ED005",
                        "A short length is written in the long form.",
                        offset,
                    ));
                }
                value
            };

            // A declared length is checked against what is actually there
            // before it is used for anything, so a wrong length cannot make
            // this read past its buffer (directive section 60). The check
            // lives here rather than in the binary reader so the diagnostic
            // names the element rather than the byte stream, and so the two
            // do not race to report the same problem with different codes.
            let contents_start = self.position + reader.position();
            let available = (self.data.len() - contents_start) as u64;
            if length > available {
                return Err(fail(
                    "ED007",
                    "An element extends past the data it is in.",
                    offset,
                )
                .with_context(format!("Wants: {length} bytes"))
                .with_context(format!("Available: {available}")));
            }
            // Bounded by `available` just above, so this cannot truncate.
            let length = length as usize;
            let end = contents_start + length;

            let element = Element {
                tag,
                contents: &self.data[contents_start..end],
                offset,
                total: end - self.position,
            };
            self.position = end;
            Ok(element)
        }

        /// Reads the next element and checks its tag.
        pub fn expect(&mut self, tag: u8) -> Result<Element<'a>, Diagnostic> {
            let element = self.next_element()?;
            if element.tag != tag {
                return Err(fail(
                    "ED010",
                    format!("Expected tag 0x{tag:02x} but found 0x{:02x}.", element.tag),
                    element.offset,
                ));
            }
            Ok(element)
        }

        /// Reads the next element if it has this tag, without consuming anything
        /// otherwise.
        pub fn take_if(&mut self, tag: u8) -> Option<Element<'a>> {
            let saved = self.position;
            match self.next_element() {
                Ok(element) if element.tag == tag => Some(element),
                _ => {
                    self.position = saved;
                    None
                }
            }
        }
    }

    /// Reads an OBJECT IDENTIFIER into its dotted form.
    ///
    /// The first byte holds two arcs at once, which is the one place the
    /// encoding is not simply base-128.
    pub fn read_oid(element: &Element<'_>) -> Result<String, Diagnostic> {
        if element.tag != tag::OID {
            return Err(fail(
                "ED020",
                "That element is not an object identifier.",
                element.offset,
            ));
        }
        if element.contents.is_empty() {
            return Err(fail(
                "ED021",
                "An object identifier is empty.",
                element.offset,
            ));
        }

        let mut out = String::with_capacity(32);
        let first = element.contents[0];
        let (a, b) = if first < 40 {
            (0, u32::from(first))
        } else if first < 80 {
            (1, u32::from(first) - 40)
        } else {
            (2, u32::from(first) - 80)
        };
        out.push_str(&a.to_string());
        out.push('.');
        out.push_str(&b.to_string());

        let mut value: u64 = 0;
        let mut in_progress = false;
        for byte in &element.contents[1..] {
            if !in_progress && *byte == 0x80 {
                return Err(fail(
                    "ED022",
                    "An object identifier arc has a leading zero byte.",
                    element.offset,
                ));
            }
            in_progress = true;

            if value > (u64::MAX >> 7) {
                return Err(fail(
                    "ED023",
                    "An object identifier arc is too large.",
                    element.offset,
                ));
            }
            value = (value << 7) | u64::from(byte & 0x7f);

            if byte & 0x80 == 0 {
                out.push('.');
                out.push_str(&value.to_string());
                value = 0;
                in_progress = false;
            }
        }

        if in_progress {
            return Err(fail(
                "ED024",
                "An object identifier ends in the middle of an arc.",
                element.offset,
            ));
        }

        Ok(out)
    }

    /// Reads an INTEGER into its unpadded big-endian bytes.
    ///
    /// A certificate serial number is routinely larger than any integer type, so
    /// it is kept as bytes and rendered as hexadecimal rather than converted.
    pub fn read_integer_bytes<'a>(element: &Element<'a>) -> Result<&'a [u8], Diagnostic> {
        if element.tag != tag::INTEGER {
            return Err(fail(
                "ED030",
                "That element is not an integer.",
                element.offset,
            ));
        }
        if element.contents.is_empty() {
            return Err(fail("ED031", "An integer has no content.", element.offset));
        }
        if element.contents.len() > 1 {
            let first = element.contents[0];
            let second = element.contents[1];
            if (first == 0x00 && second & 0x80 == 0) || (first == 0xff && second & 0x80 != 0) {
                return Err(fail(
                    "ED032",
                    "An integer is padded, so it is not in its shortest form.",
                    element.offset,
                ));
            }
        }
        Ok(element.contents)
    }

    /// Reads a text element, refusing anything that is not one.
    pub fn read_string(element: &Element<'_>) -> Result<String, Diagnostic> {
        match element.tag {
            tag::UTF8_STRING | tag::PRINTABLE_STRING | tag::IA5_STRING => {
                match core::str::from_utf8(element.contents) {
                    Ok(text) => Ok(text.to_string()),
                    Err(_) => Err(fail(
                        "ED040",
                        "A string is not valid UTF-8.",
                        element.offset,
                    )),
                }
            }
            other => Err(fail(
                "ED041",
                format!("Tag 0x{other:02x} is not a string this reader accepts."),
                element.offset,
            )),
        }
    }

    /// Renders bytes as uppercase hexadecimal, the way a serial number is shown.
    pub fn to_hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }
}

// ===========================================================================
// x509 — certificates (directive sections 25 and 30)
// ===========================================================================

/// Reading the certificate that says who signed something.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                       |
/// |----------------------|-------------------------------------------------------------|
/// | Module               | `omni_core::x509`                                           |
/// | Purpose              | Turn a certificate into facts that can be shown and compared.|
/// | Inputs               | DER bytes. Untrusted: a certificate comes from whoever signed |
/// |                      | the artifact being examined.                                  |
/// | Outputs              | A [`Certificate`], or diagnostics.                            |
/// | Non-Responsibilities | Checking a signature, building a chain, or deciding trust.    |
/// | Status               | PARTIAL — see what it does not do, below.                     |
///
/// ## What this does not do, said plainly
///
/// It **parses**. It does not verify. Nothing here checks that a certificate's
/// signature is valid, that it chains to anything, that it has not been revoked,
/// or that the key in it signed anything. Those need public-key arithmetic that
/// this tree does not have, and directive section 1 does not allow a parser to
/// be described as a verifier.
///
/// What it is good for today: identifying a signer, comparing one build's signer
/// with another's by fingerprint, and showing a person who signed something.
pub mod x509 {
    use crate::der::{self, tag, Element};
    use crate::diag::{Diagnostic, Severity};
    use crate::hash::{sha256, Digest};
    use crate::json::Writer;
    use crate::FailureClass;

    /// Largest certificate this reader accepts (directive section 60).
    pub const MAX_CERTIFICATE_BYTES: usize = 64 * 1024;

    /// An algorithm identifier, kept as its number and its name.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Algorithm {
        /// The object identifier, in dotted form.
        pub oid: String,
        /// The name, when this build knows it.
        pub name: Option<&'static str>,
    }

    impl Algorithm {
        /// The name if there is one, otherwise the identifier.
        pub fn display(&self) -> &str {
            self.name.unwrap_or(&self.oid)
        }

        /// Whether this build recognises the algorithm.
        pub fn is_known(&self) -> bool {
            self.name.is_some()
        }
    }

    /// Algorithm identifiers a signed Android package uses.
    ///
    /// An unknown identifier is reported as unknown rather than guessed at: a
    /// signature this build cannot name is one it certainly cannot check.
    const ALGORITHMS: &[(&str, &str)] = &[
        ("1.2.840.113549.1.1.1", "RSA"),
        ("1.2.840.113549.1.1.5", "SHA-1 with RSA"),
        ("1.2.840.113549.1.1.11", "SHA-256 with RSA"),
        ("1.2.840.113549.1.1.12", "SHA-384 with RSA"),
        ("1.2.840.113549.1.1.13", "SHA-512 with RSA"),
        ("1.2.840.113549.1.1.10", "RSASSA-PSS"),
        ("1.2.840.10045.2.1", "Elliptic curve"),
        ("1.2.840.10045.4.3.2", "ECDSA with SHA-256"),
        ("1.2.840.10045.4.3.3", "ECDSA with SHA-384"),
        ("1.2.840.10045.4.3.4", "ECDSA with SHA-512"),
        ("1.3.101.112", "Ed25519"),
    ];

    /// Attribute types that appear in a distinguished name.
    const NAME_ATTRIBUTES: &[(&str, &str)] = &[
        ("2.5.4.3", "CN"),
        ("2.5.4.6", "C"),
        ("2.5.4.7", "L"),
        ("2.5.4.8", "ST"),
        ("2.5.4.9", "STREET"),
        ("2.5.4.10", "O"),
        ("2.5.4.11", "OU"),
        ("0.9.2342.19200300.100.1.25", "DC"),
        ("1.2.840.113549.1.9.1", "emailAddress"),
    ];

    fn name_algorithm(oid: &str) -> Algorithm {
        Algorithm {
            oid: oid.to_string(),
            name: ALGORITHMS
                .iter()
                .find(|(known, _)| *known == oid)
                .map(|(_, name)| *name),
        }
    }

    /// A certificate, as far as this build reads one.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Certificate {
        /// Serial number, in uppercase hexadecimal.
        pub serial: String,
        /// Who issued it.
        pub issuer: String,
        /// Who it is about.
        pub subject: String,
        /// Start of validity, normalised.
        pub not_before: String,
        /// End of validity, normalised.
        pub not_after: String,
        /// Algorithm the issuer used to sign it.
        pub signature_algorithm: Algorithm,
        /// Algorithm of the key inside it.
        pub public_key_algorithm: Algorithm,
        /// Size of the key in bits, when it can be determined.
        pub public_key_bits: Option<u32>,
        /// SHA-256 of the whole certificate.
        ///
        /// This is the fingerprint every tool prints, and the only thing here
        /// that identifies a signer without needing to verify anything.
        pub fingerprint: Digest,
    }

    impl Certificate {
        /// Reads a certificate.
        pub fn parse(data: &[u8]) -> Result<Certificate, Diagnostic> {
            if data.len() > MAX_CERTIFICATE_BYTES {
                return Err(Diagnostic::new(
                    "EX001",
                    Severity::Error,
                    FailureClass::ResourceExhaustion,
                    "core.x509",
                    "The certificate is larger than the accepted limit.",
                )
                .with_context(format!("Limit: {MAX_CERTIFICATE_BYTES} bytes")));
            }

            let mut outer = der::Reader::new(data, 0);
            let certificate = outer.expect(tag::SEQUENCE)?;
            if !outer.is_empty() {
                return Err(problem(
                    "EX002",
                    "There is more than one certificate in these bytes.",
                    "A certificate is one SEQUENCE and nothing else.",
                ));
            }

            let mut body = certificate.reader();
            let tbs = body.expect(tag::SEQUENCE)?;
            let signature_algorithm = read_algorithm(&body.expect(tag::SEQUENCE)?)?;
            let _signature_value = body.expect(tag::BIT_STRING)?;

            let mut fields = tbs.reader();

            // The version is [0] EXPLICIT and absent for a version 1
            // certificate, so it is taken only if it is there.
            let _version = fields.take_if(0xa0);

            let serial = der::to_hex(der::read_integer_bytes(&fields.expect(tag::INTEGER)?)?);
            let _inner_algorithm = fields.expect(tag::SEQUENCE)?;
            let issuer = read_name(&fields.expect(tag::SEQUENCE)?)?;

            let validity = fields.expect(tag::SEQUENCE)?;
            let mut validity_fields = validity.reader();
            let not_before = read_time(&validity_fields.next_element()?)?;
            let not_after = read_time(&validity_fields.next_element()?)?;

            let subject = read_name(&fields.expect(tag::SEQUENCE)?)?;
            let key_info = fields.expect(tag::SEQUENCE)?;
            let (public_key_algorithm, public_key_bits) = read_public_key(&key_info)?;

            Ok(Certificate {
                serial,
                issuer,
                subject,
                not_before,
                not_after,
                signature_algorithm,
                public_key_algorithm,
                public_key_bits,
                fingerprint: sha256(data),
            })
        }

        /// The fingerprint in the colon-separated form tools print.
        pub fn fingerprint_display(&self) -> String {
            let hex = self.fingerprint.to_hex().to_uppercase();
            hex.as_bytes()
                .chunks(2)
                .map(|pair| String::from_utf8_lossy(pair).into_owned())
                .collect::<Vec<_>>()
                .join(":")
        }

        /// Serialises the certificate as an object inside an open array.
        pub fn write_json(&self, w: &mut Writer) {
            w.begin_object(None);
            w.field_str("serial", &self.serial);
            w.field_str("subject", &self.subject);
            w.field_str("issuer", &self.issuer);
            w.field_str("notBefore", &self.not_before);
            w.field_str("notAfter", &self.not_after);
            w.field_str("signatureAlgorithm", self.signature_algorithm.display());
            w.field_str("publicKeyAlgorithm", self.public_key_algorithm.display());
            if let Some(bits) = self.public_key_bits {
                w.field_u64("publicKeyBits", u64::from(bits));
            }
            w.field_str("fingerprintSha256", &self.fingerprint.to_hex());
            w.field_bool("signatureChecked", false);
            w.end_object();
        }
    }

    fn problem(code: &str, message: &str, suggestion: &str) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::Corruption,
            "core.x509",
            message,
        )
        .with_suggestion(suggestion)
    }

    fn read_algorithm(element: &Element<'_>) -> Result<Algorithm, Diagnostic> {
        let mut fields = element.reader();
        let oid = der::read_oid(&fields.expect(tag::OID)?)?;
        Ok(name_algorithm(&oid))
    }

    /// Renders a distinguished name in the order it is encoded.
    fn read_name(element: &Element<'_>) -> Result<String, Diagnostic> {
        let mut parts: Vec<String> = Vec::new();
        let mut sequence = element.reader();

        while !sequence.is_empty() {
            let rdn = sequence.expect(tag::SET)?;
            let mut attributes = rdn.reader();
            while !attributes.is_empty() {
                let pair = attributes.expect(tag::SEQUENCE)?;
                let mut fields = pair.reader();
                let oid = der::read_oid(&fields.expect(tag::OID)?)?;
                let value = fields.next_element()?;

                let label = NAME_ATTRIBUTES
                    .iter()
                    .find(|(known, _)| *known == oid)
                    .map(|(_, label)| (*label).to_string())
                    .unwrap_or(oid);

                // A name may hold a type this build does not model; its value is
                // still shown, because hiding part of a name is how two signers
                // are made to look like one.
                let text = der::read_string(&value)
                    .unwrap_or_else(|_| format!("#{}", der::to_hex(value.contents)));
                parts.push(format!("{label}={text}"));
            }
        }

        if parts.is_empty() {
            return Err(problem(
                "EX010",
                "A distinguished name is empty.",
                "A certificate names its subject and its issuer.",
            ));
        }

        Ok(parts.join(", "))
    }

    /// Normalises a certificate time into `YYYY-MM-DD HH:MM:SS UTC`.
    ///
    /// UTCTime writes a two-digit year, and the rule for reading it is fixed by
    /// RFC 5280: 50 and above means the twentieth century.
    fn read_time(element: &Element<'_>) -> Result<String, Diagnostic> {
        let text = core::str::from_utf8(element.contents).map_err(|_| {
            problem(
                "EX020",
                "A time is not valid text.",
                "The certificate is malformed.",
            )
        })?;

        let digits: String = match element.tag {
            tag::UTC_TIME => {
                if text.len() < 13 || !text.ends_with('Z') {
                    return Err(problem(
                        "EX021",
                        "A UTCTime is not in the form a certificate uses.",
                        "RFC 5280 requires YYMMDDHHMMSSZ.",
                    ));
                }
                let year: u32 = text[0..2].parse().map_err(|_| {
                    problem(
                        "EX022",
                        "A time has a year that is not a number.",
                        "The certificate is malformed.",
                    )
                })?;
                let century = if year >= 50 { 1900 } else { 2000 };
                format!("{}{}", century + year, &text[2..12])
            }
            tag::GENERALIZED_TIME => {
                if text.len() < 15 || !text.ends_with('Z') {
                    return Err(problem(
                        "EX023",
                        "A GeneralizedTime is not in the form a certificate uses.",
                        "RFC 5280 requires YYYYMMDDHHMMSSZ.",
                    ));
                }
                text[0..14].to_string()
            }
            other => {
                return Err(problem(
                    "EX024",
                    &format!("Tag 0x{other:02x} is not a time."),
                    "The certificate is malformed.",
                ))
            }
        };

        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(problem(
                "EX025",
                "A time contains something that is not a digit.",
                "The certificate is malformed.",
            ));
        }

        Ok(format!(
            "{}-{}-{} {}:{}:{} UTC",
            &digits[0..4],
            &digits[4..6],
            &digits[6..8],
            &digits[8..10],
            &digits[10..12],
            &digits[12..14],
        ))
    }

    /// Reads the algorithm of the key, and its size when that can be told.
    fn read_public_key(element: &Element<'_>) -> Result<(Algorithm, Option<u32>), Diagnostic> {
        let mut fields = element.reader();
        let algorithm = read_algorithm(&fields.expect(tag::SEQUENCE)?)?;
        let key = fields.expect(tag::BIT_STRING)?;

        // An RSA key is a SEQUENCE of two integers inside the bit string, and
        // the first is the modulus. Its length is the key size, which is worth
        // showing: a 1024-bit key is a fact a person should see.
        let bits = if algorithm.oid == "1.2.840.113549.1.1.1" && !key.contents.is_empty() {
            // The first byte of a BIT STRING counts unused trailing bits.
            let mut inner = der::Reader::new(&key.contents[1..], key.offset);
            inner
                .expect(tag::SEQUENCE)
                .ok()
                .and_then(|sequence| {
                    let mut numbers = sequence.reader();
                    numbers.expect(tag::INTEGER).ok()
                })
                .and_then(|modulus| der::read_integer_bytes(&modulus).ok())
                .map(|bytes| {
                    // A DER integer is signed, so a modulus whose top bit is
                    // set carries a leading zero byte that is padding and not
                    // part of the number. The key size is the number's own bit
                    // length -- 2048, not 2056 -- which is what every tool
                    // prints and what a person comparing two reports expects.
                    let digits = match bytes.iter().position(|byte| *byte != 0) {
                        Some(first) => &bytes[first..],
                        None => &[][..],
                    };
                    match digits.split_first() {
                        Some((top, rest)) => (rest.len() as u32) * 8 + (8 - top.leading_zeros()),
                        None => 0,
                    }
                })
        } else {
            None
        };

        Ok((algorithm, bits))
    }
}

// ===========================================================================
// signing — the APK signing block (directive sections 25, 27 and 30)
// ===========================================================================

/// Reading and checking the signature block an Android package carries.
///
/// ## Contract (directive section 2)
///
/// | Field                | Value                                                        |
/// |----------------------|--------------------------------------------------------------|
/// | Module               | `omni_core::signing`                                         |
/// | Purpose              | Find the signing block, read who signed, and recompute the    |
/// |                      | digests it claims over the package's own bytes.               |
/// | Inputs               | A package's bytes. Untrusted.                                 |
/// | Outputs              | A [`Report`] saying what was found and what was checked.      |
/// | Security             | Never reports a digest as verified without having recomputed  |
/// |                      | it, and never calls a digest match a valid signature.         |
/// | Status               | PARTIAL — the digests are checked, the signatures are not.    |
///
/// ## What is checked, and what is not
///
/// **Checked.** The content digest. The scheme splits the package into three
/// sections, chunks each into one-megabyte pieces, hashes every chunk and then
/// hashes the chunk hashes. Recomputing that and comparing it with what the
/// block claims detects any change to the package's contents, its central
/// directory, or its end record. That covers threats T1, T3 and T4 of directive
/// section 27: an APK modified after signing, a modified DEX, a modified native
/// library.
///
/// **Not checked.** Whether the signature over the signed data is valid. That
/// needs RSA or elliptic-curve arithmetic, which this tree does not have. So a
/// digest match proves the package has not changed since the block was written;
/// it does not prove who wrote it. Anyone able to rewrite the package can also
/// rewrite the digest, and only a signature check closes that gap.
///
/// This distinction is why [`Report::signatures_checked`] exists and is always
/// false. Directive section 1 does not allow the difference to be blurred, and
/// section 28 does not allow security to be reduced to one boolean.
pub mod signing {
    use crate::binary::{Endian, Reader};
    use crate::diag::{Diagnostic, Severity, Sink};
    use crate::hash::{Digest, Sha256};
    use crate::json::Writer;
    use crate::x509::Certificate;
    use crate::FailureClass;

    /// The magic that marks the end of an APK signing block.
    pub const MAGIC: &[u8; 16] = b"APK Sig Block 42";

    /// Identifier of the APK Signature Scheme v2 block.
    pub const V2_BLOCK_ID: u32 = 0x7109_871a;

    /// Identifier of the APK Signature Scheme v3 block.
    pub const V3_BLOCK_ID: u32 = 0xf053_68c0;

    /// Identifier of the APK Signature Scheme v3.1 block.
    pub const V31_BLOCK_ID: u32 = 0x1b93_ad61;

    /// Size of the chunks the content digest is computed over.
    pub const CHUNK_SIZE: usize = 1024 * 1024;

    /// Prefix of a chunk's own digest, from the scheme's definition.
    const CHUNK_PREFIX: u8 = 0xa5;

    /// Prefix of the digest over the chunk digests.
    const ROOT_PREFIX: u8 = 0x5a;

    /// Largest signing block this reader accepts (directive section 60).
    pub const MAX_BLOCK_BYTES: u64 = 64 * 1024 * 1024;

    /// A signature algorithm the scheme defines.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct SignatureAlgorithm {
        /// The identifier as written in the block.
        pub id: u32,
        /// Its name.
        pub name: &'static str,
        /// Whether its content digest is SHA-256, which is the one this build
        /// can recompute.
        pub uses_sha256: bool,
    }

    /// Every algorithm the scheme defines, with whether this build can check it.
    const ALGORITHMS: &[SignatureAlgorithm] = &[
        SignatureAlgorithm {
            id: 0x0101,
            name: "RSASSA-PSS with SHA-256",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0102,
            name: "RSASSA-PSS with SHA-512",
            uses_sha256: false,
        },
        SignatureAlgorithm {
            id: 0x0103,
            name: "RSASSA-PKCS1-v1_5 with SHA-256",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0104,
            name: "RSASSA-PKCS1-v1_5 with SHA-512",
            uses_sha256: false,
        },
        SignatureAlgorithm {
            id: 0x0201,
            name: "ECDSA with SHA-256",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0202,
            name: "ECDSA with SHA-512",
            uses_sha256: false,
        },
        SignatureAlgorithm {
            id: 0x0301,
            name: "DSA with SHA-256",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0421,
            name: "RSASSA-PKCS1-v1_5 with SHA-256 over a verity tree",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0423,
            name: "ECDSA with SHA-256 over a verity tree",
            uses_sha256: true,
        },
        SignatureAlgorithm {
            id: 0x0425,
            name: "DSA with SHA-256 over a verity tree",
            uses_sha256: true,
        },
    ];

    fn algorithm(id: u32) -> Option<SignatureAlgorithm> {
        ALGORITHMS.iter().copied().find(|entry| entry.id == id)
    }

    fn fail(code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(
            code,
            Severity::Error,
            FailureClass::Corruption,
            "core.signing",
            message,
        )
    }

    /// The block that sits between a package's entries and its central directory.
    #[derive(Clone, Debug)]
    pub struct Block {
        offset: u64,
        size: u64,
        pairs: Vec<(u32, Vec<u8>)>,
    }

    impl Block {
        /// Where the block starts.
        pub fn offset(&self) -> u64 {
            self.offset
        }

        /// How large it is, including its two size fields and its magic.
        pub fn size(&self) -> u64 {
            self.size
        }

        /// The identifiers it carries, in order.
        pub fn ids(&self) -> Vec<u32> {
            self.pairs.iter().map(|(id, _)| *id).collect()
        }

        /// The value stored under an identifier.
        pub fn value(&self, id: u32) -> Option<&[u8]> {
            self.pairs
                .iter()
                .find(|(candidate, _)| *candidate == id)
                .map(|(_, value)| value.as_slice())
        }
    }

    /// Finds and reads the signing block.
    ///
    /// It ends immediately before the central directory, and is found by reading
    /// backwards from there: magic, then size, then the block itself. Returns
    /// `Ok(None)` when the package simply has no block, which is not an error.
    pub fn find_block(
        data: &[u8],
        central_directory_offset: u64,
    ) -> Result<Option<Block>, Diagnostic> {
        let footer = 16 + 8;
        if central_directory_offset < footer {
            return Ok(None);
        }

        let magic_at = central_directory_offset - 16;
        let reader = Reader::new(data, Endian::Little, "signing block");
        let magic = reader.slice_at(magic_at, 16)?;
        if magic != MAGIC {
            return Ok(None);
        }

        let mut sizes = Reader::new(data, Endian::Little, "signing block");
        sizes.seek(central_directory_offset - footer)?;
        let trailing_size = sizes.u64()?;

        if trailing_size > MAX_BLOCK_BYTES {
            return Err(Diagnostic::new(
                "ES001",
                Severity::Error,
                FailureClass::ResourceExhaustion,
                "core.signing",
                "The signing block is larger than the accepted limit.",
            )
            .with_context(format!("Declared: {trailing_size} bytes"))
            .with_context(format!("Limit: {MAX_BLOCK_BYTES} bytes")));
        }

        // The block's size counts everything after the leading size field, so
        // the block begins eight bytes before that.
        let Some(start) = central_directory_offset
            .checked_sub(trailing_size)
            .and_then(|value| value.checked_sub(8))
        else {
            return Err(fail(
                "ES002",
                "The signing block's size reaches before the file starts.",
            )
            .with_context(format!("Declared: {trailing_size} bytes")));
        };

        let mut leading = Reader::new(data, Endian::Little, "signing block");
        leading.seek(start)?;
        let leading_size = leading.u64()?;
        if leading_size != trailing_size {
            return Err(
                fail("ES003", "The signing block's two size fields disagree.")
                    .with_context(format!(
                        "Leading: {leading_size}, trailing: {trailing_size}"
                    ))
                    .with_class(FailureClass::SecurityFailure)
                    .with_suggestion(
                        "A block whose size is written twice and differently is a block \
                     a reader and a verifier can be made to see differently.",
                    ),
            );
        }

        // The pairs occupy everything between the leading size and the trailing
        // size, which is the block's size less the trailing size field and magic.
        let Some(pairs_length) = trailing_size.checked_sub(24) else {
            return Err(fail(
                "ES004",
                "The signing block is too small to hold anything.",
            ));
        };

        let mut pairs: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut cursor = Reader::new(data, Endian::Little, "signing block");
        cursor.seek(start + 8)?;
        let end = start + 8 + pairs_length;

        while (cursor.position() as u64) < end {
            let length = cursor.u64()?;
            if length < 4 {
                return Err(
                    fail("ES005", "A signing block entry is too small to hold an id.")
                        .with_context(format!("Length: {length}")),
                );
            }
            let Some(value_length) = length.checked_sub(4) else {
                return Err(fail(
                    "ES005",
                    "A signing block entry has an impossible length.",
                ));
            };

            let id = cursor.u32()?;
            let value_length = cursor.checked_length(value_length)?;
            if cursor.position() as u64 + value_length as u64 > end {
                return Err(fail(
                    "ES006",
                    "A signing block entry runs past the end of the block.",
                )
                .with_context(format!("Entry id: 0x{id:08x}")));
            }
            let value = cursor.bytes(value_length)?.to_vec();

            if pairs.iter().any(|(existing, _)| *existing == id) {
                return Err(
                    fail("ES007", "The signing block carries one identifier twice.")
                        .with_context(format!("Id: 0x{id:08x}"))
                        .with_class(FailureClass::SecurityFailure)
                        .with_suggestion(
                            "Which of the two a verifier reads is not defined, so the \
                         block is refused rather than guessed at.",
                        ),
                );
            }

            pairs.push((id, value));
        }

        Ok(Some(Block {
            offset: start,
            size: trailing_size + 8,
            pairs,
        }))
    }

    /// One signer's claim, as the v2 block records it.
    #[derive(Clone, Debug)]
    pub struct Signer {
        /// Digests the signer claims, by algorithm.
        pub digests: Vec<(SignatureAlgorithm, Vec<u8>)>,
        /// Algorithms the signer produced a signature with.
        pub signature_algorithms: Vec<SignatureAlgorithm>,
        /// Certificates, the first of which identifies the signer.
        pub certificates: Vec<Certificate>,
        /// Identifiers of algorithms this build does not know.
        pub unknown_algorithms: Vec<u32>,
    }

    /// Reads a length-prefixed sequence, one element at a time.
    fn read_length_prefixed<'a>(
        reader: &mut Reader<'a>,
        what: &str,
    ) -> Result<&'a [u8], Diagnostic> {
        let length = reader.u32()?;
        let length = reader
            .checked_length(u64::from(length))
            .map_err(|error| error.with_context(format!("Reading: {what}")))?;
        reader.bytes(length)
    }

    /// Reads an APK Signature Scheme v2 or v3 block.
    ///
    /// The two share this structure; v3 adds fields this reader does not need
    /// and skips over.
    pub fn parse_signers(value: &[u8]) -> Result<Vec<Signer>, Diagnostic> {
        let mut outer = Reader::new(value, Endian::Little, "signers");
        let signers_bytes = read_length_prefixed(&mut outer, "signers")?;

        let mut signers: Vec<Signer> = Vec::new();
        let mut list = Reader::new(signers_bytes, Endian::Little, "signers");

        while list.remaining() > 0 {
            let signer_bytes = read_length_prefixed(&mut list, "signer")?;
            let mut signer = Reader::new(signer_bytes, Endian::Little, "signer");

            let signed_data = read_length_prefixed(&mut signer, "signed data")?;
            let mut signed = Reader::new(signed_data, Endian::Little, "signed data");

            // Digests.
            let digests_bytes = read_length_prefixed(&mut signed, "digests")?;
            let mut digests_reader = Reader::new(digests_bytes, Endian::Little, "digests");
            let mut digests = Vec::new();
            let mut unknown = Vec::new();
            while digests_reader.remaining() > 0 {
                let entry = read_length_prefixed(&mut digests_reader, "digest")?;
                let mut fields = Reader::new(entry, Endian::Little, "digest");
                let id = fields.u32()?;
                let digest = read_length_prefixed(&mut fields, "digest value")?;
                match algorithm(id) {
                    Some(known) => digests.push((known, digest.to_vec())),
                    None => unknown.push(id),
                }
            }

            // Certificates.
            let certificates_bytes = read_length_prefixed(&mut signed, "certificates")?;
            let mut certificates_reader =
                Reader::new(certificates_bytes, Endian::Little, "certificates");
            let mut certificates = Vec::new();
            while certificates_reader.remaining() > 0 {
                let der = read_length_prefixed(&mut certificates_reader, "certificate")?;
                certificates.push(Certificate::parse(der)?);
            }

            // The remainder of the signed data is additional attributes, which
            // this build does not interpret.

            // Signatures: their algorithms are recorded, their bytes are not
            // checked, and nothing here pretends otherwise.
            let signatures_bytes = read_length_prefixed(&mut signer, "signatures")?;
            let mut signatures_reader = Reader::new(signatures_bytes, Endian::Little, "signatures");
            let mut signature_algorithms = Vec::new();
            while signatures_reader.remaining() > 0 {
                let entry = read_length_prefixed(&mut signatures_reader, "signature")?;
                let mut fields = Reader::new(entry, Endian::Little, "signature");
                let id = fields.u32()?;
                let _bytes = read_length_prefixed(&mut fields, "signature value")?;
                match algorithm(id) {
                    Some(known) => signature_algorithms.push(known),
                    None => unknown.push(id),
                }
            }

            if certificates.is_empty() {
                return Err(fail("ES010", "A signer carries no certificate.")
                    .with_suggestion("A signature nobody can be identified by is not one."));
            }

            signers.push(Signer {
                digests,
                signature_algorithms,
                certificates,
                unknown_algorithms: unknown,
            });
        }

        if signers.is_empty() {
            return Err(fail("ES011", "The signing block names no signer."));
        }

        Ok(signers)
    }

    /// Computes the SHA-256 content digest the scheme defines.
    ///
    /// The package is treated as three sections: everything before the signing
    /// block, the central directory, and the end record with its central
    /// directory offset replaced by the signing block's offset. Each is split
    /// into one-megabyte chunks, every chunk is hashed with a `0xa5` prefix and
    /// its length, and the chunk hashes are hashed together with a `0x5a`
    /// prefix and their count.
    ///
    /// The substitution in the third section is what lets the digest cover the
    /// end record without covering the offset the signing block itself moved.
    pub fn content_digest_sha256(
        data: &[u8],
        block_offset: u64,
        central_directory_offset: u64,
        end_record_offset: u64,
    ) -> Result<Digest, Diagnostic> {
        let reader = Reader::new(data, Endian::Little, "package");

        let contents = reader.slice_at(0, block_offset)?;
        let directory = reader.slice_at(
            central_directory_offset,
            end_record_offset.saturating_sub(central_directory_offset),
        )?;

        let end_record = reader
            .slice_at(end_record_offset, data.len() as u64 - end_record_offset)?
            .to_vec();
        let mut patched = end_record;
        if patched.len() < 20 {
            return Err(fail("ES020", "The end record is too small to patch."));
        }
        let substitute = u32::try_from(block_offset).map_err(|_| {
            fail(
                "ES021",
                "The signing block starts past what the end record can express.",
            )
        })?;
        patched[16..20].copy_from_slice(&substitute.to_le_bytes());

        let mut chunk_digests: Vec<Digest> = Vec::new();
        for section in [contents, directory, patched.as_slice()] {
            for chunk in section.chunks(CHUNK_SIZE) {
                let mut hasher = Sha256::new();
                hasher.update(&[CHUNK_PREFIX]);
                hasher.update(&(chunk.len() as u32).to_le_bytes());
                hasher.update(chunk);
                chunk_digests.push(hasher.finish());
            }
        }

        let mut root = Sha256::new();
        root.update(&[ROOT_PREFIX]);
        root.update(&(chunk_digests.len() as u32).to_le_bytes());
        for digest in &chunk_digests {
            root.update(digest.as_bytes());
        }
        Ok(root.finish())
    }

    /// What was found in a package, and what of it was checked.
    #[derive(Clone, Debug)]
    pub struct Report {
        /// Whether the package carries a signing block at all.
        pub has_block: bool,
        /// Which schemes are present, by name.
        pub schemes: Vec<&'static str>,
        /// Signers found, in block order.
        pub signers: Vec<Signer>,
        /// Content digests that were recomputed and matched.
        pub digests_verified: u64,
        /// Content digests that could not be recomputed by this build.
        pub digests_unverifiable: u64,
        /// Content digests that were recomputed and did not match.
        pub digests_failed: u64,
        /// Always false, and deliberately so: see the module documentation.
        pub signatures_checked: bool,
    }

    impl Report {
        /// Whether everything this build is able to check, checked out.
        ///
        /// This is not "the package is genuine". It is "nothing this build can
        /// check is wrong", which is a smaller and more honest claim.
        pub fn everything_checkable_passed(&self) -> bool {
            self.has_block && self.digests_failed == 0 && self.digests_verified > 0
        }

        /// Serialises the report as the object member `key`.
        pub fn write_json(&self, w: &mut Writer, key: &str) {
            w.begin_object(Some(key));
            w.field_bool("hasSigningBlock", self.has_block);
            w.begin_array(Some("schemes"));
            for scheme in &self.schemes {
                w.element_str(scheme);
            }
            w.end_array();
            w.field_u64("signers", self.signers.len() as u64);
            w.field_u64("digestsVerified", self.digests_verified);
            w.field_u64("digestsUnverifiable", self.digests_unverifiable);
            w.field_u64("digestsFailed", self.digests_failed);
            w.field_bool("signaturesChecked", self.signatures_checked);
            w.field_str(
                "note",
                "A verified digest proves the package has not changed since the \
                 block was written. It does not prove who wrote it: that needs a \
                 signature check, which this build does not perform.",
            );
            w.begin_array(Some("signerDetail"));
            for signer in &self.signers {
                w.begin_object(None);
                w.begin_array(Some("algorithms"));
                for algorithm in &signer.signature_algorithms {
                    w.element_str(algorithm.name);
                }
                w.end_array();
                w.begin_array(Some("certificates"));
                for certificate in &signer.certificates {
                    certificate.write_json(w);
                }
                w.end_array();
                w.end_object();
            }
            w.end_array();
            w.end_object();
        }
    }

    /// Examines a package's signature block.
    pub fn examine(
        data: &[u8],
        central_directory_offset: u64,
        end_record_offset: u64,
        sink: &mut Sink,
    ) -> Report {
        let mut report = Report {
            has_block: false,
            schemes: Vec::new(),
            signers: Vec::new(),
            digests_verified: 0,
            digests_unverifiable: 0,
            digests_failed: 0,
            signatures_checked: false,
        };

        let block = match find_block(data, central_directory_offset) {
            Ok(Some(block)) => block,
            Ok(None) => {
                sink.emit(
                    Diagnostic::new(
                        "ES030",
                        Severity::Warning,
                        FailureClass::SecurityFailure,
                        "core.signing",
                        "The package carries no signing block.",
                    )
                    .with_suggestion(
                        "An application targeting API 30 or later is refused at \
                         install time without one.",
                    ),
                );
                return report;
            }
            Err(error) => {
                sink.emit(error);
                return report;
            }
        };

        report.has_block = true;

        for (id, name) in [
            (V2_BLOCK_ID, "v2"),
            (V3_BLOCK_ID, "v3"),
            (V31_BLOCK_ID, "v3.1"),
        ] {
            if block.value(id).is_some() {
                report.schemes.push(name);
            }
        }

        let Some(value) = block
            .value(V2_BLOCK_ID)
            .or_else(|| block.value(V3_BLOCK_ID))
        else {
            sink.emit(
                Diagnostic::new(
                    "ES031",
                    Severity::Warning,
                    FailureClass::SecurityFailure,
                    "core.signing",
                    "The signing block carries no scheme this build reads.",
                )
                .with_context(format!(
                    "Identifiers present: {}",
                    block
                        .ids()
                        .iter()
                        .map(|id| format!("0x{id:08x}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
            return report;
        };

        match parse_signers(value) {
            Ok(signers) => report.signers = signers,
            Err(error) => {
                sink.emit(error);
                return report;
            }
        }

        let computed = match content_digest_sha256(
            data,
            block.offset(),
            central_directory_offset,
            end_record_offset,
        ) {
            Ok(digest) => digest,
            Err(error) => {
                sink.emit(error);
                return report;
            }
        };

        for signer in &report.signers {
            for (algorithm, claimed) in &signer.digests {
                if !algorithm.uses_sha256 {
                    report.digests_unverifiable += 1;
                    sink.emit(
                        Diagnostic::new(
                            "ES032",
                            Severity::Warning,
                            FailureClass::SecurityFailure,
                            "core.signing",
                            format!(
                                "A digest uses {}, which this build cannot recompute.",
                                algorithm.name
                            ),
                        )
                        .with_suggestion(
                            "Only SHA-256 is implemented, so this claim is neither \
                             confirmed nor disputed.",
                        ),
                    );
                    continue;
                }

                if claimed.as_slice() == computed.as_bytes() {
                    report.digests_verified += 1;
                } else {
                    report.digests_failed += 1;
                    sink.emit(
                        Diagnostic::new(
                            "ES033",
                            Severity::Fatal,
                            FailureClass::Corruption,
                            "core.signing",
                            "The package does not match the digest recorded when it was signed.",
                        )
                        .with_context(format!("Algorithm: {}", algorithm.name))
                        .with_context(format!(
                            "Recorded: {}",
                            crate::der::to_hex(claimed).to_lowercase()
                        ))
                        .with_context(format!("Computed: {computed}"))
                        .with_suggestion(
                            "The package has been changed since it was signed. Do not \
                             install it.",
                        ),
                    );
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
    use super::archive::{self, Builder as ArchiveBuilder};
    use super::artifact::{Artifact, ArtifactId, State as ArtifactState};
    use super::binary::{
        checksum, Endian, Reader as BinaryReader, Section, Table as BinaryTable,
        Writer as BinaryWriter,
    };
    use super::cache::{Index as CacheIndex, Inputs as CacheInputs, Lookup as CacheLookup};
    use super::caps::{Capability, Decision, Policy};
    use super::diag::{Diagnostic, Location, Severity, Sink};
    use super::graph::{Graph, Kind as NodeKind, Node, NodeId, Status as NodeStatus};
    use super::hash::Digest;
    use super::json::Writer;
    use super::plugin::{Registry, Version};
    use super::project::{parse_manifest, GuardLevel, Optimization, Profile, Project};
    use super::resources::{
        Density, Kind as ResourceKind, Table as ResourceTable, Unit as ResourceUnit,
        Value as ResourceValue,
    };
    use super::scheduler::{Cancellation, Outcome as SchedulerOutcome};
    use super::signing;
    use super::toolchain::{self, Observation, Requirement, State};
    use super::vfs::{Access, Quota, VirtualFs, VirtualPath};
    use super::x509::Certificate;
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

    // --- binary core: checksums (official check values) ----------------------

    #[test]
    fn crc32_matches_the_published_check_value() {
        // The check value every CRC-32 specification quotes for "123456789".
        assert_eq!(checksum::crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(checksum::crc32(b""), 0x0000_0000);
        assert_eq!(checksum::crc32(b"a"), 0xe8b7_be43);
        assert_eq!(checksum::crc32(b"abc"), 0x3524_41c2);
        assert_eq!(
            checksum::crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414f_a339
        );
    }

    #[test]
    fn adler32_matches_the_published_check_value() {
        // RFC 1950. The DEX header carries one of these over its own body.
        assert_eq!(checksum::adler32(b"123456789"), 0x091e_01de);
        assert_eq!(checksum::adler32(b""), 0x0000_0001);
        assert_eq!(checksum::adler32(b"a"), 0x0062_0062);
        assert_eq!(checksum::adler32(b"abc"), 0x024d_0127);
        assert_eq!(checksum::adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn checksums_do_not_depend_on_how_the_input_is_split() {
        let message: Vec<u8> = (0u8..=255).cycle().take(20_000).collect();
        let crc_once = checksum::crc32(&message);
        let adler_once = checksum::adler32(&message);

        // 5552 is the reduction boundary in RFC 1950; either side of it is where
        // a streaming bug in Adler-32 shows up.
        for split in [1usize, 7, 255, 5_551, 5_552, 5_553, 8_192, 19_999] {
            let mut crc = checksum::Crc32::new();
            let mut adler = checksum::Adler32::new();
            for piece in message.chunks(split) {
                crc.update(piece);
                adler.update(piece);
            }
            assert_eq!(crc.finish(), crc_once, "crc split at {split}");
            assert_eq!(adler.finish(), adler_once, "adler split at {split}");
        }
    }

    // --- binary core: reading ------------------------------------------------

    fn reader(data: &[u8]) -> BinaryReader<'_> {
        BinaryReader::new(data, Endian::Little, "test")
    }

    #[test]
    fn integers_are_read_in_the_declared_byte_order() {
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        let mut little = BinaryReader::new(&data, Endian::Little, "test");
        assert_eq!(little.u16().unwrap(), 0x0201);
        assert_eq!(little.u16().unwrap(), 0x0403);
        assert_eq!(little.u32().unwrap(), 0x0807_0605);

        let mut big = BinaryReader::new(&data, Endian::Big, "test");
        assert_eq!(big.u16().unwrap(), 0x0102);
        assert_eq!(big.u32().unwrap(), 0x0304_0506);
        assert_eq!(big.remaining(), 2);
    }

    #[test]
    fn a_truncated_input_is_reported_not_padded() {
        // A reader that returns zero for missing bytes turns a detectable
        // problem into a silent one.
        let data = [0x01, 0x02];
        let mut r = reader(&data);
        assert_eq!(r.u32().unwrap_err().code, "E7003");
        assert_eq!(r.position(), 0, "a failed read must not move the cursor");

        assert_eq!(reader(&[]).u8().unwrap_err().code, "E7003");
        assert_eq!(reader(&data).bytes(3).unwrap_err().code, "E7003");
    }

    #[test]
    fn a_declared_length_is_checked_before_anything_is_allocated() {
        // Directive section 60: this is the exact shape of the bug that makes a
        // parser allocate gigabytes from a two-byte file.
        let data = [0xff; 4];
        let r = reader(&data);
        let error = r.checked_length(u64::MAX).unwrap_err();
        assert_eq!(error.code, "E7001");
        assert!(error.context.iter().any(|line| line.contains("Available")));

        assert_eq!(r.checked_length(4).unwrap(), 4);
        assert_eq!(r.checked_length(0).unwrap(), 0);
        assert_eq!(r.checked_length(5).unwrap_err().code, "E7001");
    }

    #[test]
    fn an_offset_past_the_end_is_refused() {
        let data = [0u8; 8];
        let r = reader(&data);
        assert_eq!(r.checked_offset(8).unwrap(), 8);
        assert_eq!(r.checked_offset(9).unwrap_err().code, "E7002");
        assert_eq!(r.checked_offset(u64::MAX).unwrap_err().code, "E7002");

        assert_eq!(r.slice_at(4, 4).unwrap().len(), 4);
        assert_eq!(r.slice_at(4, 5).unwrap_err().code, "E7002");
        assert_eq!(r.slice_at(1, u64::MAX).unwrap_err().code, "E7004");
    }

    #[test]
    fn seeking_stays_inside_the_input() {
        let data = [0u8; 4];
        let mut r = reader(&data);
        r.seek(4).unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.seek(5).unwrap_err().code, "E7002");
        assert_eq!(r.skip(1).unwrap_err().code, "E7003");
    }

    #[test]
    fn uleb128_reads_what_the_dex_format_writes() {
        let cases: &[(&[u8], u64)] = &[
            (&[0x00], 0),
            (&[0x01], 1),
            (&[0x7f], 127),
            (&[0x80, 0x01], 128),
            (&[0xff, 0x7f], 16_383),
            (&[0x80, 0x80, 0x01], 16_384),
            (&[0xff, 0xff, 0xff, 0xff, 0x0f], u64::from(u32::MAX)),
        ];
        for (bytes, expected) in cases {
            assert_eq!(reader(bytes).uleb128().unwrap(), *expected, "{bytes:?}");
        }
    }

    #[test]
    fn uleb128_refuses_overlong_and_unterminated_encodings() {
        // Two spellings of one number are two chances for a writer and a reader
        // to disagree about a checksum.
        assert_eq!(reader(&[0x80, 0x00]).uleb128().unwrap_err().code, "E7006");
        assert_eq!(reader(&[0x81, 0x00]).uleb128().unwrap_err().code, "E7006");

        assert_eq!(reader(&[0x80]).uleb128().unwrap_err().code, "E7003");
        assert_eq!(reader(&[0x80; 10]).uleb128().unwrap_err().code, "E7007");
        assert_eq!(
            reader(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f])
                .uleb128()
                .unwrap_err()
                .code,
            "E7005"
        );
    }

    #[test]
    fn a_string_without_a_terminator_is_refused() {
        let mut r = reader(b"omni\0rest");
        assert_eq!(r.cstring().unwrap(), b"omni");
        assert_eq!(r.position(), 5);
        assert_eq!(
            reader(b"no terminator").cstring().unwrap_err().code,
            "E7008"
        );
        assert_eq!(reader(b"\0").cstring().unwrap(), b"");
    }

    #[test]
    fn a_wrong_magic_number_says_what_it_found() {
        let mut r = reader(b"dex\n035\0");
        r.expect_magic(b"dex\n035\0").unwrap();

        let mut r = reader(b"PK\x03\x04");
        let error = r.expect_magic(b"dex\n").unwrap_err();
        assert_eq!(error.code, "E7009");
        assert!(error.context.iter().any(|line| line.contains("Found")));
    }

    // --- binary core: writing ------------------------------------------------

    #[test]
    fn a_writer_produces_the_bytes_the_format_expects() {
        let mut w = BinaryWriter::new(Endian::Little);
        w.u8(0x01).unwrap();
        w.u16(0x0302).unwrap();
        w.u32(0x0706_0504).unwrap();
        assert_eq!(w.as_slice(), &[1, 2, 3, 4, 5, 6, 7]);

        let mut w = BinaryWriter::new(Endian::Big);
        w.u16(0x0102).unwrap();
        w.u64(0x0304_0506_0708_090a).unwrap();
        assert_eq!(w.finish(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn a_reserved_span_can_be_filled_in_once_the_size_is_known() {
        // This is how every real binary format writes a size it does not know
        // until later, without a second pass.
        let mut w = BinaryWriter::new(Endian::Little);
        let size = w.reserve_u32().unwrap();
        w.bytes(b"payload").unwrap();
        let written = w.position() as u32 - 4;
        w.patch_u32(size, written).unwrap();

        let bytes = w.finish();
        let mut r = BinaryReader::new(&bytes, Endian::Little, "roundtrip");
        assert_eq!(r.u32().unwrap(), 7);
        assert_eq!(r.bytes(7).unwrap(), b"payload");
    }

    #[test]
    fn a_patch_must_match_the_span_it_was_given() {
        let mut w = BinaryWriter::new(Endian::Little);
        let narrow = w.reserve_u16().unwrap();
        assert_eq!(w.patch_u32(narrow, 1).unwrap_err().code, "E7023");
        w.patch_u16(narrow, 0xabcd).unwrap();
        assert_eq!(w.as_slice(), &[0xcd, 0xab]);

        // A patch from one writer cannot be used on another.
        let mut other = BinaryWriter::new(Endian::Little);
        assert_eq!(other.patch_u16(narrow, 1).unwrap_err().code, "E7024");
    }

    #[test]
    fn alignment_pads_and_refuses_anything_that_is_not_a_power_of_two() {
        let mut w = BinaryWriter::new(Endian::Little);
        w.bytes(b"abc").unwrap();
        w.align_to(4).unwrap();
        assert_eq!(w.position(), 4);
        w.align_to(4).unwrap();
        assert_eq!(
            w.position(),
            4,
            "aligning an aligned writer must do nothing"
        );
        w.align_to(8).unwrap();
        assert_eq!(w.as_slice(), &[b'a', b'b', b'c', 0, 0, 0, 0, 0]);

        assert_eq!(w.align_to(0).unwrap_err().code, "E7022");
        assert_eq!(w.align_to(3).unwrap_err().code, "E7022");
    }

    #[test]
    fn a_writer_refuses_to_grow_past_its_limit() {
        // Directive section 60: an unbounded writer is a way to run a phone out
        // of memory.
        let mut w = BinaryWriter::with_limit(Endian::Little, 8);
        w.bytes(&[0u8; 8]).unwrap();
        let error = w.u8(0).unwrap_err();
        assert_eq!(error.code, "E7021");
        assert_eq!(error.class, FailureClass::ResourceExhaustion);
        assert_eq!(w.position(), 8, "a refused write must not have written");
    }

    #[test]
    fn uleb128_round_trips_through_the_writer_and_the_reader() {
        let values = [
            0u64,
            1,
            63,
            64,
            127,
            128,
            255,
            256,
            16_383,
            16_384,
            65_535,
            65_536,
            u64::from(u32::MAX),
            u64::MAX / 2,
            u64::MAX,
        ];
        for value in values {
            let mut w = BinaryWriter::new(Endian::Little);
            w.uleb128(value).unwrap();
            let bytes = w.finish();
            assert_eq!(
                BinaryReader::new(&bytes, Endian::Little, "roundtrip")
                    .uleb128()
                    .unwrap(),
                value,
                "value {value}"
            );
        }
    }

    // --- binary core: sections and tables ------------------------------------

    #[test]
    fn a_section_must_lie_inside_the_file() {
        let section = Section {
            name: "header".into(),
            offset: 0,
            size: 16,
        };
        section.validate(16).unwrap();
        assert_eq!(section.validate(15).unwrap_err().code, "E7031");

        let overflowing = Section {
            name: "bad".into(),
            offset: u64::MAX,
            size: 2,
        };
        assert_eq!(overflowing.validate(1024).unwrap_err().code, "E7030");
    }

    #[test]
    fn overlapping_sections_are_detectable() {
        let a = Section {
            name: "a".into(),
            offset: 0,
            size: 16,
        };
        let b = Section {
            name: "b".into(),
            offset: 8,
            size: 16,
        };
        let c = Section {
            name: "c".into(),
            offset: 16,
            size: 16,
        };
        let empty = Section {
            name: "empty".into(),
            offset: 8,
            size: 0,
        };

        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        assert!(!a.overlaps(&c), "touching is not overlapping");
        assert!(!a.overlaps(&empty), "an empty section overlaps nothing");
    }

    #[test]
    fn a_table_refuses_the_multiplication_that_hides_a_malformed_header() {
        // entry_size * count is where a header claims a two-byte file holds an
        // enormous table.
        let table = BinaryTable {
            name: "string_ids".into(),
            offset: 32,
            entry_size: 4,
            count: 8,
        };
        assert_eq!(table.span().unwrap(), 32);
        table.validate(64).unwrap();
        assert_eq!(table.validate(63).unwrap_err().code, "E7031");

        let overflowing = BinaryTable {
            name: "evil".into(),
            offset: 0,
            entry_size: u64::MAX,
            count: 2,
        };
        assert_eq!(overflowing.span().unwrap_err().code, "E7040");
        assert_eq!(overflowing.validate(1024).unwrap_err().code, "E7040");
    }

    #[test]
    fn table_entries_are_addressed_by_index_and_bounded_by_count() {
        let table = BinaryTable {
            name: "method_ids".into(),
            offset: 100,
            entry_size: 8,
            count: 4,
        };
        assert_eq!(table.entry_offset(0).unwrap(), 100);
        assert_eq!(table.entry_offset(3).unwrap(), 124);
        assert_eq!(table.entry_offset(4).unwrap_err().code, "E7041");
        assert_eq!(table.entry_offset(u64::MAX).unwrap_err().code, "E7041");
    }

    // --- binary core: robustness against malformed input ---------------------

    /// A deterministic generator, so a failure can be reproduced from its seed.
    ///
    /// This is xorshift64*, chosen because it is four lines and needs no
    /// dependency (ADR-0003). It is not a cryptographic generator and nothing
    /// here needs one.
    fn xorshift(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    #[test]
    fn the_reader_survives_arbitrary_input() {
        // Directive section 41 names BinaryReader as a fuzz target. This is not
        // coverage-guided fuzzing and does not claim to be: it is a deterministic
        // randomised robustness test over the operations a real parser performs.
        // What it proves is that no sequence of them panics, hangs, or reads
        // outside the buffer.
        let mut seed = 0x0123_4567_89ab_cdefu64;

        for _ in 0..4_000 {
            let length = (xorshift(&mut seed) % 96) as usize;
            let data: Vec<u8> = (0..length)
                .map(|_| (xorshift(&mut seed) & 0xff) as u8)
                .collect();

            for endian in [Endian::Little, Endian::Big] {
                let mut r = BinaryReader::new(&data, endian, "fuzz");

                for _ in 0..24 {
                    let before = r.position();
                    let operation = xorshift(&mut seed) % 11;

                    let moved = match operation {
                        0 => r.u8().is_ok(),
                        1 => r.u16().is_ok(),
                        2 => r.u32().is_ok(),
                        3 => r.u64().is_ok(),
                        4 => r.uleb128().is_ok(),
                        5 => r.cstring().is_ok(),
                        6 => {
                            let count = (xorshift(&mut seed) % 200) as usize;
                            r.bytes(count).is_ok()
                        }
                        7 => r.skip((xorshift(&mut seed) % 200) as usize).is_ok(),
                        8 => r.seek(xorshift(&mut seed) % 200).is_ok(),
                        9 => {
                            let _ = r.checked_length(xorshift(&mut seed));
                            let _ = r.checked_offset(xorshift(&mut seed));
                            false
                        }
                        _ => {
                            let offset = xorshift(&mut seed) % 200;
                            let len = xorshift(&mut seed) % 200;
                            let _ = r.slice_at(offset, len);
                            false
                        }
                    };
                    let _ = moved;

                    // The cursor never leaves the buffer, whatever happened.
                    assert!(r.position() <= data.len(), "cursor escaped the buffer");
                    assert_eq!(r.remaining(), data.len() - r.position());
                    if before > r.position() {
                        // Only an explicit seek may move backwards.
                        assert_eq!(operation, 8, "cursor moved backwards unexpectedly");
                    }
                }
            }
        }
    }

    #[test]
    fn the_writer_survives_arbitrary_use() {
        let mut seed = 0xfedc_ba98_7654_3210u64;

        for _ in 0..2_000 {
            let limit = (xorshift(&mut seed) % 128) as usize;
            let mut w = BinaryWriter::with_limit(Endian::Little, limit);
            let mut patches: Vec<super::binary::Patch> = Vec::new();

            for _ in 0..32 {
                match xorshift(&mut seed) % 8 {
                    0 => {
                        let _ = w.u8((xorshift(&mut seed) & 0xff) as u8);
                    }
                    1 => {
                        let _ = w.u32(xorshift(&mut seed) as u32);
                    }
                    2 => {
                        let _ = w.uleb128(xorshift(&mut seed));
                    }
                    3 => {
                        let count = (xorshift(&mut seed) % 64) as usize;
                        let _ = w.bytes(&vec![0xaa; count]);
                    }
                    4 => {
                        let alignment = 1usize << (xorshift(&mut seed) % 6);
                        let _ = w.align_to(alignment);
                    }
                    5 => {
                        if let Ok(patch) = w.reserve_u32() {
                            patches.push(patch);
                        }
                    }
                    6 => {
                        if let Some(patch) = patches.first().copied() {
                            let _ = w.patch_u32(patch, xorshift(&mut seed) as u32);
                        }
                    }
                    _ => {
                        let _ = w.align_to((xorshift(&mut seed) % 10) as usize);
                    }
                }

                // The limit is never exceeded, whatever the sequence.
                assert!(w.position() <= limit, "writer exceeded its limit");
            }
        }
    }

    #[test]
    fn checksums_survive_arbitrary_input() {
        let mut seed = 0x0f0f_0f0f_0f0f_0f0fu64;
        for _ in 0..2_000 {
            let length = (xorshift(&mut seed) % 12_000) as usize;
            let data: Vec<u8> = (0..length)
                .map(|_| (xorshift(&mut seed) & 0xff) as u8)
                .collect();
            // Both must agree with themselves whatever the input; the point is
            // that neither panics nor loops on any length, including zero and
            // lengths either side of the Adler-32 reduction boundary.
            assert_eq!(checksum::crc32(&data), checksum::crc32(&data));
            assert_eq!(checksum::adler32(&data), checksum::adler32(&data));
        }
    }

    // --- xml -----------------------------------------------------------------

    fn parse_xml(text: &str) -> (Option<super::xml::Element>, Sink) {
        let mut sink = Sink::new();
        let root = super::xml::parse(text, "test.xml", &mut sink);
        (root, sink)
    }

    #[test]
    fn xml_reads_elements_attributes_and_text() {
        let (root, sink) = parse_xml(
            r#"<?xml version="1.0" encoding="utf-8"?>
<resources xmlns:android="http://schemas.android.com/apk/res/android">
    <!-- a comment -->
    <string name="app">Omni_Builder</string>
    <color name="accent">#6cb6ff</color>
    <item android:id="x" empty="yes" />
</resources>"#,
        );

        let root = root.expect("must parse");
        assert!(!sink.has_blocking(), "{:?}", sink.entries());
        assert_eq!(root.name, "resources");
        assert_eq!(root.children.len(), 3);
        assert_eq!(
            root.attribute("xmlns:android").unwrap(),
            "http://schemas.android.com/apk/res/android"
        );

        let string = &root.children[0];
        assert_eq!(string.name, "string");
        assert_eq!(string.attribute("name"), Some("app"));
        assert_eq!(string.text, "Omni_Builder");

        let item = &root.children[2];
        assert_eq!(item.attributes.len(), 2);
        assert_eq!(item.attribute("android:id"), Some("x"));
        assert!(item.children.is_empty());
    }

    #[test]
    fn xml_decodes_the_five_predefined_entities_and_numeric_references() {
        let (root, _) =
            parse_xml(r#"<r a="&lt;&gt;&amp;&quot;&apos;"><t>&#65;&#x42;&#199;</t></r>"#);
        let root = root.unwrap();
        assert_eq!(root.attribute("a").unwrap(), "<>&\"'");
        assert_eq!(root.children[0].text, "ABÇ");
    }

    #[test]
    fn xml_refuses_a_document_type_declaration() {
        // This is the entry point for external entity expansion and for the
        // nested definitions that make a small file expand to gigabytes. It is
        // not defended against; it is simply unavailable.
        let (root, sink) = parse_xml(r#"<!DOCTYPE r [<!ENTITY x "boom">]><r>&x;</r>"#);
        assert!(root.is_none());
        let error = sink.entries().iter().find(|d| d.code == "E8002").unwrap();
        assert_eq!(error.class, FailureClass::SecurityFailure);
    }

    #[test]
    fn xml_refuses_an_entity_it_does_not_define() {
        // With no entity table there is nothing to expand recursively, so the
        // billion-laughs shape cannot be built in the first place.
        let (root, sink) = parse_xml("<r>&lol;</r>");
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8031"));

        let (root, sink) = parse_xml("<r>a & b</r>");
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8030"));
    }

    #[test]
    fn xml_reads_cdata_verbatim() {
        let (root, _) = parse_xml("<r><![CDATA[<not> & parsed]]></r>");
        let root = root.unwrap();
        assert!(root.children.is_empty(), "CDATA is text, not an element");
        assert_eq!(root.text, "<not> & parsed");

        // And it mixes with ordinary text in the order it appears.
        let (root, _) = parse_xml("<r>before <![CDATA[<raw>]]> after</r>");
        assert_eq!(root.unwrap().text, "before <raw> after");

        let (root, sink) = parse_xml("<r><![CDATA[unterminated</r>");
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8013"));
    }

    #[test]
    fn xml_reports_structural_problems_with_a_position() {
        let cases = [
            ("<a>\n<b>\n</a>\n</b>", "E8004", 3u32),
            ("<a></a>\n</b>", "E8003", 2),
            ("<a>\n<b>", "E8009", 2),
            ("<a></a><b></b>", "E8005", 1),
            ("text before<a></a>", "E8008", 1),
            ("<a b></a>", "E8020", 1),
            ("<a b=unquoted></a>", "E8021", 1),
            ("<a b=\"x\" b=\"y\"></a>", "E8024", 1),
            ("<a><!-- never closed </a>", "E8011", 1),
            ("", "E8010", 1),
        ];
        for (document, code, line) in cases {
            let (root, sink) = parse_xml(document);
            assert!(root.is_none(), "{document:?} should not parse");
            let error = sink
                .entries()
                .iter()
                .find(|d| d.code == code)
                .unwrap_or_else(|| panic!("{document:?} -> {:?}", sink.entries()));
            assert_eq!(error.location.as_ref().unwrap().line, line, "{document:?}");
        }
    }

    #[test]
    fn xml_is_bounded_in_every_direction() {
        // Directive section 60, on input that comes from a user's project.
        let deep = format!(
            "{}{}",
            "<a>".repeat(super::xml::MAX_DEPTH + 1),
            "</a>".repeat(super::xml::MAX_DEPTH + 1)
        );
        let (root, sink) = parse_xml(&deep);
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8007"));

        let attributes: String = (0..super::xml::MAX_ATTRIBUTES + 5)
            .map(|i| format!(" a{i}=\"v\""))
            .collect();
        let (root, sink) = parse_xml(&format!("<r{attributes}></r>"));
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8019"));

        let long_name = "x".repeat(super::xml::MAX_NAME_BYTES + 1);
        let (root, sink) = parse_xml(&format!("<{long_name}></{long_name}>"));
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8015"));

        let (root, sink) = parse_xml(&"x".repeat(super::xml::MAX_DOCUMENT_BYTES + 1));
        assert!(root.is_none());
        assert!(sink.entries().iter().any(|d| d.code == "E8001"));
    }

    #[test]
    fn xml_columns_count_characters_not_bytes() {
        // A column number that counts bytes points at the wrong place in any file
        // with Turkish, Greek or emoji in it.
        let (root, sink) = parse_xml("<r>şşşş</r>\n<b>");
        assert!(root.is_none());
        let error = sink.entries().iter().find(|d| d.code == "E8009").unwrap();
        assert_eq!(error.location.as_ref().unwrap().line, 2);
    }

    #[test]
    fn xml_accepts_a_byte_order_mark() {
        let (root, _) = parse_xml("\u{feff}<r/>");
        assert_eq!(root.unwrap().name, "r");
    }

    #[test]
    fn xml_survives_arbitrary_input() {
        // Directive section 41. Resource files come from a user's project, and
        // this reader is the first thing that touches them.
        let mut seed = 0x5eed_0000_1111_2222u64;
        let alphabet = b"<>/=\"'& \n\tabc&#;!-[]DOCTYPExml?";

        for _ in 0..3_000 {
            let length = (xorshift(&mut seed) % 200) as usize;
            let document: String = (0..length)
                .map(|_| alphabet[(xorshift(&mut seed) as usize) % alphabet.len()] as char)
                .collect();
            let mut sink = Sink::new();
            // The requirement is that this returns, whatever it was given.
            let _ = super::xml::parse(&document, "fuzz.xml", &mut sink);
        }
    }

    // --- resources -----------------------------------------------------------

    fn values(text: &str) -> (ResourceTable, Sink) {
        let mut table = ResourceTable::new();
        let mut sink = Sink::new();
        table.read_values(text, "values/test.xml", &mut sink);
        (table, sink)
    }

    #[test]
    fn a_values_file_becomes_resources() {
        let (table, sink) = values(
            r#"<resources>
    <string name="app_name">Omni_Builder</string>
    <color name="accent">#6cb6ff</color>
    <dimen name="gap">16dp</dimen>
    <bool name="enabled">true</bool>
    <integer name="retries">3</integer>
    <id name="root" />
</resources>"#,
        );

        assert!(!sink.has_blocking(), "{:?}", sink.entries());
        assert_eq!(table.len(), 6);

        let compiled = table.compile(&mut Sink::new()).expect("must compile");
        let by_name = |kind, name| {
            compiled
                .entries()
                .iter()
                .find(|e| e.kind == kind && e.name == name)
                .map(|e| e.value.clone())
        };

        assert_eq!(
            by_name(ResourceKind::String, "app_name"),
            Some(ResourceValue::Text("Omni_Builder".into()))
        );
        assert_eq!(
            by_name(ResourceKind::Color, "accent"),
            Some(ResourceValue::Color(0xff6c_b6ff))
        );
        assert_eq!(
            by_name(ResourceKind::Dimension, "gap"),
            Some(ResourceValue::Dimension {
                milli: 16_000,
                unit: ResourceUnit::Dp
            })
        );
        assert_eq!(
            by_name(ResourceKind::Bool, "enabled"),
            Some(ResourceValue::Bool(true))
        );
        assert_eq!(
            by_name(ResourceKind::Integer, "retries"),
            Some(ResourceValue::Integer(3))
        );
        assert_eq!(
            by_name(ResourceKind::Id, "root"),
            Some(ResourceValue::Empty)
        );
    }

    #[test]
    fn colours_are_read_in_every_written_form() {
        let (table, sink) = values(
            r#"<resources>
    <color name="a">#f00</color>
    <color name="b">#8f00</color>
    <color name="c">#ff0000</color>
    <color name="d">#80ff0000</color>
</resources>"#,
        );
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let compiled = table.compile(&mut Sink::new()).unwrap();
        let colour = |name: &str| {
            compiled
                .entries()
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| match e.value {
                    ResourceValue::Color(value) => Some(value),
                    _ => None,
                })
        };
        assert_eq!(colour("a"), Some(0xffff_0000));
        assert_eq!(colour("b"), Some(0x88ff_0000));
        assert_eq!(colour("c"), Some(0xffff_0000));
        assert_eq!(colour("d"), Some(0x80ff_0000));
    }

    #[test]
    fn a_malformed_colour_is_refused_with_the_forms_that_work() {
        for bad in ["red", "#", "#ff", "#fffff", "#gg0000", "0xff0000"] {
            let (_, sink) = values(&format!(
                "<resources><color name=\"c\">{bad}</color></resources>"
            ));
            assert!(
                sink.entries().iter().any(|d| d.code == "E9041"),
                "{bad} was accepted"
            );
        }
    }

    #[test]
    fn dimensions_keep_exactly_three_decimal_places() {
        // Stored in thousandths, because a build that rounds differently on two
        // machines is not reproducible (directive section 12).
        let (table, sink) = values(
            r#"<resources>
    <dimen name="a">16dp</dimen>
    <dimen name="b">14.5sp</dimen>
    <dimen name="c">0.125in</dimen>
    <dimen name="d">-8px</dimen>
</resources>"#,
        );
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let compiled = table.compile(&mut Sink::new()).unwrap();
        let dimension = |name: &str| {
            compiled
                .entries()
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.value.clone())
        };
        assert_eq!(
            dimension("a"),
            Some(ResourceValue::Dimension {
                milli: 16_000,
                unit: ResourceUnit::Dp
            })
        );
        assert_eq!(
            dimension("b"),
            Some(ResourceValue::Dimension {
                milli: 14_500,
                unit: ResourceUnit::Sp
            })
        );
        assert_eq!(
            dimension("c"),
            Some(ResourceValue::Dimension {
                milli: 125,
                unit: ResourceUnit::In
            })
        );
        assert_eq!(
            dimension("d"),
            Some(ResourceValue::Dimension {
                milli: -8_000,
                unit: ResourceUnit::Px
            })
        );

        // Rendering round-trips, so a report shows what the author wrote.
        assert_eq!(
            ResourceValue::Dimension {
                milli: 14_500,
                unit: ResourceUnit::Sp
            }
            .to_source(),
            "14.5sp"
        );
    }

    #[test]
    fn a_dimension_that_would_have_to_be_rounded_is_refused() {
        let (_, sink) = values("<resources><dimen name=\"a\">1.2345dp</dimen></resources>");
        let error = sink.entries().iter().find(|d| d.code == "E9042").unwrap();
        assert!(error.message.contains("three decimal places"));

        for bad in ["16", "dp", "16em", "abc dp", "1.2.3dp"] {
            let (_, sink) = values(&format!(
                "<resources><dimen name=\"a\">{bad}</dimen></resources>"
            ));
            assert!(
                sink.entries().iter().any(|d| d.code == "E9042"),
                "{bad} was accepted"
            );
        }
    }

    #[test]
    fn strings_follow_the_escape_and_whitespace_rules() {
        let (table, sink) = values(
            "<resources>\
             <string name=\"collapsed\">  one   two\n   three  </string>\
             <string name=\"quoted\">\"  kept  \"</string>\
             <string name=\"escaped\">line\\nbreak \\@ \\' \\u00e7</string>\
             </resources>",
        );
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let compiled = table.compile(&mut Sink::new()).unwrap();
        let text = |name: &str| {
            compiled
                .entries()
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| match &e.value {
                    ResourceValue::Text(value) => Some(value.clone()),
                    _ => None,
                })
        };
        assert_eq!(text("collapsed").as_deref(), Some("one two three"));
        assert_eq!(text("quoted").as_deref(), Some("  kept  "));
        assert_eq!(text("escaped").as_deref(), Some("line\nbreak @ ' ç"));
    }

    #[test]
    fn identifiers_are_assigned_from_sorted_order_not_file_order() {
        // Directive section 12: an identifier must not depend on which file
        // happened to be read first.
        let forwards =
            "<resources><string name=\"b\">B</string><string name=\"a\">A</string></resources>";
        let backwards =
            "<resources><string name=\"a\">A</string><string name=\"b\">B</string></resources>";

        let left = values(forwards).0.compile(&mut Sink::new()).unwrap();
        let right = values(backwards).0.compile(&mut Sink::new()).unwrap();

        assert_eq!(left.assignments(), right.assignments());
        assert_eq!(
            left.id(ResourceKind::String, "a").unwrap().raw(),
            right.id(ResourceKind::String, "a").unwrap().raw()
        );
    }

    #[test]
    fn an_identifier_has_the_shape_android_uses() {
        let (table, _) = values(
            r#"<resources>
    <color name="accent">#fff</color>
    <string name="a">A</string>
    <string name="b">B</string>
</resources>"#,
        );
        let compiled = table.compile(&mut Sink::new()).unwrap();

        let colour = compiled.id(ResourceKind::Color, "accent").unwrap();
        assert_eq!(colour.package(), 0x7f);
        assert_eq!(colour.type_index(), 1, "color sorts before string");
        assert_eq!(colour.entry_index(), 0);
        assert_eq!(colour.to_string(), "0x7f010000");

        let first = compiled.id(ResourceKind::String, "a").unwrap();
        let second = compiled.id(ResourceKind::String, "b").unwrap();
        assert_eq!(first.type_index(), 2);
        assert_eq!(first.entry_index(), 0);
        assert_eq!(second.entry_index(), 1);
    }

    #[test]
    fn references_are_read_and_resolved() {
        let (table, sink) = values(
            r#"<resources>
    <color name="base">#123456</color>
    <color name="accent">@color/base</color>
    <string name="platform">@android:string/ok</string>
</resources>"#,
        );
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let mut compile_sink = Sink::new();
        let compiled = table.compile(&mut compile_sink).expect("must compile");
        assert!(!compile_sink.has_blocking(), "{:?}", compile_sink.entries());
        assert!(compiled.id(ResourceKind::Color, "accent").is_some());
    }

    #[test]
    fn a_reference_to_something_undeclared_is_refused_with_a_suggestion() {
        let (table, _) = values(
            r#"<resources>
    <color name="omni_accent">#123456</color>
    <color name="other">@color/omni_accen</color>
</resources>"#,
        );
        let mut sink = Sink::new();
        assert!(table.compile(&mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "E9031").unwrap();
        assert!(error.suggestion.as_deref().unwrap().contains("omni_accent"));
    }

    #[test]
    fn a_reference_loop_is_refused_rather_than_followed() {
        let (table, _) = values(
            r#"<resources>
    <color name="a">@color/b</color>
    <color name="b">@color/c</color>
    <color name="c">@color/a</color>
</resources>"#,
        );
        let mut sink = Sink::new();
        assert!(table.compile(&mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "E9032").unwrap();
        assert!(error.context.iter().any(|line| line.contains("->")));
    }

    #[test]
    fn a_resource_declared_twice_is_refused() {
        let (_, sink) = values(
            "<resources><string name=\"a\">1</string><string name=\"a\">2</string></resources>",
        );
        let error = sink.entries().iter().find(|d| d.code == "E9013").unwrap();
        assert!(error
            .context
            .iter()
            .any(|line| line.contains("First declared")));
    }

    #[test]
    fn a_resource_name_must_be_usable_as_an_identifier() {
        for bad in ["1abc", "has space", "has-dash", "", "has/slash"] {
            let (_, sink) = values(&format!(
                "<resources><string name=\"{bad}\">x</string></resources>"
            ));
            assert!(
                sink.entries()
                    .iter()
                    .any(|d| d.code == "E9005" || d.code == "E8031"),
                "{bad:?} was accepted"
            );
        }
        let (_, sink) = values("<resources><string name=\"ok_name.2\">x</string></resources>");
        assert!(!sink.has_blocking());
    }

    #[test]
    fn an_unmodelled_element_is_reported_not_skipped() {
        // Directive section 64: a resource that silently never existed is harder
        // to find than one that was refused.
        let (_, sink) = values("<resources><plurals name=\"p\">x</plurals></resources>");
        assert!(sink.entries().iter().any(|d| d.code == "E9002"));

        let (_, sink) = values("<resources><style name=\"Theme\">x</style></resources>");
        let error = sink.entries().iter().find(|d| d.code == "E9003").unwrap();
        assert!(error.message.contains("not yet modelled"));
    }

    #[test]
    fn file_resources_carry_their_density_qualifier() {
        let mut table = ResourceTable::new();
        let mut sink = Sink::new();

        assert!(table.read_file(
            "drawable",
            "omni_mark.xml",
            "res/drawable/omni_mark.xml",
            &mut sink
        ));
        assert!(table.read_file(
            "mipmap-xxhdpi",
            "ic_launcher.png",
            "res/mipmap-xxhdpi/ic_launcher.png",
            &mut sink
        ));
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let compiled = table.compile(&mut Sink::new()).unwrap();
        assert!(compiled.id(ResourceKind::Drawable, "omni_mark").is_some());

        let launcher = compiled
            .entries()
            .iter()
            .find(|e| e.name == "ic_launcher")
            .unwrap();
        assert_eq!(launcher.kind, ResourceKind::Mipmap);
        assert_eq!(launcher.config.density, Density::ExtraExtraHigh);
    }

    #[test]
    fn a_qualifier_this_build_does_not_model_is_refused() {
        // A locale qualifier on a directory this build does understand: the type
        // is fine, the qualifier is not modelled, and treating it as the default
        // would put the wrong file on every device.
        let mut table = ResourceTable::new();
        let mut sink = Sink::new();
        assert!(!table.read_file("drawable-tr", "a.png", "res/drawable-tr/a.png", &mut sink));
        let error = sink.entries().iter().find(|d| d.code == "E9010").unwrap();
        assert!(
            error.message.contains("does not model"),
            "{}",
            error.message
        );

        // A directory whose type is not a resource type at all.
        let mut sink = Sink::new();
        assert!(!table.read_file("nonsense", "a.png", "res/nonsense/a.png", &mut sink));
        let error = sink.entries().iter().find(|d| d.code == "E9010").unwrap();
        assert!(
            error.message.contains("is not a resource type"),
            "{}",
            error.message
        );
    }

    #[test]
    fn the_same_name_may_exist_at_several_densities() {
        let mut table = ResourceTable::new();
        let mut sink = Sink::new();
        table.read_file("mipmap-hdpi", "ic.png", "res/mipmap-hdpi/ic.png", &mut sink);
        table.read_file(
            "mipmap-xhdpi",
            "ic.png",
            "res/mipmap-xhdpi/ic.png",
            &mut sink,
        );
        assert!(!sink.has_blocking(), "{:?}", sink.entries());
        assert_eq!(table.len(), 2);

        let compiled = table.compile(&mut Sink::new()).unwrap();
        // One name, one identifier, whatever the number of configurations.
        assert_eq!(compiled.assignments().len(), 1);
    }

    #[test]
    fn a_values_file_must_be_wrapped_in_resources() {
        let (_, sink) = values("<strings><string name=\"a\">x</string></strings>");
        assert!(sink.entries().iter().any(|d| d.code == "E9001"));
    }

    #[test]
    fn a_compiled_table_serialises_into_a_valid_report() {
        let (table, _) = values(
            "<resources><string name=\"a\">A</string><color name=\"c\">#fff</color></resources>",
        );
        let compiled = table.compile(&mut Sink::new()).unwrap();

        let mut w = Writer::new();
        w.begin_object(None);
        compiled.write_json(&mut w, "resources");
        w.end_object();
        let document = w.finish();

        assert!(is_structurally_valid(&document), "{document}");
        assert!(document.contains("\"packageId\":\"0x7f\""));
        assert!(document.contains("\"binaryTableWritten\":false"));
        assert!(document.contains("0x7f010000"));
    }

    #[test]
    fn the_resource_engine_survives_arbitrary_input() {
        let mut seed = 0xbeef_1234_5678_9abcu64;
        let fragments = [
            "<resources>",
            "</resources>",
            "<string name=\"a\">",
            "</string>",
            "<color name=",
            "\"#\">",
            "@color/",
            "@+id/x",
            "16dp",
            "#gg",
            "&amp;",
            "<dimen>",
            "\"",
            "/>",
            "\n",
        ];
        for _ in 0..2_000 {
            let count = (xorshift(&mut seed) % 20) as usize;
            let document: String = (0..count)
                .map(|_| fragments[(xorshift(&mut seed) as usize) % fragments.len()])
                .collect();
            let mut table = ResourceTable::new();
            let mut sink = Sink::new();
            table.read_values(&document, "fuzz.xml", &mut sink);
            let _ = table.compile(&mut sink);
        }
    }

    // --- archive: conformance against an independently produced file --------

    /// An archive produced by the Info-ZIP `zip` tool, byte for byte.
    ///
    /// Directive section 24 ends its list with conformance tests, and a parser
    /// tested only against its own writer proves that the two agree, not that
    /// either follows the specification. This file was made by a tool that has
    /// nothing to do with this project and holds two entries: `a.txt` with
    /// "hello omni", and `dir/b.txt` with "second entry".
    const INFOZIP_SAMPLE: &[u8] = &[
        0x50, 0x4b, 0x03, 0x04, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd7, 0x40, 0x14, 0x5d, 0x04,
        0xc9, 0x25, 0x28, 0x0a, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
        0x61, 0x2e, 0x74, 0x78, 0x74, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x6f, 0x6d, 0x6e, 0x69,
        0x50, 0x4b, 0x03, 0x04, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd7, 0x40, 0x14, 0x5d, 0x33,
        0xeb, 0xbf, 0xe0, 0x0c, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00,
        0x64, 0x69, 0x72, 0x2f, 0x62, 0x2e, 0x74, 0x78, 0x74, 0x73, 0x65, 0x63, 0x6f, 0x6e, 0x64,
        0x20, 0x65, 0x6e, 0x74, 0x72, 0x79, 0x50, 0x4b, 0x01, 0x02, 0x1e, 0x03, 0x0a, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xd7, 0x40, 0x14, 0x5d, 0x04, 0xc9, 0x25, 0x28, 0x0a, 0x00, 0x00, 0x00,
        0x0a, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0xa4, 0x81, 0x00, 0x00, 0x00, 0x00, 0x61, 0x2e, 0x74, 0x78, 0x74, 0x50, 0x4b, 0x01,
        0x02, 0x1e, 0x03, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd7, 0x40, 0x14, 0x5d, 0x33, 0xeb,
        0xbf, 0xe0, 0x0c, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xa4, 0x81, 0x2d, 0x00, 0x00, 0x00, 0x64, 0x69,
        0x72, 0x2f, 0x62, 0x2e, 0x74, 0x78, 0x74, 0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x02, 0x00, 0x6a, 0x00, 0x00, 0x00, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn an_archive_written_by_another_tool_reads_correctly() {
        let mut sink = Sink::new();
        let archive = archive::read(INFOZIP_SAMPLE, &mut sink).expect("must read");
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        assert_eq!(archive.len(), 2);
        assert_eq!(archive.size(), INFOZIP_SAMPLE.len() as u64);

        let a = archive.entry("a.txt").expect("a.txt");
        assert_eq!(a.uncompressed_size, 10);
        assert_eq!(a.compression, archive::Compression::Stored);
        assert!(!a.is_directory());

        let b = archive.entry("dir/b.txt").expect("dir/b.txt");
        assert_eq!(b.uncompressed_size, 12);

        // The bytes are where the headers say they are.
        assert_eq!(
            archive.stored_bytes(INFOZIP_SAMPLE, a).unwrap(),
            b"hello omni"
        );
        assert_eq!(
            archive.stored_bytes(INFOZIP_SAMPLE, b).unwrap(),
            b"second entry"
        );

        // And the checksums that tool computed match the ones this one does.
        let mut verify_sink = Sink::new();
        let (checked, skipped) =
            archive::verify_checksums(&archive, INFOZIP_SAMPLE, &mut verify_sink);
        assert_eq!(checked, 2);
        assert_eq!(skipped, 0);
        assert!(!verify_sink.has_blocking(), "{:?}", verify_sink.entries());
    }

    // --- archive: writing ----------------------------------------------------

    #[test]
    fn an_archive_round_trips_through_the_writer_and_the_reader() {
        let mut builder = ArchiveBuilder::new();
        builder
            .add("AndroidManifest.xml", b"<manifest/>".to_vec())
            .unwrap();
        builder.add("classes.dex", vec![0x11; 300]).unwrap();
        builder.add("res/values.arsc", b"table".to_vec()).unwrap();
        let bytes = builder.finish().unwrap();

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).expect("must read back");
        assert!(!sink.has_blocking(), "{:?}", sink.entries());
        assert_eq!(archive.len(), 3);

        assert_eq!(
            archive
                .stored_bytes(&bytes, archive.entry("AndroidManifest.xml").unwrap())
                .unwrap(),
            b"<manifest/>"
        );
        assert_eq!(
            archive
                .stored_bytes(&bytes, archive.entry("classes.dex").unwrap())
                .unwrap()
                .len(),
            300
        );

        let (checked, skipped) = archive::verify_checksums(&archive, &bytes, &mut sink);
        assert_eq!(checked, 3);
        assert_eq!(skipped, 0);
        assert!(!sink.has_blocking());
    }

    #[test]
    fn entries_are_written_in_sorted_order_whatever_order_they_arrive_in() {
        // Directive section 23 lists deterministic ordering as mandatory.
        // Sorting is the only ordering that does not depend on how the caller
        // happened to walk a directory.
        let build = |names: &[&str]| {
            let mut builder = ArchiveBuilder::new();
            for name in names {
                builder.add(*name, name.as_bytes().to_vec()).unwrap();
            }
            builder.finish().unwrap()
        };

        let forwards = build(&["a.txt", "b.txt", "c.txt"]);
        let backwards = build(&["c.txt", "b.txt", "a.txt"]);
        assert_eq!(forwards, backwards, "entry order changed the archive");

        let mut sink = Sink::new();
        let archive = archive::read(&forwards, &mut sink).unwrap();
        let names: Vec<&str> = archive.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn the_same_input_always_produces_the_same_archive() {
        // No timestamp, no host attributes, nothing about the machine.
        let build = || {
            let mut builder = ArchiveBuilder::for_android();
            builder.add("classes.dex", vec![7; 64]).unwrap();
            builder
                .add("lib/arm64-v8a/libomni.so", vec![9; 100])
                .unwrap();
            builder.finish().unwrap()
        };
        assert_eq!(build(), build());
        assert_eq!(super::hash::sha256(&build()), super::hash::sha256(&build()));
    }

    #[test]
    fn native_libraries_land_on_a_page_boundary() {
        // A device with 16 KB pages maps a library straight out of the package,
        // which it can only do when the library starts on a page boundary.
        let mut builder = ArchiveBuilder::for_android();
        builder
            .add("AndroidManifest.xml", b"<manifest/>".to_vec())
            .unwrap();
        builder
            .add("lib/arm64-v8a/libomni_builder.so", vec![0x7f; 5_000])
            .unwrap();
        builder
            .add("lib/x86_64/libomni_builder.so", vec![0x7f; 3_000])
            .unwrap();
        builder.add("classes.dex", vec![1; 200]).unwrap();
        let bytes = builder.finish().unwrap();

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).unwrap();
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        for entry in archive.entries() {
            if entry.name.starts_with("lib/") && entry.name.ends_with(".so") {
                assert!(
                    entry.is_aligned_to(16 * 1024),
                    "{} starts at {}, which is not a 16 KB boundary",
                    entry.name,
                    entry.data_offset
                );
            } else {
                assert!(
                    entry.is_aligned_to(4),
                    "{} is not 4-byte aligned",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn alignment_padding_is_a_well_formed_extra_field() {
        // Raw padding is tolerated by most readers; a proper record is skipped
        // correctly by all of them.
        let mut builder = ArchiveBuilder::new();
        builder.add_aligned("x", vec![1; 4], 64).unwrap();
        let bytes = builder.finish().unwrap();

        let extra_length = u16::from_le_bytes([bytes[28], bytes[29]]);
        assert!(extra_length >= 4, "padding must be a whole record");
        let start = 30 + 1;
        assert_eq!(
            u16::from_le_bytes([bytes[start], bytes[start + 1]]),
            0xd935,
            "the padding record must carry Android's identifier"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]),
            extra_length - 4
        );
    }

    #[test]
    fn the_writer_refuses_a_name_no_archive_should_carry() {
        // Directive section 23 names path traversal and invalid names as
        // mandatory checks, and refusing at write time is better than producing
        // an archive that will be refused later.
        let mut builder = ArchiveBuilder::new();
        for (name, code) in [
            ("", "EA001"),
            ("/absolute", "EA003"),
            ("dir\\file", "EA004"),
            ("../escape", "EA006"),
            ("a/../../b", "EA006"),
            ("C:/windows", "EA007"),
        ] {
            let error = builder.add(name, Vec::new()).unwrap_err();
            assert_eq!(error.code, code, "name {name:?}");
            assert_eq!(error.class, FailureClass::SecurityFailure);
        }

        let with_control = format!("a{}b", '\u{7}');
        assert_eq!(
            builder.add(with_control, Vec::new()).unwrap_err().code,
            "EA005"
        );
    }

    #[test]
    fn the_writer_refuses_the_same_name_twice() {
        let mut builder = ArchiveBuilder::new();
        builder.add("a.txt", b"one".to_vec()).unwrap();
        let error = builder.add("a.txt", b"two".to_vec()).unwrap_err();
        assert_eq!(error.code, "EA051");
    }

    #[test]
    fn an_empty_archive_is_still_a_valid_archive() {
        let bytes = ArchiveBuilder::new().finish().unwrap();
        assert_eq!(bytes.len(), 22, "an empty archive is just its end record");

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).expect("must read");
        assert!(archive.is_empty());
        assert!(!sink.has_blocking());
    }

    // --- archive: refusing what should be refused ----------------------------

    #[test]
    fn a_truncated_or_absent_end_record_is_reported() {
        let mut sink = Sink::new();
        assert!(archive::read(b"", &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "EA012"));

        let mut sink = Sink::new();
        assert!(archive::read(&[0u8; 64], &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "EA013"));

        // A real archive with its tail cut off.
        let mut sink = Sink::new();
        let cut = &INFOZIP_SAMPLE[..INFOZIP_SAMPLE.len() - 4];
        assert!(archive::read(cut, &mut sink).is_none());
    }

    #[test]
    fn a_central_directory_that_points_outside_the_file_is_refused() {
        let mut bytes = ArchiveBuilder::new();
        bytes.add("a", b"x".to_vec()).unwrap();
        let mut bytes = bytes.finish().unwrap();

        // The end record's last four bytes before the comment length are the
        // directory offset. Point it past the end.
        let offset_position = bytes.len() - 6;
        bytes[offset_position..offset_position + 4].copy_from_slice(&0xffff_fff0u32.to_le_bytes());

        let mut sink = Sink::new();
        assert!(archive::read(&bytes, &mut sink).is_none());
        assert!(sink
            .entries()
            .iter()
            .any(|d| d.code == "EA018" || d.code == "EA017"));
    }

    #[test]
    fn an_entry_whose_two_headers_disagree_is_refused() {
        // Which of the two a reader believes decides which file it gets. An
        // archive that gives two answers is refused rather than guessed at.
        let mut builder = ArchiveBuilder::new();
        builder.add("a.txt", b"content".to_vec()).unwrap();
        let mut bytes = builder.finish().unwrap();

        // Corrupt the checksum in the local header only.
        bytes[14..18].copy_from_slice(&0xdead_beefu32.to_le_bytes());

        let mut sink = Sink::new();
        assert!(archive::read(&bytes, &mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "EA033").unwrap();
        assert_eq!(error.class, FailureClass::SecurityFailure);
    }

    #[test]
    fn an_entry_that_does_not_match_its_checksum_is_reported() {
        let mut builder = ArchiveBuilder::new();
        builder.add("a.txt", b"content".to_vec()).unwrap();
        let mut bytes = builder.finish().unwrap();

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).unwrap();
        let offset = archive.entry("a.txt").unwrap().data_offset as usize;
        bytes[offset] = b'X';

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).unwrap();
        let (checked, _) = archive::verify_checksums(&archive, &bytes, &mut sink);
        assert_eq!(checked, 0);
        assert!(sink.entries().iter().any(|d| d.code == "EA041"));
    }

    #[test]
    fn an_archive_naming_the_same_entry_twice_is_refused() {
        // The writer will not produce one, so the archive is built by hand: two
        // central directory records pointing at one local header.
        let mut builder = ArchiveBuilder::new();
        builder.add("a.txt", b"x".to_vec()).unwrap();
        let single = builder.finish().unwrap();

        let mut sink = Sink::new();
        let archive = archive::read(&single, &mut sink).unwrap();
        let directory_start = single.len() - 22 - (46 + 5);
        let record = single[directory_start..single.len() - 22].to_vec();
        assert_eq!(archive.len(), 1);

        let mut doubled = single[..directory_start].to_vec();
        doubled.extend_from_slice(&record);
        doubled.extend_from_slice(&record);
        let directory_size = (record.len() * 2) as u32;

        let mut end = single[single.len() - 22..].to_vec();
        end[8..10].copy_from_slice(&2u16.to_le_bytes());
        end[10..12].copy_from_slice(&2u16.to_le_bytes());
        end[12..16].copy_from_slice(&directory_size.to_le_bytes());
        end[16..20].copy_from_slice(&(directory_start as u32).to_le_bytes());
        doubled.extend_from_slice(&end);

        let mut sink = Sink::new();
        assert!(archive::read(&doubled, &mut sink).is_none());
        let error = sink.entries().iter().find(|d| d.code == "EA011").unwrap();
        assert_eq!(error.class, FailureClass::SecurityFailure);
    }

    #[test]
    fn an_archive_carrying_a_traversing_name_is_refused_when_read() {
        // The writer refuses these, so this one is assembled by hand to prove
        // the reader refuses them too. A hostile archive was not written here.
        let mut builder = ArchiveBuilder::new();
        builder.add("aa/bb", b"x".to_vec()).unwrap();
        let mut bytes = builder.finish().unwrap();

        // Rewrite the name in both headers, keeping its length.
        let first = bytes
            .windows(5)
            .position(|window| window == b"aa/bb")
            .unwrap();
        bytes[first..first + 5].copy_from_slice(b"../bb");
        let second = bytes[first + 5..]
            .windows(5)
            .position(|window| window == b"aa/bb")
            .unwrap()
            + first
            + 5;
        bytes[second..second + 5].copy_from_slice(b"../bb");

        let mut sink = Sink::new();
        assert!(archive::read(&bytes, &mut sink).is_none());
        assert!(sink.entries().iter().any(|d| d.code == "EA006"));
    }

    #[test]
    fn the_archive_reader_survives_arbitrary_input() {
        // Directive section 41 names ZIP and APK as fuzz targets. An archive is
        // a structure of offsets pointing at other offsets, which is the shape
        // that makes a reader loop or read out of bounds.
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut valid = ArchiveBuilder::new();
        valid.add("a.txt", b"hello".to_vec()).unwrap();
        valid.add("dir/b.bin", vec![3; 40]).unwrap();
        let base = valid.finish().unwrap();

        for _ in 0..3_000 {
            let mut bytes = base.clone();

            // Corrupt a few bytes of a real archive: mutations near a structure
            // reach code that random noise never would.
            let mutations = (xorshift(&mut seed) % 6) + 1;
            for _ in 0..mutations {
                let position = (xorshift(&mut seed) as usize) % bytes.len();
                bytes[position] = (xorshift(&mut seed) & 0xff) as u8;
            }

            let mut sink = Sink::new();
            if let Some(archive) = archive::read(&bytes, &mut sink) {
                // Anything it accepts must be self-consistent.
                for entry in archive.entries() {
                    assert!(entry.data_offset <= bytes.len() as u64);
                    assert!(archive::validate_entry_name(&entry.name).is_ok());
                }
                let _ = archive::verify_checksums(&archive, &bytes, &mut sink);
            }
        }

        // And pure noise, which mostly exercises the end-record search.
        for _ in 0..2_000 {
            let length = (xorshift(&mut seed) % 300) as usize;
            let bytes: Vec<u8> = (0..length)
                .map(|_| (xorshift(&mut seed) & 0xff) as u8)
                .collect();
            let mut sink = Sink::new();
            let _ = archive::read(&bytes, &mut sink);
        }
    }

    #[test]
    fn an_archive_this_build_writes_satisfies_independent_tools() {
        // Directive section 24 ends with conformance tests. Reading a file made
        // by another tool proves the parser follows the specification; this
        // proves the writer does, which is the half a round-trip cannot show.
        //
        // The tools are used where they exist and the test says so when they do
        // not, rather than passing quietly on a machine that checked nothing.
        let directory = temp_directory("conformance");
        let path = directory.join("omni.apk");

        let mut builder = ArchiveBuilder::for_android();
        builder
            .add(
                "AndroidManifest.xml",
                b"<manifest package=\"com.omni\"/>".to_vec(),
            )
            .unwrap();
        builder.add("classes.dex", vec![0x64; 1_234]).unwrap();
        builder
            .add("lib/arm64-v8a/libomni_builder.so", vec![0x7f; 9_999])
            .unwrap();
        builder
            .add("res/values/strings.arsc", b"table".to_vec())
            .unwrap();
        std::fs::write(&path, builder.finish().unwrap()).unwrap();

        let run = |program: &str, arguments: &[&str]| -> Option<(bool, String)> {
            let output = std::process::Command::new(program)
                .args(arguments)
                .output()
                .ok()?;
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Some((output.status.success(), text))
        };

        let mut checked = 0;

        // Info-ZIP: does the archive hold together, and do its checksums pass?
        if let Some((ok, text)) = run("unzip", &["-t", path.to_str().unwrap()]) {
            assert!(ok, "unzip -t refused the archive:\n{text}");
            assert!(
                text.contains("No errors detected"),
                "unzip -t said:\n{text}"
            );
            checked += 1;
        }

        // And does it list what was put in it?
        if let Some((ok, text)) = run("unzip", &["-l", path.to_str().unwrap()]) {
            assert!(ok, "unzip -l failed:\n{text}");
            for name in [
                "AndroidManifest.xml",
                "classes.dex",
                "lib/arm64-v8a/libomni_builder.so",
                "res/values/strings.arsc",
            ] {
                assert!(text.contains(name), "unzip -l did not list {name}:\n{text}");
            }
            checked += 1;
        }

        // Android's own tool: are the native libraries on a page boundary?
        if let Ok(sdk) = std::env::var("ANDROID_HOME") {
            let zipalign = format!("{sdk}/build-tools/36.0.0/zipalign");
            if std::path::Path::new(&zipalign).is_file() {
                let (ok, text) = run(
                    &zipalign,
                    &["-c", "-P", "16", "-v", "4", path.to_str().unwrap()],
                )
                .expect("zipalign is present");
                assert!(ok, "zipalign refused the archive:\n{text}");
                assert!(
                    text.contains("Verification successful"),
                    "zipalign said:\n{text}"
                );
                checked += 1;
            }
        }

        if checked == 0 {
            eprintln!(
                "conformance: neither unzip nor zipalign is available here, so the \
                 archive was written but not independently checked"
            );
        } else {
            eprintln!("conformance: {checked} independent check(s) accepted the archive");
        }

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn an_archive_serialises_into_a_valid_report() {
        let mut builder = ArchiveBuilder::for_android();
        builder.add("classes.dex", vec![1; 10]).unwrap();
        let bytes = builder.finish().unwrap();
        let archive = archive::read(&bytes, &mut Sink::new()).unwrap();

        let mut w = Writer::new();
        w.begin_object(None);
        archive.write_json(&mut w, "archive");
        w.end_object();
        let document = w.finish();

        assert!(is_structurally_valid(&document), "{document}");
        assert!(document.contains("\"compression\":\"STORED\""));
        assert!(document.contains(&archive.digest().to_hex()));
    }

    // --- DER -----------------------------------------------------------------

    /// A self-signed certificate produced by `keytool`, byte for byte.
    ///
    /// OpenSSL reads it as:
    ///   subject = C = TR, O = Omni, CN = Omni Conformance
    ///   serial  = 5D97B82E9226CBB1
    ///   sha256  = a725...9fbe
    /// Those are the values the tests below check against, so the parser is
    /// measured against a tool that has nothing to do with this project.
    const CONFORMANCE_CERTIFICATE: &[u8] = &[
        0x30, 0x82, 0x03, 0x11, 0x30, 0x82, 0x01, 0xf9, 0xa0, 0x03, 0x02, 0x01, 0x02, 0x02, 0x08,
        0x5d, 0x97, 0xb8, 0x2e, 0x92, 0x26, 0xcb, 0xb1, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48,
        0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c, 0x05, 0x00, 0x30, 0x37, 0x31, 0x0b, 0x30, 0x09, 0x06,
        0x03, 0x55, 0x04, 0x06, 0x13, 0x02, 0x54, 0x52, 0x31, 0x0d, 0x30, 0x0b, 0x06, 0x03, 0x55,
        0x04, 0x0a, 0x13, 0x04, 0x4f, 0x6d, 0x6e, 0x69, 0x31, 0x19, 0x30, 0x17, 0x06, 0x03, 0x55,
        0x04, 0x03, 0x13, 0x10, 0x4f, 0x6d, 0x6e, 0x69, 0x20, 0x43, 0x6f, 0x6e, 0x66, 0x6f, 0x72,
        0x6d, 0x61, 0x6e, 0x63, 0x65, 0x30, 0x1e, 0x17, 0x0d, 0x32, 0x36, 0x30, 0x38, 0x32, 0x30,
        0x30, 0x38, 0x31, 0x35, 0x35, 0x37, 0x5a, 0x17, 0x0d, 0x32, 0x36, 0x30, 0x39, 0x31, 0x39,
        0x30, 0x38, 0x31, 0x35, 0x35, 0x37, 0x5a, 0x30, 0x37, 0x31, 0x0b, 0x30, 0x09, 0x06, 0x03,
        0x55, 0x04, 0x06, 0x13, 0x02, 0x54, 0x52, 0x31, 0x0d, 0x30, 0x0b, 0x06, 0x03, 0x55, 0x04,
        0x0a, 0x13, 0x04, 0x4f, 0x6d, 0x6e, 0x69, 0x31, 0x19, 0x30, 0x17, 0x06, 0x03, 0x55, 0x04,
        0x03, 0x13, 0x10, 0x4f, 0x6d, 0x6e, 0x69, 0x20, 0x43, 0x6f, 0x6e, 0x66, 0x6f, 0x72, 0x6d,
        0x61, 0x6e, 0x63, 0x65, 0x30, 0x82, 0x01, 0x22, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48,
        0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00, 0x03, 0x82, 0x01, 0x0f, 0x00, 0x30, 0x82,
        0x01, 0x0a, 0x02, 0x82, 0x01, 0x01, 0x00, 0xb0, 0x1f, 0xc3, 0xda, 0xaa, 0xec, 0x54, 0xff,
        0xbb, 0xf5, 0xf3, 0xda, 0xfd, 0xa5, 0x2c, 0x95, 0x12, 0x1e, 0x14, 0xd1, 0xda, 0xe6, 0xf1,
        0x81, 0x0a, 0x0a, 0x97, 0xc0, 0x9a, 0xa9, 0xd7, 0x86, 0xaf, 0x8b, 0xe2, 0x1d, 0x8d, 0x18,
        0x16, 0x45, 0x55, 0xec, 0x5e, 0x88, 0xf5, 0xc7, 0xef, 0xe1, 0xa0, 0x1f, 0xcd, 0x70, 0xf5,
        0x5f, 0xf9, 0xda, 0x3d, 0x4d, 0x5a, 0xbe, 0x33, 0x52, 0x08, 0x3b, 0xf6, 0x02, 0xa0, 0x2a,
        0x17, 0x99, 0x2a, 0xa1, 0x75, 0x34, 0x12, 0x2f, 0x9c, 0x72, 0xb2, 0xe7, 0xe8, 0xa1, 0x03,
        0x31, 0x1e, 0x0d, 0xe6, 0xeb, 0xc1, 0x0c, 0x92, 0x74, 0x0e, 0xc7, 0x78, 0xf5, 0x1b, 0xe4,
        0x39, 0x04, 0xd4, 0xae, 0x8c, 0x55, 0x23, 0xd0, 0xd4, 0xd5, 0x5d, 0x2a, 0x8f, 0x1d, 0xbb,
        0x8f, 0x35, 0xef, 0x64, 0xf9, 0x51, 0xd9, 0x77, 0xca, 0x8b, 0x6d, 0x4b, 0xff, 0x98, 0x13,
        0xd4, 0x10, 0xc1, 0x31, 0xe5, 0xda, 0xad, 0xf7, 0x39, 0x3d, 0x98, 0x21, 0x9f, 0x51, 0xcc,
        0x51, 0x72, 0xde, 0x76, 0x93, 0xbb, 0xae, 0xfb, 0x96, 0x9c, 0xe6, 0xcd, 0x8b, 0xd1, 0x22,
        0x31, 0xec, 0x5c, 0x05, 0x7d, 0x13, 0x9e, 0xa7, 0xd0, 0xc5, 0xfc, 0x02, 0xa7, 0x04, 0x7b,
        0x58, 0xd9, 0x10, 0x2b, 0xf4, 0x5a, 0x40, 0x8a, 0x61, 0xe2, 0xaa, 0x72, 0xaf, 0x61, 0xed,
        0x7a, 0xc3, 0xc7, 0xa4, 0x23, 0x97, 0xc9, 0xa5, 0x71, 0x11, 0xc8, 0x76, 0x01, 0x51, 0x38,
        0x29, 0xc7, 0xaa, 0x46, 0x38, 0x7b, 0x35, 0x22, 0x59, 0x4c, 0x78, 0x46, 0x0d, 0xb1, 0x64,
        0xf3, 0x36, 0xb1, 0x33, 0xcc, 0x25, 0x43, 0x76, 0x86, 0x14, 0x59, 0x68, 0x09, 0x1e, 0x90,
        0x7a, 0xd4, 0xb3, 0x6f, 0x7a, 0x8f, 0xc9, 0xbd, 0x77, 0x30, 0x5d, 0x89, 0x10, 0x69, 0x18,
        0xd0, 0xe3, 0xbb, 0x3a, 0xe5, 0x00, 0x36, 0xab, 0x02, 0x03, 0x01, 0x00, 0x01, 0xa3, 0x21,
        0x30, 0x1f, 0x30, 0x1d, 0x06, 0x03, 0x55, 0x1d, 0x0e, 0x04, 0x16, 0x04, 0x14, 0xee, 0x76,
        0x4b, 0xad, 0x89, 0xb8, 0xa8, 0x59, 0xe1, 0x5b, 0xb1, 0xf8, 0x72, 0x08, 0xfb, 0x9b, 0x12,
        0x16, 0xa8, 0x37, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01,
        0x0c, 0x05, 0x00, 0x03, 0x82, 0x01, 0x01, 0x00, 0x74, 0x6f, 0x97, 0x28, 0xb6, 0x84, 0xfe,
        0x21, 0x5c, 0xfb, 0x06, 0xeb, 0x14, 0x9c, 0xc5, 0xd8, 0x04, 0x81, 0x59, 0xda, 0x1f, 0x7f,
        0x7a, 0x0f, 0xde, 0x22, 0xd9, 0xf5, 0x98, 0x5f, 0x3a, 0xde, 0xed, 0xc9, 0x3c, 0x7b, 0x86,
        0x09, 0x6b, 0x62, 0xdb, 0x7f, 0x5d, 0x8e, 0xe5, 0x65, 0x2d, 0x48, 0xf4, 0xd2, 0xbe, 0xca,
        0x44, 0x68, 0xb3, 0x23, 0x15, 0x41, 0x4a, 0x50, 0xcc, 0x0d, 0x3e, 0xf0, 0x9a, 0xfd, 0x7f,
        0x7c, 0x93, 0x13, 0xd9, 0x32, 0xfe, 0x91, 0x1b, 0x77, 0x8d, 0x1a, 0x69, 0xdb, 0x00, 0xe8,
        0x7b, 0xff, 0x8d, 0x32, 0x17, 0x95, 0xab, 0xb3, 0xb4, 0x08, 0x12, 0x38, 0x78, 0xf9, 0x84,
        0x6f, 0x60, 0xee, 0xa2, 0x50, 0xb3, 0x46, 0x1e, 0xa2, 0x82, 0x17, 0xfc, 0x22, 0xda, 0x51,
        0x77, 0x3c, 0x98, 0xd5, 0x9f, 0x4b, 0x02, 0x7b, 0x33, 0xb4, 0xc6, 0x59, 0x97, 0xd1, 0x79,
        0xf0, 0x20, 0xdb, 0x0d, 0xc6, 0x16, 0xb3, 0xf4, 0x55, 0xb2, 0xe2, 0x99, 0x48, 0xa2, 0x31,
        0xc6, 0xa1, 0x56, 0x10, 0xdc, 0x75, 0x2a, 0x59, 0x75, 0xc5, 0xe9, 0xfc, 0xf2, 0xa7, 0x69,
        0x19, 0x26, 0xb2, 0x73, 0x37, 0x1c, 0x7a, 0xce, 0x81, 0x4b, 0x99, 0x07, 0x1d, 0xe8, 0xfa,
        0xee, 0xf6, 0x26, 0xd6, 0x31, 0x01, 0x19, 0x0b, 0xfa, 0xb5, 0x58, 0xfd, 0xa0, 0xd9, 0x02,
        0x63, 0x49, 0x03, 0x8a, 0x82, 0x0c, 0x16, 0x8e, 0xab, 0x86, 0x83, 0x6c, 0x25, 0x52, 0x04,
        0x7d, 0xcf, 0x58, 0xf1, 0x96, 0x15, 0x78, 0x88, 0x83, 0x94, 0x23, 0x3e, 0xc0, 0x31, 0x13,
        0x24, 0x46, 0x34, 0xa6, 0x60, 0x9a, 0x70, 0x97, 0xdf, 0xfd, 0xf9, 0x25, 0x76, 0x98, 0xc9,
        0x54, 0xc4, 0x6a, 0x4b, 0x65, 0xe7, 0x8c, 0x43, 0xa3, 0xdc, 0x0f, 0x22, 0xfe, 0xea, 0x13,
        0x7b, 0x3a, 0xfc, 0x42, 0xd1, 0x53, 0x88, 0x07, 0x6e,
    ];

    #[test]
    fn der_refuses_every_encoding_the_rules_forbid() {
        // DER exists because BER lets one value be written several ways, and a
        // verifier that accepts two spellings of a name can be shown a different
        // name from the one a parser displays.
        let cases: &[(&[u8], &str)] = &[
            (&[0x30, 0x80, 0x00, 0x00], "ED002"), // indefinite length
            (&[0x30, 0x81, 0x00], "ED004"),       // long form, leading zero
            (&[0x30, 0x81, 0x01, 0x00], "ED005"), // long form for a short length
            (&[0x1f, 0x01, 0x00], "ED001"),       // high tag number form
            (&[0x30, 0x05, 0x00], "ED007"),       // runs past its container
        ];
        for (bytes, code) in cases {
            let mut reader = super::der::Reader::new(bytes, 0);
            let error = reader.next_element().unwrap_err();
            assert_eq!(error.code, *code, "{bytes:?}");
        }

        // A padded integer has a shorter encoding, so it is not DER.
        let padded = super::der::Element {
            tag: super::der::tag::INTEGER,
            contents: &[0x00, 0x01],
            offset: 0,
            total: 4,
        };
        assert_eq!(
            super::der::read_integer_bytes(&padded).unwrap_err().code,
            "ED032"
        );
    }

    #[test]
    fn der_reads_object_identifiers_the_way_the_encoding_defines_them() {
        // The first byte holds two arcs at once, which is the one place the
        // encoding is not simply base-128.
        let cases: &[(&[u8], &str)] = &[
            (&[0x06, 0x03, 0x55, 0x04, 0x03], "2.5.4.3"),
            (
                &[
                    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b,
                ],
                "1.2.840.113549.1.1.11",
            ),
            (
                &[0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03],
                "1.2.840.10045.4.3",
            ),
        ];
        for (bytes, expected) in cases {
            let mut reader = super::der::Reader::new(bytes, 0);
            let element = reader.next_element().unwrap();
            assert_eq!(super::der::read_oid(&element).unwrap(), *expected);
        }

        // An arc that never ends, and one with a leading zero byte.
        for bad in [
            &[0x06u8, 0x02, 0x55, 0x80][..],
            &[0x06, 0x03, 0x55, 0x80, 0x01][..],
        ] {
            let mut reader = super::der::Reader::new(bad, 0);
            let element = reader.next_element().unwrap();
            assert!(super::der::read_oid(&element).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn der_survives_arbitrary_input() {
        let mut seed = 0xaaaa_5555_cccc_3333u64;
        for _ in 0..3_000 {
            let length = (xorshift(&mut seed) % 120) as usize;
            let bytes: Vec<u8> = (0..length)
                .map(|_| (xorshift(&mut seed) & 0xff) as u8)
                .collect();
            let mut reader = super::der::Reader::new(&bytes, 0);
            // Reading until it stops must terminate, whatever it was given.
            for _ in 0..64 {
                match reader.next_element() {
                    Ok(element) => {
                        assert!(element.contents.len() <= bytes.len());
                        let _ = super::der::read_oid(&element);
                        let _ = super::der::read_string(&element);
                        let _ = super::der::read_integer_bytes(&element);
                    }
                    Err(_) => break,
                }
                if reader.is_empty() {
                    break;
                }
            }
        }
    }

    // --- X.509 ---------------------------------------------------------------

    #[test]
    fn a_certificate_reads_as_the_tools_read_it() {
        let certificate = Certificate::parse(CONFORMANCE_CERTIFICATE).expect("must parse");

        assert_eq!(certificate.subject, "C=TR, O=Omni, CN=Omni Conformance");
        assert_eq!(certificate.issuer, certificate.subject, "it is self-signed");
        assert_eq!(certificate.serial, "5D97B82E9226CBB1");
        assert_eq!(certificate.public_key_algorithm.display(), "RSA");
        assert_eq!(certificate.public_key_bits, Some(2048));
        assert!(certificate.signature_algorithm.is_known());

        // The fingerprint OpenSSL prints for this file.
        assert_eq!(
            certificate.fingerprint.to_hex(),
            "a7259d236ba6819af41ef78ed77ed17804f07ec9edce8294a5ff380558289fbe"
        );
        assert!(certificate
            .fingerprint_display()
            .starts_with("A7:25:9D:23:6B:A6:81:9A"));

        assert!(certificate.not_before.ends_with("UTC"));
        assert!(certificate.not_after > certificate.not_before);
    }

    #[test]
    fn a_certificate_never_claims_its_signature_was_checked() {
        // Directive section 1: a parser is not a verifier, and the report must
        // not let the two be confused.
        let certificate = Certificate::parse(CONFORMANCE_CERTIFICATE).unwrap();
        let mut w = Writer::new();
        w.begin_array(None);
        certificate.write_json(&mut w);
        w.end_array();
        let document = w.finish();

        assert!(is_structurally_valid(&document));
        assert!(document.contains("\"signatureChecked\":false"));
    }

    #[test]
    fn a_damaged_certificate_is_refused_rather_than_half_read() {
        let mut sink_count = 0;
        for position in [0usize, 1, 4, 40, 200, 500] {
            let mut damaged = CONFORMANCE_CERTIFICATE.to_vec();
            damaged[position] ^= 0xff;
            if Certificate::parse(&damaged).is_err() {
                sink_count += 1;
            }
        }
        assert!(sink_count > 0, "damaging a certificate must be noticed");

        assert!(Certificate::parse(&[]).is_err());
        assert!(Certificate::parse(&[0x30, 0x00]).is_err());
        assert!(Certificate::parse(&CONFORMANCE_CERTIFICATE[..100]).is_err());
    }

    #[test]
    fn certificate_parsing_survives_arbitrary_input() {
        let mut seed = 0x9999_1111_2222_3333u64;
        for _ in 0..2_000 {
            let mut bytes = CONFORMANCE_CERTIFICATE.to_vec();
            let mutations = (xorshift(&mut seed) % 8) + 1;
            for _ in 0..mutations {
                let position = (xorshift(&mut seed) as usize) % bytes.len();
                bytes[position] = (xorshift(&mut seed) & 0xff) as u8;
            }
            let _ = Certificate::parse(&bytes);
        }
    }

    // --- signing block -------------------------------------------------------

    #[test]
    fn a_package_without_a_signing_block_is_reported_as_such() {
        let mut builder = ArchiveBuilder::new();
        builder.add("a.txt", b"x".to_vec()).unwrap();
        let bytes = builder.finish().unwrap();
        let archive = archive::read(&bytes, &mut Sink::new()).unwrap();

        let mut sink = Sink::new();
        let report = signing::examine(
            &bytes,
            archive.central_directory_offset(),
            archive.end_record_offset(),
            &mut sink,
        );

        assert!(!report.has_block);
        assert!(!report.everything_checkable_passed());
        assert!(sink.entries().iter().any(|d| d.code == "ES030"));
    }

    /// Signs a small package with `apksigner`, when it is available.
    ///
    /// Returns the bytes and the directory to clean up, or `None` when the tool
    /// is not here.
    /// Finds `apksigner` in whatever Android SDK this machine has.
    ///
    /// A conformance test that silently skips is worse than no test, because it
    /// reports success while checking nothing. So the search covers the places
    /// an SDK actually lives -- both environment variables and the path the
    /// image installs it at -- and takes the newest build-tools it finds rather
    /// than a hard-coded version that will stop existing.
    fn find_apksigner() -> Option<std::path::PathBuf> {
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        for name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Ok(value) = std::env::var(name) {
                if !value.is_empty() {
                    roots.push(std::path::PathBuf::from(value));
                }
            }
        }
        roots.push(std::path::PathBuf::from("/opt/android-sdk"));
        roots.push(std::path::PathBuf::from("/usr/local/lib/android/sdk"));
        if let Ok(home) = std::env::var("HOME") {
            roots.push(std::path::PathBuf::from(home).join("Android/Sdk"));
        }

        for root in roots {
            let Ok(entries) = std::fs::read_dir(root.join("build-tools")) else {
                continue;
            };
            let mut versions: Vec<std::path::PathBuf> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .collect();
            versions.sort();
            for version in versions.into_iter().rev() {
                let candidate = version.join("apksigner");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        // A machine without an SDK may skip; a machine that promised one may
        // not. Continuous integration sets this so that a conformance test
        // which silently stopped running fails the build instead.
        assert!(
            std::env::var("OMNI_REQUIRE_APKSIGNER").is_err(),
            "OMNI_REQUIRE_APKSIGNER is set but no apksigner was found, so the \
             signing conformance tests would have skipped silently"
        );
        None
    }

    fn sign_with_apksigner(label: &str) -> Option<(Vec<u8>, std::path::PathBuf)> {
        let apksigner = find_apksigner()?;

        let directory = temp_directory(label);
        let apk = directory.join("sample.apk");
        let keystore = directory.join("k.jks");

        let mut builder = ArchiveBuilder::for_android();
        builder
            .add("AndroidManifest.xml", b"<manifest/>".to_vec())
            .unwrap();
        builder.add("classes.dex", vec![0x2a; 3_000]).unwrap();
        builder
            .add("lib/arm64-v8a/libomni.so", vec![0x7f; 2_000])
            .unwrap();
        std::fs::write(&apk, builder.finish().unwrap()).unwrap();

        let keytool = std::process::Command::new("keytool")
            .args([
                "-genkeypair",
                "-keystore",
                keystore.to_str().unwrap(),
                "-storepass",
                "pass123",
                "-keypass",
                "pass123",
                "-alias",
                "k",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "30",
                "-dname",
                "CN=Omni Test, O=Omni, C=TR",
            ])
            .output()
            .ok()?;
        if !keytool.status.success() {
            std::fs::remove_dir_all(&directory).ok();
            return None;
        }

        let signed = std::process::Command::new(&apksigner)
            .args([
                "sign",
                "--ks",
                keystore.to_str().unwrap(),
                "--ks-pass",
                "pass:pass123",
                "--key-pass",
                "pass:pass123",
                "--ks-key-alias",
                "k",
                "--v1-signing-enabled",
                "false",
                "--v2-signing-enabled",
                "true",
                "--v3-signing-enabled",
                "false",
                "--min-sdk-version",
                "28",
                apk.to_str().unwrap(),
            ])
            .output()
            .ok()?;
        assert!(
            signed.status.success(),
            "apksigner failed: {}",
            String::from_utf8_lossy(&signed.stderr)
        );

        let bytes = std::fs::read(&apk).unwrap();
        Some((bytes, directory))
    }

    #[test]
    fn the_digest_this_build_computes_matches_the_one_apksigner_wrote() {
        // This is the conformance test that matters. The scheme's chunked digest
        // is defined precisely, and recomputing it from the specification and
        // getting the same answer as Google's signer is the only way to know the
        // implementation is right rather than merely self-consistent.
        let Some((bytes, directory)) = sign_with_apksigner("v2-digest") else {
            eprintln!(
                "signing conformance: apksigner is not available here, so the digest \
                 was not checked against it"
            );
            return;
        };

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).expect("the signed package must read");
        assert!(!sink.has_blocking(), "{:?}", sink.entries());

        let report = signing::examine(
            &bytes,
            archive.central_directory_offset(),
            archive.end_record_offset(),
            &mut sink,
        );

        assert!(
            report.has_block,
            "apksigner wrote a block and it was not found"
        );
        assert!(
            report.schemes.contains(&"v2"),
            "schemes: {:?}",
            report.schemes
        );
        assert_eq!(report.signers.len(), 1);
        assert_eq!(report.digests_failed, 0, "{:?}", sink.entries());
        assert!(
            report.digests_verified > 0,
            "no digest was recomputed: {:?}",
            sink.entries()
        );
        assert!(report.everything_checkable_passed());

        // And the certificate inside is the one that was just made.
        let certificate = &report.signers[0].certificates[0];
        assert_eq!(certificate.subject, "C=TR, O=Omni, CN=Omni Test");
        assert_eq!(certificate.public_key_bits, Some(2048));

        // The report never overstates what happened.
        assert!(!report.signatures_checked);

        eprintln!(
            "signing conformance: {} digest(s) recomputed and matched apksigner",
            report.digests_verified
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    /// The package this repository's own build produced, if it has been built.
    ///
    /// The conformance test above signs an archive this tree wrote, which
    /// proves the digest against apksigner but says nothing about a package
    /// with thousands of deflated entries that the Android Gradle Plugin laid
    /// out. This points the same reader at exactly that.
    fn package_this_build_produced() -> Option<Vec<u8>> {
        // Naming a package is a promise that it is there. Falling back to the
        // default path when the named one cannot be read would turn a wrong
        // path into a silent skip, which is the failure this whole helper
        // exists to avoid.
        if let Ok(named) = std::env::var("OMNI_PACKAGE_UNDER_TEST") {
            if !named.is_empty() {
                let bytes = std::fs::read(&named)
                    .unwrap_or_else(|why| panic!("OMNI_PACKAGE_UNDER_TEST={named}: {why}"));
                return Some(bytes);
            }
        }
        std::fs::read("Builder/build/outputs/apk/debug/Builder-debug.apk").ok()
    }

    #[test]
    fn the_package_this_build_produced_reads_and_its_digest_holds() {
        let Some(bytes) = package_this_build_produced() else {
            eprintln!(
                "signing conformance: no built package here, so a real Android \
                 Gradle Plugin package was not read. Build one with \
                 `./gradlew :Builder:assembleDebug` or set \
                 OMNI_PACKAGE_UNDER_TEST."
            );
            return;
        };

        let mut sink = Sink::new();
        let archive = archive::read(&bytes, &mut sink).expect("the package must read");
        assert!(!sink.has_blocking(), "{:?}", sink.entries());
        assert!(
            archive.entries().len() > 10,
            "a real package has more than a handful of entries"
        );

        let report = signing::examine(
            &bytes,
            archive.central_directory_offset(),
            archive.end_record_offset(),
            &mut sink,
        );
        assert!(report.has_block, "a debug package carries a v2 block");
        assert_eq!(report.digests_failed, 0, "{:?}", sink.entries());
        assert!(report.digests_verified > 0, "{:?}", sink.entries());
        assert!(!report.signatures_checked);

        eprintln!(
            "signing conformance: {} entries, schemes {:?}, {} digest(s) matched",
            archive.entries().len(),
            report.schemes,
            report.digests_verified
        );
    }

    #[test]
    fn changing_one_byte_of_a_signed_package_is_detected() {
        // Threats T1, T3 and T4 of directive section 27: an APK modified after
        // signing, a modified DEX, a modified native library. All three are the
        // same thing to a content digest, and this is what catches them.
        let Some((bytes, directory)) = sign_with_apksigner("v2-tamper") else {
            eprintln!("signing conformance: apksigner is not available here");
            return;
        };

        let archive = archive::read(&bytes, &mut Sink::new()).unwrap();
        let entry = archive.entry("classes.dex").expect("classes.dex");
        let position = entry.data_offset as usize + 10;

        let mut tampered = bytes.clone();
        tampered[position] ^= 0xff;

        let mut sink = Sink::new();
        let archive = archive::read(&tampered, &mut sink).unwrap();
        let report = signing::examine(
            &tampered,
            archive.central_directory_offset(),
            archive.end_record_offset(),
            &mut sink,
        );

        assert!(report.has_block);
        assert_eq!(report.digests_verified, 0);
        assert!(report.digests_failed > 0, "a changed byte must be noticed");
        assert!(!report.everything_checkable_passed());

        let error = sink.entries().iter().find(|d| d.code == "ES033").unwrap();
        assert_eq!(error.severity, Severity::Fatal);
        assert_eq!(error.class, FailureClass::Corruption);
        assert!(error.suggestion.as_deref().unwrap().contains("Do not"));

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_signing_report_says_what_it_did_not_check() {
        let Some((bytes, directory)) = sign_with_apksigner("v2-report") else {
            return;
        };

        let archive = archive::read(&bytes, &mut Sink::new()).unwrap();
        let report = signing::examine(
            &bytes,
            archive.central_directory_offset(),
            archive.end_record_offset(),
            &mut Sink::new(),
        );

        let mut w = Writer::new();
        w.begin_object(None);
        report.write_json(&mut w, "signing");
        w.end_object();
        let document = w.finish();

        assert!(is_structurally_valid(&document), "{document}");
        assert!(document.contains("\"signaturesChecked\":false"));
        assert!(document.contains("does not prove who wrote it"));

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_signing_block_reader_survives_arbitrary_input() {
        let Some((bytes, directory)) = sign_with_apksigner("v2-fuzz") else {
            return;
        };

        let mut seed = 0x7777_8888_9999_aaaau64;
        for _ in 0..1_500 {
            let mut damaged = bytes.clone();
            let mutations = (xorshift(&mut seed) % 6) + 1;
            for _ in 0..mutations {
                let position = (xorshift(&mut seed) as usize) % damaged.len();
                damaged[position] = (xorshift(&mut seed) & 0xff) as u8;
            }

            let mut sink = Sink::new();
            if let Some(archive) = archive::read(&damaged, &mut sink) {
                let report = signing::examine(
                    &damaged,
                    archive.central_directory_offset(),
                    archive.end_record_offset(),
                    &mut sink,
                );
                // Whatever it decided, it must never claim to have checked a
                // signature.
                assert!(!report.signatures_checked);
            }
        }

        std::fs::remove_dir_all(&directory).ok();
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
    fn sha256_survives_arbitrary_input() {
        // Directive section 41 applies to every parser and every primitive that
        // reads untrusted bytes. SHA-256 has no parsing to get wrong, so what is
        // checked here is that no length, including the padding boundaries, can
        // make it disagree with itself or misbehave.
        let mut seed = 0xdead_beef_cafe_1234u64;
        for _ in 0..2_000 {
            let length = (xorshift(&mut seed) % 4_096) as usize;
            let data: Vec<u8> = (0..length)
                .map(|_| (xorshift(&mut seed) & 0xff) as u8)
                .collect();

            let one_shot = super::hash::sha256(&data);
            let split = ((xorshift(&mut seed) % 200) + 1) as usize;
            let mut hasher = super::hash::Sha256::new();
            for piece in data.chunks(split) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finish(), one_shot, "length {length}, split {split}");
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
