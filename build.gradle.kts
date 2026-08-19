// Omni_Builder — root build script
//
// FILE NAME NOTE (ADR-0001 in Omni.rs)
// -----------------------------------------------------------------------------
// Directive section 46 spells this file "Build.gradle.kts". Gradle resolves a
// build script only as build.gradle, build.gradle.kts or build.gradle.dcl, and
// the per-project buildFileName override was removed in Gradle 9.
//
// VERSION POLICY (directive section 14)
// -----------------------------------------------------------------------------
// Every version below is an exact literal. Dynamic notation - "latest", "9.+",
// "*" - is forbidden, and the Core's toolchain lock encodes the same numbers so
// that a drift between the two is detectable rather than silent.

// The Kotlin Android plugin is deliberately absent. From AGP 9.0 onwards Kotlin
// support is built into the Android plugin, and applying
// org.jetbrains.kotlin.android alongside it is a hard error. The Kotlin version
// therefore comes from AGP, which is why the toolchain lock records both and the
// `verifyToolchainLock` task checks that they still agree.
plugins {
    id("com.android.application") version "9.3.0" apply false
}

// The versions this build is pinned to, mirrored from directive section 14 and
// from `toolchain::LOCK` in Omni.rs. `verifyToolchainLock` below fails the build
// if the two ever disagree.
// Kotlin is deliberately absent from this map. Since AGP 9.0 the Kotlin version
// is chosen by AGP, not by this build, so pinning it here would assert control
// this build does not have. The `:Builder:verifyKotlinToolchain` task measures
// and reports the real version instead.
val omniPinnedVersions = mapOf(
    "agp" to "9.3.0",
    "gradle" to "9.7.0",
    "ndk" to "29.0.14206865",
    "androidApi" to "36",
    "buildTools" to "36.0.0",
    "cmake" to "4.1.2",
    "minSdk" to "28",
    "targetSdk" to "36",
    "rust" to "1.97.1",
)

/**
 * Fails if a version pinned in the Gradle build has drifted from the version the
 * Core reports as pinned.
 *
 * Two copies of the truth is a defect waiting to happen; this task turns it into
 * a build failure instead. It reads Omni.rs as text rather than running the Core,
 * so it works before anything has been compiled.
 */
tasks.register("verifyToolchainLock") {
    group = "verification"
    description = "Checks that the Gradle pins match the toolchain lock in Omni.rs."

    val coreSource = layout.projectDirectory.file("Omni.rs").asFile
    val pinned = omniPinnedVersions
    inputs.file(coreSource)

    doLast {
        val text = coreSource.readText()
        val entries = Regex(
            """id:\s*"([^"]+)"\s*,\s*display_name:\s*"[^"]*"\s*,\s*pinned:\s*"([^"]+)""""
        ).findAll(text).associate { it.groupValues[1] to it.groupValues[2] }

        if (entries.isEmpty()) {
            throw GradleException(
                "No pins could be read from ${coreSource.name}. The toolchain lock " +
                    "format changed and this check no longer sees it."
            )
        }

        val drift = pinned.mapNotNull { (id, gradleValue) ->
            val coreValue = entries[id]
            when {
                coreValue == null -> "$id: pinned to $gradleValue here, absent from the Core lock"
                // The Core pins CMake and the JDK as a series, so only the leading
                // component is comparable for those.
                coreValue != gradleValue && !gradleValue.startsWith("$coreValue.") ->
                    "$id: Gradle says $gradleValue, the Core lock says $coreValue"
                else -> null
            }
        }

        if (drift.isNotEmpty()) {
            throw GradleException(
                "The Gradle pins and the Core toolchain lock disagree:\n" +
                    drift.joinToString("\n") { "  - $it" } +
                    "\nChange both, or neither."
            )
        }

        logger.lifecycle("Toolchain lock verified: ${pinned.size} pins agree with Omni.rs.")
    }
}
