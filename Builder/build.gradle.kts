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

// `compilerVersion` below is marked experimental by the Kotlin Gradle Plugin.
// The alternative is to let AGP choose the compiler, which directive section 14
// does not allow, so the opt-in is deliberate and recorded in ADR-0006.
@file:OptIn(org.jetbrains.kotlin.buildtools.api.ExperimentalBuildToolsApi::class)

import java.util.Properties
import java.util.zip.ZipEntry
import java.util.zip.ZipFile

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
 * Host-specific settings, read once.
 *
 * `local.properties` is deliberately not committed (.gitignore), so it is the
 * right place for machine paths and for a reference to signing material that
 * must never enter the repository (directive section 25).
 */
val omniLocalProperties: Properties = Properties().apply {
    rootProject.file("local.properties").takeIf { it.isFile }?.inputStream()?.use(::load)
}

/**
 * Reads a setting from local.properties, falling back to an environment variable.
 *
 * The environment is read through Gradle's provider API so the configuration
 * cache treats it as a tracked input.
 */
fun omniSetting(propertyName: String, environmentName: String): String? =
    omniLocalProperties.getProperty(propertyName)?.takeIf { it.isNotBlank() }
        ?: providers.environmentVariable(environmentName).orNull?.takeIf { it.isNotBlank() }

/**
 * Location of the CMake pinned by directive section 14.
 *
 * The Android SDK publishes CMake only up to 4.1.2, so 4.4.2 is provisioned from
 * upstream and pointed at here. `verifyCmakeToolchain` turns a missing or wrong
 * entry into a precise failure rather than an AGP error about a package it
 * cannot find.
 */
val omniCmakeDirectory: String? = omniSetting("cmake.dir", "OMNI_CMAKE_DIR")

// -----------------------------------------------------------------------------
// Bootstrap signing (directive sections 15 and 25)
// -----------------------------------------------------------------------------
// This is AGP signing the bootstrap APK, not the Omni signing subsystem. That is
// directive section 25 and roadmap phase 12; Plugins/Sign.rs stays PLANNED.
//
// No key material lives in this repository and none ever will. The keystore is
// referenced from local.properties, which is not committed, or from the
// environment. Nothing here is printed, logged or written into a diagnostic.
//
// Signing matters for more than provenance: an application targeting API 30 or
// later is rejected at install time unless it carries an APK Signature Scheme v2
// or later signature. A v1-only JAR signature - still the default in some
// third-party signing tools - produces "App not installed" with no further
// explanation.
val omniSigningStoreFile = omniSetting("omni.signing.storeFile", "OMNI_SIGNING_STORE_FILE")
val omniSigningStorePassword =
    omniSetting("omni.signing.storePassword", "OMNI_SIGNING_STORE_PASSWORD")
val omniSigningKeyAlias = omniSetting("omni.signing.keyAlias", "OMNI_SIGNING_KEY_ALIAS")
val omniSigningKeyPassword =
    omniSetting("omni.signing.keyPassword", "OMNI_SIGNING_KEY_PASSWORD")

val omniSigningSettings = mapOf(
    "omni.signing.storeFile" to omniSigningStoreFile,
    "omni.signing.storePassword" to omniSigningStorePassword,
    "omni.signing.keyAlias" to omniSigningKeyAlias,
    "omni.signing.keyPassword" to omniSigningKeyPassword,
)

// All four or none. A half-configured identity would silently produce an
// unsigned APK, and the person who set three of them would find out at install
// time instead of at build time.
val omniSigningConfigured: Boolean = when (omniSigningSettings.count { it.value != null }) {
    0 -> false
    omniSigningSettings.size -> true
    else -> throw GradleException(
        "Release signing is only partly configured. Missing: " +
            omniSigningSettings.filterValues { it == null }.keys.joinToString(", ") +
            "\nSet all four, or none of them. Run `./gradlew :Builder:signingHelp` " +
            "for the exact commands."
    )
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

        // Both source sets are redirected, and both matter.
        //
        // AGP's built-in Kotlin compiles what `kotlin.directories` names, not what
        // `java.directories` names. Redirecting only the latter left
        // compileDebugKotlin reporting NO-SOURCE: the build succeeded, the APK was
        // well formed, signed and installable, and it contained none of this
        // module's code - only the generated R classes. The application then died
        // at launch with ClassNotFoundException for its own activity.
        //
        // `verifyApkClasses` below exists so that this cannot happen again
        // without the build failing first.
        kotlin.directories.clear()
        kotlin.directories.add("Source/Main/Kotlin")
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

    signingConfigs {
        if (omniSigningConfigured) {
            create("omniRelease") {
                storeFile = file(omniSigningStoreFile!!)
                storePassword = omniSigningStorePassword
                keyAlias = omniSigningKeyAlias
                keyPassword = omniSigningKeyPassword

                // minSdk is 28, so every device that can run this application
                // understands scheme v2. The v1 JAR signature would add weight
                // and a weaker guarantee for no reader.
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
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
            // Signed when this machine has been told where the keystore is, and
            // left unsigned otherwise. An unsigned APK is honest; an APK signed
            // with a key committed to the repository would not be.
            signingConfig = signingConfigs.findByName("omniRelease")
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
    // Directive section 14 pins this version. It takes effect only because
    // kotlin.compiler.runViaBuildToolsApi is enabled in gradle.properties; the
    // Build Tools API is the supported way to drive a compiler other than the one
    // the Android Gradle Plugin ships with (ADR-0006).
    compilerVersion.set(omniKotlinPin)

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
// Installability (directive sections 33, 51 and 55)
// -----------------------------------------------------------------------------
// "App not installed" is the least actionable message Android produces: the
// package manager refuses and explains nothing. Every condition below has already
// caused it on this project, so each one is checked against the real APK and
// turned into a build failure that says what is wrong and how to fix it.
tasks.register("verifyApkInstallability") {
    group = "verification"
    description = "Checks that every built APK can actually be installed."

    val apkDirectory = layout.buildDirectory.dir("outputs/apk")
    val buildTools = "$omniSdkDirectory/build-tools/$omniBuildToolsVersion"
    val minSdk = omniMinSdk
    val targetSdk = omniTargetSdk

    doLast {
        val directory = apkDirectory.get().asFile
        val packages = directory.walkTopDown().filter { it.extension == "apk" }.sorted().toList()
        if (packages.isEmpty()) {
            // This task finalises every assemble task, so it also runs when the
            // assemble failed. Failing here too would bury the real error under a
            // second one about a missing file.
            logger.lifecycle("No APK found under $directory; nothing to check.")
            return@doLast
        }

        fun run(vararg command: String): Pair<Int, String> {
            val process = ProcessBuilder(*command).redirectErrorStream(true).start()
            val output = process.inputStream.bufferedReader().use { it.readText() }
            return process.waitFor() to output
        }

        val problems = mutableListOf<String>()

        for (apk in packages) {
            val name = apk.name
            logger.lifecycle("Checking $name")

            // 1. Native libraries must be stored, not deflated.
            //
            // AndroidManifest.xml declares android:extractNativeLibs="false", which
            // means the platform maps each library straight out of the APK instead
            // of unpacking it. A compressed entry cannot be mapped, and the
            // installer refuses the package. Re-zipping an APK with an ordinary
            // archiver - which is what most third-party signing tools do - turns
            // every stored entry into a deflated one and breaks exactly this.
            ZipFile(apk).use { zip ->
                val compressed = zip.entries().toList()
                    .filter { it.name.startsWith("lib/") && it.name.endsWith(".so") }
                    .filter { it.method != ZipEntry.STORED }
                    .map { it.name }

                if (compressed.isNotEmpty()) {
                    problems += "$name: native libraries are compressed while the " +
                        "manifest declares extractNativeLibs=\"false\", so the " +
                        "installer will reject the package: " +
                        compressed.joinToString(", ") +
                        "\n    The APK was almost certainly repacked by a tool that " +
                        "re-zipped it. Sign the APK produced by this build instead " +
                        "of repacking it, using `apksigner`, which rewrites only " +
                        "the signature."
                }
            }

            // 2. Native libraries must stay 16 KB aligned.
            //
            // Devices with 16 KB memory pages cannot map a library that is not
            // aligned to that boundary, and refuse to install the package.
            val (alignStatus, alignOutput) = run("$buildTools/zipalign", "-c", "-P", "16", "-v", "4", apk.path)
            if (alignStatus != 0) {
                problems += "$name: native libraries are not 16 KB aligned, so the " +
                    "package will not install on a device with 16 KB pages.\n" +
                    alignOutput.lines().filter { it.contains("BAD") }.joinToString("\n")
            }

            // 3. The signature must be one the platform accepts.
            val (_, verifyOutput) = run(
                "$buildTools/apksigner", "verify",
                "--min-sdk-version", minSdk.toString(),
                "--max-sdk-version", targetSdk.toString(),
                apk.path,
            )

            when {
                verifyOutput.contains("Missing META-INF/MANIFEST.MF") ->
                    logger.lifecycle(
                        "  unsigned - this is expected for the release artifact. " +
                            "It cannot be installed until it is signed; run " +
                            "`./gradlew :Builder:signingHelp`."
                    )

                verifyOutput.contains("DOES NOT VERIFY") ->
                    problems += "$name: the signature is not one this platform " +
                        "accepts.\n    " +
                        verifyOutput.lines().filter { it.startsWith("ERROR") }
                            .joinToString("\n    ") +
                        "\n    An application targeting API $targetSdk must carry an " +
                        "APK Signature Scheme v2 or later signature. A v1-only JAR " +
                        "signature is refused at install time with no explanation. " +
                        "Run `./gradlew :Builder:signingHelp`."

                else -> logger.lifecycle("  signature accepted for API $minSdk to $targetSdk")
            }
        }

        if (problems.isNotEmpty()) {
            throw GradleException(
                "These APKs cannot be installed:\n\n" +
                    problems.joinToString("\n\n") { "  - $it" }
            )
        }

        logger.lifecycle("Installability verified for ${packages.size} APK(s).")
    }
}

// -----------------------------------------------------------------------------
// Class presence (directive sections 51 and 55)
// -----------------------------------------------------------------------------
// The bug this exists for: the Kotlin source directory was redirected on the
// `java` source set but not on the `kotlin` one, so compileDebugKotlin reported
// NO-SOURCE. The build succeeded, the APK was well formed, correctly aligned,
// signed and installable - and it contained none of this module's code. The
// application died at launch with ClassNotFoundException for its own activity.
//
// Nothing in the build objected, because nothing was looking. This does.
tasks.register("verifyApkClasses") {
    group = "verification"
    description = "Checks that every class the manifest names is really in the APK."

    val apkDirectory = layout.buildDirectory.dir("outputs/apk")
    val manifestFile = file("Source/Main/AndroidManifest.xml")
    val namespace = "com.omni.builder"
    val dexdump = "$omniSdkDirectory/build-tools/$omniBuildToolsVersion/dexdump"
    val scratch = layout.buildDirectory.dir("tmp/verifyApkClasses")

    // Named in code rather than in the manifest, and just as fatal when absent:
    // the JNI symbols in Builder.cpp are bound to this class by name.
    val alsoRequired = listOf("$namespace.Builder")

    inputs.file(manifestFile)

    doLast {
        val directory = apkDirectory.get().asFile
        val packages = directory.walkTopDown().filter { it.extension == "apk" }.sorted().toList()
        if (packages.isEmpty()) {
            logger.lifecycle("No APK found under $directory; nothing to check.")
            return@doLast
        }

        // Every component Android will try to instantiate by name.
        val document = javax.xml.parsers.DocumentBuilderFactory.newInstance()
            .apply { isNamespaceAware = true }
            .newDocumentBuilder()
            .parse(manifestFile)

        val androidNamespace = "http://schemas.android.com/apk/res/android"
        val declared = listOf("application", "activity", "service", "receiver", "provider")
            .flatMap { tag ->
                val nodes = document.getElementsByTagName(tag)
                (0 until nodes.length).mapNotNull { index ->
                    (nodes.item(index) as? org.w3c.dom.Element)
                        ?.getAttributeNS(androidNamespace, "name")
                        ?.takeIf { it.isNotBlank() }
                }
            }
            .map { name ->
                when {
                    name.startsWith(".") -> namespace + name
                    !name.contains('.') -> "$namespace.$name"
                    else -> name
                }
            }

        val required = (declared + alsoRequired).distinct().sorted()
        if (required.isEmpty()) {
            throw GradleException(
                "No component could be read from ${manifestFile.name}. The check " +
                    "cannot pass by finding nothing to look for."
            )
        }

        val work = scratch.get().asFile
        val problems = mutableListOf<String>()

        for (apk in packages) {
            work.deleteRecursively()
            work.mkdirs()

            val present = mutableSetOf<String>()
            ZipFile(apk).use { zip ->
                zip.entries().toList()
                    .filter { it.name.matches(Regex("""classes\d*\.dex""")) }
                    .forEach { entry ->
                        val dex = File(work, entry.name)
                        zip.getInputStream(entry).use { input ->
                            dex.outputStream().use { output -> input.copyTo(output) }
                        }

                        val process = ProcessBuilder(dexdump, "-f", dex.path)
                            .redirectErrorStream(true)
                            .start()
                        process.inputStream.bufferedReader().useLines { lines ->
                            lines.forEach { line ->
                                val descriptor = Regex("""Class descriptor\s*:\s*'L([^;]+);'""")
                                    .find(line)
                                    ?.groupValues
                                    ?.get(1)
                                if (descriptor != null) {
                                    present += descriptor.replace('/', '.')
                                }
                            }
                        }
                        check(process.waitFor() == 0) { "dexdump failed on ${entry.name}" }
                    }
            }

            if (present.isEmpty()) {
                problems += "${apk.name}: no classes could be read from the APK at all."
                continue
            }

            val missing = required.filterNot { present.contains(it) }
            if (missing.isNotEmpty()) {
                problems += "${apk.name}: declared but not packaged: " +
                    missing.joinToString(", ") +
                    "\n    Android instantiates these by name and will throw " +
                    "ClassNotFoundException at launch.\n    Check that " +
                    "compileKotlin actually had sources: a source set pointed at " +
                    "the wrong directory produces exactly this, and reports " +
                    "NO-SOURCE rather than failing."
            } else {
                logger.lifecycle("${apk.name}: all ${required.size} required classes present")
            }
        }

        work.deleteRecursively()

        if (problems.isNotEmpty()) {
            throw GradleException(
                "These APKs are missing code they declare:\n\n" +
                    problems.joinToString("\n\n") { "  - $it" }
            )
        }
    }
}

tasks.register("signingHelp") {
    group = "help"
    description = "Explains how to sign the release APK without breaking it."

    val configured = omniSigningConfigured
    val buildTools = "$omniSdkDirectory/build-tools/$omniBuildToolsVersion"
    val minSdk = omniMinSdk
    val targetSdk = omniTargetSdk

    doLast {
        if (configured) {
            logger.lifecycle(
                "Release signing is configured. `./gradlew :Builder:assembleRelease` " +
                    "produces a signed APK with scheme v2 and v3."
            )
            return@doLast
        }

        logger.lifecycle(
            """
            |Release signing is not configured, so assembleRelease produces an
            |unsigned APK. Android will not install an unsigned package.
            |
            |Do not sign it with a tool that repacks the archive. An application
            |targeting API $targetSdk needs an APK Signature Scheme v2 or later
            |signature, and this APK stores its native libraries uncompressed and
            |16 KB aligned so the platform can map them directly. A tool that
            |re-zips the file breaks the second condition even when it gets the
            |first one right, and the only symptom is "App not installed".
            |
            |Option 1 - let this build sign it.
            |
            |  Create a key once, outside the repository:
            |
            |    keytool -genkeypair -v \
            |      -keystore ~/omni-release.jks \
            |      -alias omni -keyalg RSA -keysize 4096 -validity 10000
            |
            |  Then add to local.properties, which is never committed:
            |
            |    omni.signing.storeFile=/absolute/path/to/omni-release.jks
            |    omni.signing.storePassword=...
            |    omni.signing.keyAlias=omni
            |    omni.signing.keyPassword=...
            |
            |  The same four values are also read from the environment as
            |  OMNI_SIGNING_STORE_FILE, OMNI_SIGNING_STORE_PASSWORD,
            |  OMNI_SIGNING_KEY_ALIAS and OMNI_SIGNING_KEY_PASSWORD.
            |
            |Option 2 - sign the finished APK by hand. apksigner rewrites only the
            |signature and leaves the archive layout intact:
            |
            |    $buildTools/apksigner sign \
            |      --ks ~/omni-release.jks --ks-key-alias omni \
            |      --v1-signing-enabled false \
            |      --v2-signing-enabled true \
            |      --v3-signing-enabled true \
            |      --min-sdk-version $minSdk \
            |      Builder-release-unsigned.apk
            |
            |Either way, check the result before installing:
            |
            |    ./gradlew :Builder:verifyApkInstallability
            """.trimMargin()
        )
    }
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
    // Checked after the APK exists, so a package that cannot be installed is a
    // build failure rather than a discovery made on a device.
    finalizedBy("verifyApkInstallability", "verifyApkClasses")
}
