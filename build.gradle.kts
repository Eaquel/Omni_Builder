plugins {
    id("com.android.application") version "9.3.0" apply false
}

val omniPinnedVersions = mapOf(
    "agp" to "9.3.0",
    "gradle" to "9.7.0",
    "ndk" to "29.0.14206865",
    "androidApi" to "36",
    "buildTools" to "36.0.0",
    "cmake" to "4.4.2",
    "kotlin" to "2.4.10",
    "minSdk" to "28",
    "targetSdk" to "36",
    "rust" to "1.97.1",
)

tasks.register("verifyToolchainLock") {
    group = "verification"
    description = "Checks that the Gradle pins match the toolchain lock in Builder.rs."

    val coreSource = layout.projectDirectory.file("Builder.rs").asFile
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

        logger.lifecycle("Toolchain lock verified: ${pinned.size} pins agree with Builder.rs.")
    }
}
