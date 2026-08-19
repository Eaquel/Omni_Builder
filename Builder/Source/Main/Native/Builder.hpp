// Omni_Builder — JNI bridge, declarations
//
// Contract (directive section 2)
// -----------------------------------------------------------------------------
// Module              : builder.native.bridge
// Purpose             : Carry calls from the Kotlin user interface to the Omni
//                       Core and carry results back.
// Scope               : JNI marshalling and lifetime management. Nothing else.
// Non-Responsibilities: Any build logic, any policy decision, any interpretation
//                       of what the Core returns. This file must stay thin; if it
//                       starts deciding things, the decision belongs in the Core.
// Inputs              : Java strings from com.omni.builder.Builder.
// Outputs             : Java strings, or a Java exception.
// Security            : Never logs the payloads it carries. Refuses to load
//                       against a Core built for a different ABI (ADR-0004).
// Failure Modes       : Core returns null -> a Java exception is thrown with a
//                       precise reason. The bridge never invents a result.
// Determinism         : Pure pass-through; adds no state.
// Status              : FOUNDATION
//
// See ADR-0004 in Omni.rs for why the JNI layer is C++ and the Core exposes a
// plain C ABI.

#ifndef OMNI_BUILDER_NATIVE_BUILDER_HPP
#define OMNI_BUILDER_NATIVE_BUILDER_HPP

#include <jni.h>

#include <cstdint>

// -----------------------------------------------------------------------------
// The C ABI exported by the Rust Core (see the `ffi` module in Omni.rs).
//
// These declarations must stay in step with that module. The ABI version check
// in JNI_OnLoad is what turns a mismatch into a clean refusal instead of
// undefined behaviour.
// -----------------------------------------------------------------------------
extern "C" {

/// Returns the ABI version the linked Core was built with.
uint32_t omni_abi_version(void);

/// Returns the Core version. Static lifetime; must not be freed.
const char *omni_core_version(void);

/// Builds the Core state report as JSON. May return null on failure.
/// A non-null result is owned by the caller and must be released with
/// omni_string_free.
char *omni_state_report(const char *observed_environment);

/// Releases a string returned by omni_state_report. Accepts null.
void omni_string_free(char *value);

}  // extern "C"

/// ABI version this bridge was compiled against.
///
/// JNI_OnLoad compares this with omni_abi_version() and refuses to load the
/// library if they differ, rather than calling into a Core it does not
/// understand (directive section 65).
constexpr uint32_t kOmniExpectedAbiVersion = 1;

#endif  // OMNI_BUILDER_NATIVE_BUILDER_HPP
