// Omni_Builder — Builder application module
//
// FILE NAME NOTE (ADR-0001 in Omni.rs): directive section 46 spells this file
// "Build.gradle.kts". Gradle resolves only "build.gradle.kts".
//
// WHAT THIS MODULE IS (directive section 15)
// -----------------------------------------------------------------------------
// The bootstrap shell: an Android application that loads the Omni Core through
// JNI and shows what the Core reports. It is not the Omni build engine, and the
// Gradle/AGP machinery below is scaffolding that Omni Build is meant to replace.
//
// BUILD ORDER
// -----------------------------------------------------------------------------
//   cargo build --target <abi triple>   ->  target/<triple>/release/libomni_core.a
//   CMake                               ->  libomni_builder.so   (links the archive)
//   AGP                                 ->  Builder-<variant>.apk
//
// The CMake file fails loudly if the archive is missing, so an APK can never be
// produced without the Core actually linked into it.

import java.util.Properties

plugins {
    // Kotlin support comes from AGP itself (AGP 9.0+). Applying
    // org.jetbrains.kotlin.android as well is rejected outright.
    id("com.android.application")
}

// -----------------------------------------------------------------------------
// Pins. These literals are checked against the Core's toolchain lock by the
// root project's `verifyToolchainLock` task (directive section 14).
// -----------------------------------------------------------------------------
val omniNdkVersion = "29.0.14206865"
val omniBuildToolsVersion = "36.0.0"
// Directive section 14 pins "CMake 4.x stable". Without an explicit version AGP
// silently installs and uses its own default (3.22.1), which is exactly the kind
// of unpinned toolchain the directive forbids.
//
// The Android SDK only publishes CMake up to 4.1.2, so 4.4.2 is provisioned from
// upstream Kitware and pointed at through `cmake.dir` in local.properties. The
// `verifyCmakeToolchain` task below refuses to build if that is not in place.
val omniCmakeVersion = "4.4.2"
val omniCompileSdk = 36
val omniMinSdk = 28
val omniTargetSdk = 36

// Android ABI -> Rust target triple. The same mapping appears in
// Source/Main/Native/CMakeLists.txt, where an unknown ABI is a hard error.
//
// 32-bit x86 is deliberately absent: no current device ships it, and every ABI
// in this list costs a full Rust compilation and a native link on every build
// (directive section 36).
val omniAbis = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi",
    "x86_64" to "x86_64-linux-android",
)

// Clang driver prefix per triple. It differs from the Rust triple for 32-bit ARM.
val omniClangPrefix = mapOf(
    "aarch64-linux-android" to "aarch64-linux-android",
    "armv7-linux-androideabi" to "armv7a-linux-androideabi",
    "x86_64-linux-android" to "x86_64-linux-android",
)

val omniRustProfile = "release"
val omniCoreDirectory = rootProject.layout.projectDirectory
val omniRustArtifactDirectory = omniCoreDirectory.dir("target").asFile.absolutePath

/**
 * Locates the Android SDK.
 *
 * Order: ANDROID_HOME, ANDROID_SDK_ROOT, then local.properties. The environment
 * is read through Gradle's provider API so that the configuration cache treats it
 * as a tracked input rather than an untracked side effect. Failing loudly here is
 * better than letting CMake fail later with an empty path segment in the middle
 * of a filename.
 */
val omniSdkDirectory: String = run {
    val fromEnvironment = providers.environmentVariable("ANDROID_HOME")
        .orElse(providers.environmentVariable("ANDROID_SDK_ROOT"))
        .orNull
        ?.takeIf { it.isNotBlank() }

    fromEnvironment ?: run {
        val localProperties = rootProject.file("local.properties")
        val fromFile = if (localProperties.isFile) {
            val properties = Properties()
            localProperties.inputStream().use(properties::load)
            properties.getProperty("sdk.dir")?.takeIf { it.isNotBlank() }
        } else {
            null
        }

        fromFile ?: throw GradleException(
            "The Android SDK could not be located. Set ANDROID_HOME, or put " +
                "sdk.dir in local.properties."
        )
    }
}

/**
 * Reads `cmake.dir` from local.properties, if it is set.
 *
 * Directive section 14 pins CMake 4.4.2, which the Android SDK does not publish
 * (sdkmanager stops at 4.1.2), so the toolchain is provisioned from upstream and
 * pointed at here. `verifyCmakeToolchain` turns a missing or wrong entry into a
 * precise failure rather than an AGP error about a package it cannot find.
 */
val omniCmakeDirectory: String? = rootProject.file("local.properties")
    .takeIf { it.isFile }
    ?.let { file ->
        val properties = Properties()
        file.inputStream().use(properties::load)
        properties.getProperty("cmake.dir")?.takeIf { it.isNotBlank() }
    }

val omniHostTag: String = when {
    org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "darwin-x86_64"
    org.gradle.internal.os.OperatingSystem.current().isWindows -> "windows-x86_64"
    else -> "linux-x86_64"
}

val omniNdkToolchainBin =
    "$omniSdkDirectory/ndk/$omniNdkVersion/toolchains/llvm/prebuilt/$omniHostTag/bin"

// -----------------------------------------------------------------------------
// Rust compilation, one task per ABI.
// -----------------------------------------------------------------------------
val cargoTasks = omniAbis.map { (abi, triple) ->
    // Everything the task body needs is resolved to a plain String here. A task
    // action that reaches back into the build script cannot be serialised by the
    // configuration cache, and a build without the configuration cache is a
    // slower build for every contributor.
    val clangDriver = "$omniNdkToolchainBin/${omniClangPrefix.getValue(triple)}$omniMinSdk-clang"
    val linkerVariable = "CARGO_TARGET_${triple.uppercase().replace('-', '_')}_LINKER"
    val archivePath = "$omniRustArtifactDirectory/$triple/$omniRustProfile/libomni_core.a"
    val coreDirectory = omniCoreDirectory.asFile
    val abiName = abi
    val ndkVersion = omniNdkVersion

    tasks.register<Exec>("cargoBuild${abi.replace("-", "").replaceFirstChar(Char::uppercase)}") {
        group = "build"
        description = "Compiles the Omni Core for $abi ($triple)."

        workingDir = coreDirectory

        // --locked refuses to change Cargo.lock during a build. A build that can
        // silently move a dependency is not reproducible (directive section 12).
        commandLine("cargo", "build", "--locked", "--$omniRustProfile", "--target", triple)

        // Only consulted when a crate type needs a linker. The Core is built as a
        // static archive, so this keeps the environment correct rather than
        // making the current build work.
        environment(linkerVariable, clangDriver)
        environment("TARGET_CC", clangDriver)

        inputs.file(coreDirectory.resolve("Cargo.toml"))
        inputs.file(coreDirectory.resolve("Cargo.lock"))
        inputs.file(coreDirectory.resolve("Omni.rs"))
        inputs.dir(coreDirectory.resolve("Plugins"))
        outputs.file(archivePath)

        doFirst {
            if (!File(clangDriver).isFile) {
                throw GradleException(
                    "The NDK Clang driver for $abiName was not found.\n" +
                        "  Expected: $clangDriver\n" +
                        "  Install NDK $ndkVersion, which the toolchain lock pins."
                )
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Android
// -----------------------------------------------------------------------------
android {
    namespace = "com.omni.builder"
    compileSdk = omniCompileSdk
    buildToolsVersion = omniBuildToolsVersion
    ndkVersion = omniNdkVersion

    defaultConfig {
        applicationId = "com.omni.builder"
        minSdk = omniMinSdk
        targetSdk = omniTargetSdk
        versionCode = 1
        versionName = "0.1.0"

        ndk {
            abiFilters += omniAbis.keys
        }

        externalNativeBuild {
            cmake {
                arguments += listOf(
                    "-DOMNI_RUST_ARTIFACT_DIR=$omniRustArtifactDirectory",
                    "-DOMNI_RUST_PROFILE=$omniRustProfile",
                    "-DANDROID_STL=c++_static",
                )
                cppFlags += listOf("-fno-exceptions", "-fno-rtti")
            }
        }
    }

    // Directive section 46 fixes the source layout, which is not the Gradle
    // default. Every path below points at a directory the directive defines.
    sourceSets.getByName("main") {
        manifest.srcFile("Source/Main/AndroidManifest.xml")
        java.directories.clear()
        java.directories.add("Source/Main/Kotlin")
        res.directories.clear()
        res.directories.add("Source/Main/res")
        // No res/layout directory exists, by design: the report screen is built
        // in code so that it cannot drift from the Core's state shape.
    }

    externalNativeBuild {
        cmake {
            path = file("Source/Main/Native/CMakeLists.txt")
            version = omniCmakeVersion
        }
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
            isJniDebuggable = true
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // Left unsigned on purpose. Signing is directive section 25 and
            // roadmap phase 6; no signing identity exists in this repository, and
            // committing one would be exactly the mistake section 25 warns about.
            signingConfig = null
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            // Native libraries stay compressed inside the APK and are mapped
            // directly, which is also what android:extractNativeLibs="false"
            // in the manifest requires.
            useLegacyPackaging = false
        }
    }

    // The dependency metadata block AGP normally embeds is an opaque, signed
    // blob that changes between builds. Omitting it removes one obstacle to a
    // reproducible APK (directive section 12).
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    buildFeatures {
        buildConfig = false
        viewBinding = false
    }

    lint {
        warningsAsErrors = true
        abortOnError = true
    }
}

kotlin {
    compilerOptions {
        // jvmTarget is intentionally not set: with AGP's built-in Kotlin it
        // defaults to android.compileOptions.targetCompatibility, so setting it
        // here would create a second place for the same value to drift.
        //
        // A warning in this module is a defect. It carries no third-party
        // dependency to blame one on.
        allWarningsAsErrors.set(true)
    }
}

// -----------------------------------------------------------------------------
// Kotlin version drift (directive sections 14 and 64)
// -----------------------------------------------------------------------------
// AGP 9.3.0 ships its own Kotlin and offers no supported way to choose the
// version: the only knob, android.builtInKotlin, turns the whole feature on or
// off, and the AGP 9.3.0 artifacts were inspected to confirm nothing else exists.
// The directive pins Kotlin 2.4.10; AGP currently delivers a different version.
//
// Forcing the pin with a resolution rule would substitute a compiler AGP was
// never tested against, which trades a visible mismatch for an invisible one.
// So the mismatch is measured and reported instead. A major-version drift is
// treated as fatal, because that changes the language; a minor drift is reported
// loudly and left for a human to decide on.
// -----------------------------------------------------------------------------
// CMake provisioning check (directive sections 14 and 31, ADR-0005)
// -----------------------------------------------------------------------------
tasks.register("verifyCmakeToolchain") {
    group = "verification"
    description = "Checks that the pinned CMake is provisioned and reachable."

    val cmakeDirectory = omniCmakeDirectory
    val pinned = omniCmakeVersion

    doLast {
        val instructions =
            "Directive section 14 pins CMake $pinned, which the Android SDK does " +
                "not publish. Provision it from upstream and point the build at it:\n" +
                "  1. Download\n" +
                "     https://github.com/Kitware/CMake/releases/download/v$pinned/" +
                "cmake-$pinned-linux-x86_64.tar.gz\n" +
                "  2. Verify its SHA-256 against the checksum recorded in the\n" +
                "     toolchain lock in Omni.rs. Do not skip this step.\n" +
                "  3. Extract it, and copy a `ninja` binary into its bin directory;\n" +
                "     the Kitware archive does not ship one.\n" +
                "  4. Add `cmake.dir=<that directory>` to local.properties."

        if (cmakeDirectory == null) {
            throw GradleException("`cmake.dir` is not set in local.properties.\n\n$instructions")
        }

        val cmakeBinary = File(cmakeDirectory, "bin/cmake")
        if (!cmakeBinary.isFile) {
            throw GradleException(
                "No CMake executable at ${cmakeBinary.absolutePath}.\n\n$instructions"
            )
        }

        val ninjaBinary = File(cmakeDirectory, "bin/ninja")
        if (!ninjaBinary.isFile) {
            throw GradleException(
                "No Ninja generator at ${ninjaBinary.absolutePath}. The Android " +
                    "build needs one and the Kitware archive does not ship it.\n\n" +
                    instructions
            )
        }

        // ProcessBuilder rather than a Gradle Exec task: this runs inside a task
        // action and must not reach back into the build script, which the
        // configuration cache cannot serialise.
        val process = ProcessBuilder(cmakeBinary.absolutePath, "--version")
            .redirectErrorStream(true)
            .start()
        val output = process.inputStream.bufferedReader().use { it.readText() }
        check(process.waitFor() == 0) { "`cmake --version` failed:\n$output" }

        val reported = Regex("""cmake version (\S+)""").find(output)?.groupValues?.get(1)
            ?: throw GradleException("Could not read a version from:\n$output")

        if (reported != pinned) {
            throw GradleException(
                "CMake version mismatch: the toolchain lock pins $pinned, " +
                    "$cmakeDirectory provides $reported.\n\n$instructions"
            )
        }

        logger.lifecycle("CMake toolchain verified: $reported at $cmakeDirectory.")
    }
}

val omniKotlinPin = "2.4.10"

// AGP 9.3.0 ships its own Kotlin and offers no DSL to choose the version; the
// only knob, android.builtInKotlin, turns the feature on or off. The version is
// therefore pinned where Gradle does have authority: dependency resolution. Every
// org.jetbrains.kotlin module - the compiler AGP runs through the Kotlin Build
// Tools API, and the standard library that ends up in the APK - is forced to the
// pinned version.
//
// The Build Tools API exists precisely so that a build system can drive a Kotlin
// compiler it was not shipped with, so this is the mechanism working as intended
// rather than a version being smuggled past AGP. `verifyKotlinToolchain` proves
// the result instead of assuming it.
configurations.configureEach {
    resolutionStrategy.eachDependency {
        if (requested.group == "org.jetbrains.kotlin") {
            useVersion(omniKotlinPin)
            because("Directive section 14 pins Kotlin $omniKotlinPin.")
        }
    }
}

// The compile classpath only exists once AGP has created its variants, which is
// why this is registered from the variant callback rather than at script level.
androidComponents {
    onVariants(selector().withBuildType("release")) { variant ->
        val resolved = variant.compileConfiguration.incoming.artifacts.resolvedArtifacts
            .map { artifacts ->
                artifacts
                    .mapNotNull { it.variant.owner as? ModuleComponentIdentifier }
                    .firstOrNull { it.module == "kotlin-stdlib" }
                    ?.version
                    ?: "unknown"
            }

        tasks.register("verifyKotlinToolchain") {
            group = "verification"
            description = "Reports how far AGP's built-in Kotlin has drifted from the pin."

            val pinned = omniKotlinPin

            doLast {
                val actual = resolved.get()
                when {
                    actual == pinned ->
                        logger.lifecycle("Kotlin toolchain verified: $actual matches the pin.")

                    actual == "unknown" -> throw GradleException(
                        "The Kotlin version in use could not be determined. The " +
                            "toolchain lock cannot be verified against an unknown " +
                            "compiler."
                    )

                    actual.substringBefore('.') != pinned.substringBefore('.') ->
                        throw GradleException(
                            "Kotlin major version drift: the toolchain lock pins " +
                                "$pinned, AGP delivers $actual. A different major " +
                                "version is a different language; this build will " +
                                "not proceed on it."
                        )

                    else -> logger.warn(
                        "\nTOOLCHAIN DRIFT (directive section 14)\n" +
                            "  Pinned by the toolchain lock : $pinned\n" +
                            "  Delivered by AGP 9.3.0       : $actual\n" +
                            "  AGP 9.3.0 exposes no supported way to select the Kotlin\n" +
                            "  version. This gap is unresolved and is recorded in the\n" +
                            "  Core's toolchain lock in Omni.rs. It is reported, never\n" +
                            "  silently accepted.\n"
                    )
                }
            }
        }
    }
}

// That is deliberate (ADR-0003 in Omni.rs): the JSON parser it uses, org.json,
// is part of Android itself.
dependencies {
}

// -----------------------------------------------------------------------------
// Wiring: CMake must not run before cargo has produced the archive it links.
// Matching by task name avoids depending on an AGP task class that has moved
// between major versions.
// -----------------------------------------------------------------------------
tasks.matching { it.name.startsWith("buildCMake") || it.name.startsWith("configureCMake") }
    .configureEach {
        dependsOn(cargoTasks)
    }

// The APK is only meaningful if every pin holds: the ones this file and the Core
// both declare, the CMake that is provisioned outside the SDK, and the Kotlin
// version AGP would otherwise have chosen for us.
tasks.matching { it.name.startsWith("assemble") }.configureEach {
    dependsOn(":verifyToolchainLock", "verifyCmakeToolchain", "verifyKotlinToolchain")
}
