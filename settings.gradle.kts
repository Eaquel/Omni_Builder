// Omni_Builder — Gradle settings
//
// FILE NAME NOTE (ADR-0001 in Omni.rs)
// -----------------------------------------------------------------------------
// Directive section 46 spells this file "Settings.gradle.kts". Gradle resolves a
// settings file only as settings.gradle, settings.gradle.kts or
// settings.gradle.dcl, and the -c/--settings-file escape hatch was removed in
// Gradle 9. The capitalised name was tested and rejected outright, so the file
// carries the name the tool requires. Nothing else about the layout changed.
//
// BOOTSTRAP NOTE (directive section 15)
// -----------------------------------------------------------------------------
// Gradle is the bootstrap build driver, not the Omni build engine. Everything in
// this file is scaffolding that Omni Build is meant to replace.

pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    // A subproject must not introduce a repository of its own. Every artifact
    // this build consumes has to come from a location declared here, so the
    // supply-chain surface stays visible in one file (directive section 31).
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
    }
}

rootProject.name = "Omni_Builder"

include(":Builder")
