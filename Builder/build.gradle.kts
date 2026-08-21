@file:OptIn(org.jetbrains.kotlin.buildtools.api.ExperimentalBuildToolsApi::class)

import java.util.Properties
import java.util.zip.ZipEntry
import java.util.zip.ZipFile

plugins {
    id("com.android.application")
}

val omniNdkVersion = "29.0.14206865"
val omniBuildToolsVersion = "36.0.0"
val omniCmakeVersion = "4.4.2"
val omniCompileSdk = 36
val omniMinSdk = 28
val omniTargetSdk = 36

val omniAbis = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi",
    "x86_64" to "x86_64-linux-android",
)

val omniClangPrefix = mapOf(
    "aarch64-linux-android" to "aarch64-linux-android",
    "armv7-linux-androideabi" to "armv7a-linux-androideabi",
    "x86_64-linux-android" to "x86_64-linux-android",
)

val omniRustProfile = "release"
val omniCoreDirectory = rootProject.layout.projectDirectory
val omniRustArtifactDirectory = omniCoreDirectory.dir("target").asFile.absolutePath

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

val omniLocalProperties: Properties = Properties().apply {
    rootProject.file("local.properties").takeIf { it.isFile }?.inputStream()?.use(::load)
}

fun omniSetting(propertyName: String, environmentName: String): String? =
    omniLocalProperties.getProperty(propertyName)?.takeIf { it.isNotBlank() }
        ?: providers.environmentVariable(environmentName).orNull?.takeIf { it.isNotBlank() }

val omniCmakeDirectory: String? = omniSetting("cmake.dir", "OMNI_CMAKE_DIR")

val omniSigningStoreFile = omniSetting("omni.signing.storeFile", "OMNI_SIGNING_STORE_FILE")
val omniSigningStorePassword =
    omniSetting("omni.signing.storePassword", "OMNI_SIGNING_STORE_PASSWORD")
val omniSigningKeyAlias = omniSetting("omni.signing.keyAlias", "OMNI_SIGNING_KEY_ALIAS")
val omniExpectedCertificate: String =
    omniSetting("omni.signing.certificateSha256", "OMNI_SIGNING_CERT_SHA256") ?: ""

val omniSigningKeyPassword =
    omniSetting("omni.signing.keyPassword", "OMNI_SIGNING_KEY_PASSWORD")

val omniSigningSettings = mapOf(
    "omni.signing.storeFile" to omniSigningStoreFile,
    "omni.signing.storePassword" to omniSigningStorePassword,
    "omni.signing.keyAlias" to omniSigningKeyAlias,
    "omni.signing.keyPassword" to omniSigningKeyPassword,
)

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

val cargoTasks = omniAbis.map { (abi, triple) ->
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

        commandLine("cargo", "build", "--locked", "--$omniRustProfile", "--target", triple)

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

    sourceSets.getByName("main") {
        manifest.srcFile("Source/Main/AndroidManifest.xml")

        kotlin.directories.clear()
        kotlin.directories.add("Source/Main/Kotlin")
        java.directories.clear()
        java.directories.add("Source/Main/Kotlin")
        res.directories.clear()
        res.directories.add("Source/Main/res")
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

                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    defaultConfig {
        resValue("string", "omni_expected_certificate", omniExpectedCertificate)
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
            signingConfig = signingConfigs.findByName("omniRelease")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
    }

    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    buildFeatures {
        resValues = true
        buildConfig = false
        viewBinding = false
    }

    lint {
        warningsAsErrors = true
        abortOnError = true
    }
}

kotlin {
    compilerVersion.set(omniKotlinPin)

    compilerOptions {
        allWarningsAsErrors.set(true)
    }
}

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

configurations.configureEach {
    resolutionStrategy.eachDependency {
        if (requested.group == "org.jetbrains.kotlin") {
            useVersion(omniKotlinPin)
            because("Directive section 14 pins Kotlin $omniKotlinPin.")
        }
    }
}

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

dependencies {
}

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

            val (alignStatus, alignOutput) = run("$buildTools/zipalign", "-c", "-P", "16", "-v", "4", apk.path)
            if (alignStatus != 0) {
                problems += "$name: native libraries are not 16 KB aligned, so the " +
                    "package will not install on a device with 16 KB pages.\n" +
                    alignOutput.lines().filter { it.contains("BAD") }.joinToString("\n")
            }

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

tasks.register("verifyApkClasses") {
    group = "verification"
    description = "Checks that every class the manifest names is really in the APK."

    val apkDirectory = layout.buildDirectory.dir("outputs/apk")
    val manifestFile = file("Source/Main/AndroidManifest.xml")
    val namespace = "com.omni.builder"
    val dexdump = "$omniSdkDirectory/build-tools/$omniBuildToolsVersion/dexdump"
    val scratch = layout.buildDirectory.dir("tmp/verifyApkClasses")

    val alsoRequired = listOf("$namespace.Builder")

    inputs.file(manifestFile)

    doLast {
        val directory = apkDirectory.get().asFile
        val packages = directory.walkTopDown().filter { it.extension == "apk" }.sorted().toList()
        if (packages.isEmpty()) {
            logger.lifecycle("No APK found under $directory; nothing to check.")
            return@doLast
        }

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

tasks.matching { it.name.startsWith("buildCMake") || it.name.startsWith("configureCMake") }
    .configureEach {
        dependsOn(cargoTasks)
    }

tasks.matching { it.name.startsWith("assemble") }.configureEach {
    dependsOn(":verifyToolchainLock", "verifyCmakeToolchain", "verifyKotlinToolchain")
    finalizedBy("verifyApkInstallability", "verifyApkClasses")
}
