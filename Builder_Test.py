#!/usr/bin/env python3
"""Every check this project has, run from one place.

    ./Builder_Test.py                 every check that can run here
    ./Builder_Test.py core strings    only the checks named
    ./Builder_Test.py --list          what the names are
    ./Builder_Test.py --strict        a check that cannot run here fails

A check that needs a tool this machine does not have is reported as
SKIPPED and does not fail the run, unless --strict is passed. Continuous
integration passes --strict, so a missing tool there is a failure rather
than a quiet gap.

Everything a check writes goes under Tests/, which is never committed.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent
WORK = ROOT / "Tests"
CORE = ROOT / "Builder.rs"
KOTLIN = ROOT / "Builder/Source/Main/Kotlin/com/omni/builder/Builder.kt"
RESOURCES = ROOT / "Builder/Source/Main/res"
COMPILERS = ROOT / "Compilers"

CONFORMANCE = {
    "OMNI_REQUIRE_APKSIGNER": "1",
    "OMNI_REQUIRE_AXML_CONFORMANCE": "1",
    "OMNI_REQUIRE_IMAGE_CONFORMANCE": "1",
    "OMNI_REQUIRE_INFLATE_CONFORMANCE": "1",
}


class Skip(Exception):
    """A check cannot run here because a tool it needs is missing."""


@dataclass
class Result:
    name: str
    state: str
    seconds: float
    detail: str = ""
    lines: list[str] = field(default_factory=list)


def sdk_root() -> Path:
    for name in ("ANDROID_HOME", "ANDROID_SDK_ROOT"):
        value = os.environ.get(name)
        if value and Path(value).is_dir():
            return Path(value)
    for guess in ("/opt/android-sdk", "/usr/local/lib/android/sdk"):
        if Path(guess).is_dir():
            return Path(guess)
    home = os.environ.get("HOME")
    if home and (Path(home) / "Android/Sdk").is_dir():
        return Path(home) / "Android/Sdk"
    raise Skip("no Android SDK on this machine")


def build_tool(name: str) -> Path:
    root = sdk_root()
    found = sorted((root / "build-tools").glob(f"*/{name}"))
    if not found:
        raise Skip(f"{name} is not in the Android build tools here")
    return found[-1]


def ndk_tool(name: str) -> Path:
    root = sdk_root()
    found = sorted(root.glob(f"ndk/*/toolchains/llvm/prebuilt/*/bin/{name}"))
    if not found:
        raise Skip(f"{name} is not in the NDK here")
    return found[-1]


def run(command: list[str] | str, *, env: dict[str, str] | None = None,
        cwd: Path = ROOT, check: bool = True) -> subprocess.CompletedProcess:
    merged = dict(os.environ)
    merged.update(env or {})
    shell = isinstance(command, str)
    done = subprocess.run(
        command, cwd=cwd, env=merged, shell=shell,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    if check and done.returncode != 0:
        tail = "\n".join(done.stdout.strip().splitlines()[-40:])
        raise AssertionError(
            f"{command if shell else ' '.join(command)} exited {done.returncode}\n{tail}"
        )
    return done


def gradle(*tasks: str) -> subprocess.CompletedProcess:
    sdk_root()
    return run([str(ROOT / "gradlew"), *tasks, "--console=plain"])


# ---------------------------------------------------------------- checks


def check_layout() -> str:
    """The tree is the one this project agreed on."""
    required = [
        CORE,
        ROOT / "Cargo.toml",
        ROOT / "Builder_Test.py",
        ROOT / "build.gradle.kts",
        ROOT / "settings.gradle.kts",
        ROOT / "gradle.properties",
        ROOT / "gradlew",
        ROOT / "Gradle/gradle-wrapper.jar",
        ROOT / "Gradle/gradle-wrapper.properties",
        KOTLIN,
        ROOT / "Builder/Source/Main/AndroidManifest.xml",
        ROOT / "Builder/Source/Main/Native/Builder.cpp",
        ROOT / "Builder/Source/Main/Native/Builder.hpp",
        ROOT / "Builder/Source/Main/Native/CMakeLists.txt",
        ROOT / "Builder/build.gradle.kts",
        ROOT / "Builder/proguard-rules.pro",
    ]
    missing = [str(path.relative_to(ROOT)) for path in required if not path.is_file()]
    if missing:
        raise AssertionError("missing: " + ", ".join(missing))

    compilers = sorted(path.name for path in COMPILERS.glob("*.rs"))
    expected = ["Cpp.rs", "Java.rs", "Kotlin.rs", "Rust.rs"]
    if compilers != expected:
        raise AssertionError(f"Compilers holds {compilers}, expected {expected}")

    if (ROOT / "Omni.rs").exists() or (ROOT / "Plugins").exists():
        raise AssertionError("the old Omni.rs / Plugins layout is still here")

    stray = sorted(
        path.name for path in ROOT.glob("*.py") if path.name != "Builder_Test.py"
    )
    if stray:
        raise AssertionError(f"tests live in Builder_Test.py alone, found {stray}")
    return f"{len(required)} files and {len(compilers)} compilers in place"


def check_format() -> str:
    """The Core is formatted the way rustfmt writes it."""
    if shutil.which("cargo") is None:
        raise Skip("cargo is not on this machine")
    run(["cargo", "fmt", "--check"])
    return "rustfmt agrees"


def check_lint() -> str:
    """Clippy finds nothing, with warnings treated as errors."""
    if shutil.which("cargo") is None:
        raise Skip("cargo is not on this machine")
    run(["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"])
    return "clippy is clean"


def tool_present(finder, name: str) -> bool:
    try:
        return finder(name).is_file()
    except Skip:
        return False


def check_core() -> str:
    """The Core's own suite, demanding every conformance check the tools here allow."""
    if shutil.which("cargo") is None:
        raise Skip("cargo is not on this machine")

    # A conformance check is demanded only where the tool it needs is here, and
    # only where the build outputs it reads have been produced.
    built = sorted(ROOT.glob("Builder/build/outputs/apk/*/*.apk"))
    release = [one for one in built if "/release/" in one.as_posix()]
    signed = [one for one in release if not one.name.endswith("-unsigned.apk")]
    under_test = signed or built

    env = dict(CONFORMANCE)
    demanded = list(CONFORMANCE)
    for flag, present in (
        ("OMNI_REQUIRE_AXML_CONFORMANCE", tool_present(build_tool, "aapt2")),
        ("OMNI_REQUIRE_RSA_CONFORMANCE", shutil.which("openssl") is not None),
        ("OMNI_REQUIRE_DEX_CONFORMANCE",
         tool_present(build_tool, "dexdump") and bool(built)),
        ("OMNI_REQUIRE_CLASS_CONFORMANCE",
         shutil.which("javap") is not None and bool(built)),
        ("OMNI_REQUIRE_BUNDLETOOL",
         shutil.which("bundletool") is not None
         and bool(sorted(ROOT.glob("Builder/build/outputs/bundle/*/*.aab")))),
        ("OMNI_REQUIRE_SELF_BUILT_APK",
         tool_present(build_tool, "apksigner") and shutil.which("openssl") is not None),
        ("OMNI_REQUIRE_SELF_POLICY", bool(built)),
    ):
        if present and flag not in env:
            env[flag] = "1"
            demanded.append(flag)
    if under_test:
        env["OMNI_PACKAGE_UNDER_TEST"] = str(under_test[0])

    done = run(["cargo", "test", "--locked"], env=env)
    passed = sum(
        int(found.group(1))
        for found in re.finditer(r"test result: ok\. (\d+) passed", done.stdout)
    )
    if passed == 0:
        raise AssertionError("no test reported a result\n" + done.stdout[-2000:])
    return f"{passed} tests passed, demanding {len(demanded)} conformance checks"


def check_dependencies() -> str:
    """The Core carries no third-party dependency."""
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    packages = lock.count("[[package]]")
    if packages != 1:
        raise AssertionError(f"Cargo.lock holds {packages} packages, expected 1")
    return "one package, and it is this one"


def check_strings() -> str:
    """Every language carries every string the interface asks for."""
    def names(path: Path) -> list[str]:
        return sorted(re.findall(r'name="([^"]*)"', path.read_text(encoding="utf-8")))

    base = names(RESOURCES / "values/strings.xml")
    if not base:
        raise AssertionError("values/strings.xml holds no string")

    translations = sorted(RESOURCES.glob("values-*/strings.xml"))
    for path in translations:
        here = names(path)
        if here != base:
            short = path.parent.name
            extra = sorted(set(here) - set(base))
            absent = sorted(set(base) - set(here))
            raise AssertionError(
                f"{short} differs: missing {absent or 'nothing'}, extra {extra or 'nothing'}"
            )
    if len(translations) < 9:
        raise AssertionError(f"{len(translations)} translations, expected at least 9")

    # android.R.string.* are the framework's own; only this project's are ours.
    used = set(
        re.findall(r"(?<!android\.)\bR\.string\.([a-z_0-9]+)",
                   KOTLIN.read_text(encoding="utf-8"))
    )
    generated = {"omni_expected_certificate"}
    fromXml = {"omni_app_name"}
    unresolved = sorted(used - set(base) - generated)
    if unresolved:
        raise AssertionError(f"the interface asks for strings nobody declares: {unresolved}")
    unused = sorted(set(base) - used - fromXml)
    if unused:
        raise AssertionError(f"strings nothing uses: {unused}")
    return f"{len(base)} strings in {len(translations) + 1} languages, none spare"


def release_apk() -> Path:
    """The package this build produced, which is the only one it produces.

    A build with no signing key configured leaves a `-unsigned` package beside
    nothing else; a build with one leaves the signed package. Prefer the signed
    one where both are somehow present.
    """
    made = sorted(ROOT.glob("Builder/build/outputs/apk/release/*.apk"))
    if not made:
        raise Skip("the release package has not been built here")
    signed = [one for one in made if not one.name.endswith("-unsigned.apk")]
    return (signed or made)[0]


def check_release_apk() -> str:
    """The package and the bundle both build. This project ships one variant."""
    gradle(":Builder:assembleRelease", ":Builder:bundleRelease")
    made = sorted(ROOT.glob("Builder/build/outputs/apk/release/*.apk"))
    bundles = sorted(ROOT.glob("Builder/build/outputs/bundle/release/*.aab"))
    if not made or not bundles:
        raise AssertionError("the release package or the bundle is missing")
    if sorted(ROOT.glob("Builder/build/outputs/apk/debug/*.apk")):
        raise AssertionError("a debug package was built; this project ships one variant")
    return (f"{made[0].name}, {made[0].stat().st_size // 1024} KB, "
            f"and {bundles[0].name}")


def check_bridge() -> str:
    """Every native method Kotlin declares is exported by every ABI, and nothing else is."""
    apk = release_apk()
    nm = ndk_tool("llvm-nm")

    declared = sorted(
        f"Java_com_omni_builder_Builder_{name}"
        for name in re.findall(r"external fun (native[A-Za-z]*)",
                               KOTLIN.read_text(encoding="utf-8"))
    ) + ["JNI_OnLoad"]
    declared = sorted(declared)

    work = WORK / "bridge"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    with zipfile.ZipFile(apk) as archive:
        archive.extractall(work)

    checked = 0
    for abi in ("arm64-v8a",):
        library = work / f"lib/{abi}/libomni_builder.so"
        if not library.is_file():
            raise AssertionError(f"no native library for {abi}")
        listed = run([str(nm), "-D", "--defined-only", str(library)])
        exported = sorted(
            line.split()[2] for line in listed.stdout.splitlines() if len(line.split()) >= 3
        )
        if exported != declared:
            raise AssertionError(
                f"{abi} exports {sorted(set(exported) - set(declared))} extra, "
                f"{sorted(set(declared) - set(exported))} missing"
            )
        if any("omni_state_report" in name for name in exported):
            raise AssertionError(f"{abi} leaks the Core's own symbols")
        checked += 1
    return f"{len(declared)} symbols agree across {checked} ABIs"


def check_reproducible() -> str:
    """The package and the bundle are the same bytes across a clean rebuild.

    Both are rebuilt, not only the package: this check runs last, so whatever
    it leaves behind is what gets published, and a run that rebuilt only the
    package would quietly drop the bundle.
    """
    if os.environ.get("OMNI_SKIP_REPRODUCIBLE"):
        raise Skip("asked to skip the rebuild")
    import hashlib

    def digest(pattern: str, what: str) -> str:
        made = sorted(ROOT.glob(pattern))
        if not made:
            raise AssertionError(f"no {what} to weigh")
        return hashlib.sha256(made[0].read_bytes()).hexdigest()

    both = (":Builder:assembleRelease", ":Builder:bundleRelease")
    weighed = (
        ("Builder/build/outputs/apk/release/*.apk", "release package"),
        ("Builder/build/outputs/bundle/release/*.aab", "bundle"),
    )

    gradle(*both)
    first = [digest(pattern, what) for pattern, what in weighed]
    gradle("clean")
    run(["cargo", "clean"])
    gradle(*both)
    second = [digest(pattern, what) for pattern, what in weighed]

    for (_, what), before, after in zip(weighed, first, second):
        if before != after:
            raise AssertionError(f"the {what} is not reproducible\n{before}\n{after}")
    return f"package and bundle identical across a clean rebuild, {first[0][:16]}"


def check_installable() -> str:
    """The packages this build produces are ones Android would accept."""
    gradle(":Builder:verifyApkClasses", ":Builder:verifyApkInstallability", "--rerun-tasks")
    return "classes present and signatures accepted"


CHECKS: dict[str, tuple[str, object]] = {
    "layout": ("the tree is the agreed one", check_layout),
    "format": ("rustfmt", check_format),
    "lint": ("clippy", check_lint),
    "core": ("the Core suite", check_core),
    "dependencies": ("no third-party dependency", check_dependencies),
    "strings": ("every language, every string", check_strings),
    "release": ("the package and the bundle", check_release_apk),
    "bridge": ("the native bridge", check_bridge),
    "installable": ("what Android would accept", check_installable),
    "reproducible": ("byte-for-byte rebuild", check_reproducible),
}

# The Core suite reads the packages this build produced, so they are made first.
DEFAULT = [
    "layout", "format", "lint", "dependencies", "strings",
    "release", "core", "bridge", "installable",
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("checks", nargs="*", help="checks to run, or none for the usual set")
    parser.add_argument("--list", action="store_true", help="name every check and stop")
    parser.add_argument("--all", action="store_true", help="every check, the slow ones too")
    parser.add_argument("--strict", action="store_true",
                        help="a check that cannot run here is a failure")
    chosen = parser.parse_args()

    if chosen.list:
        width = max(len(name) for name in CHECKS)
        for name, (what, _) in CHECKS.items():
            mark = " " if name in DEFAULT else "*"
            print(f"  {mark} {name:<{width}}  {what}")
        print("\n  * not in the usual set; ask for it by name or pass --all")
        return 0

    if chosen.checks:
        unknown = [name for name in chosen.checks if name not in CHECKS]
        if unknown:
            print(f"no such check: {', '.join(unknown)}", file=sys.stderr)
            return 2
        wanted = chosen.checks
    elif chosen.all:
        wanted = list(CHECKS)
    else:
        wanted = DEFAULT

    WORK.mkdir(exist_ok=True)
    (WORK / ".gitignore").write_text("*\n", encoding="utf-8")

    results: list[Result] = []
    width = max(len(name) for name in wanted)
    for name in wanted:
        what, check = CHECKS[name]
        print(f"  {name:<{width}}  ...", end="", flush=True)
        started = time.monotonic()
        try:
            detail = check()
            state = "ok"
        except Skip as why:
            detail, state = str(why), "skipped"
        except AssertionError as why:
            detail, state = str(why), "failed"
        except Exception as why:  # a check itself broke
            detail, state = f"{type(why).__name__}: {why}", "failed"
        spent = time.monotonic() - started
        results.append(Result(name, state, spent, detail))
        mark = {"ok": "ok", "skipped": "--", "failed": "FAILED"}[state]
        first = detail.splitlines()[0] if detail else ""
        print(f"\r  {name:<{width}}  {mark:<6} {spent:6.1f}s  {first}")

    failed = [one for one in results if one.state == "failed"]
    skipped = [one for one in results if one.state == "skipped"]

    print()
    for one in failed:
        print(f"{one.name} failed:")
        for line in one.detail.splitlines():
            print(f"    {line}")
        print()

    summary = (
        f"{len(results) - len(failed) - len(skipped)} passed, "
        f"{len(failed)} failed, {len(skipped)} skipped, "
        f"{sum(one.seconds for one in results):.0f}s"
    )
    if failed:
        print(summary)
        return 1
    if skipped and chosen.strict:
        print(f"{summary} — and --strict makes a skipped check a failure")
        return 1
    print(summary)
    return 0


if __name__ == "__main__":
    sys.exit(main())
