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
pub const CORE_PHASE: &str = "PHASE 1 — MOBILE BOOTSTRAP";

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
    /// `execute` returns a whole [`Diagnostic`] by value rather than a boxed one.
    /// The type is around 200 bytes, which Clippy flags, but the failure path is
    /// not a hot path and the diagnostic *is* the result the caller needs; boxing
    /// would add an allocation to every failure in exchange for nothing
    /// measurable. Revisit only if profiling says otherwise (directive section 10).
    #[allow(clippy::result_large_err)]
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
            note: "Compiles the builder user interface. UNRESOLVED BOOTSTRAP GAP: \
                   since AGP 9.0 the Kotlin version is chosen by the Android \
                   Gradle Plugin, which exposes no supported way to select it; \
                   AGP 9.3.0 delivers 2.2.10, not the pinned 2.4.10. The \
                   `verifyKotlinToolchain` Gradle task measures and reports the \
                   real version rather than letting the difference pass silently.",
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
            pinned: "4",
            requirement: Requirement::Series,
            source: "Android SDK package cmake;4.x",
            checksum: None,
            observable_on_device: false,
            note: "Directive section 14 pins a series, not a point release.",
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
    use super::caps::{Capability, Decision, Policy};
    use super::diag::{Diagnostic, Location, Severity, Sink};
    use super::json::Writer;
    use super::plugin::{Registry, Version};
    use super::toolchain::{self, Observation, Requirement, State};
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
