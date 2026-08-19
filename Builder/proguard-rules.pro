# Omni_Builder — R8 / ProGuard rules
#
# Only what is provably necessary is kept. Every rule below has a reason next to
# it; a rule without a reason is a rule nobody can safely remove later.

# The JNI bridge resolves these methods by name at runtime, so the shrinker
# cannot see the reference and would otherwise remove or rename them. The symbol
# names are baked into Builder.cpp as Java_com_omni_builder_Builder_*.
-keepclasseswithmembernames class com.omni.builder.Builder {
    native <methods>;
}
-keep class com.omni.builder.Builder { *; }

# BuilderActivity is referenced from AndroidManifest.xml, not from code.
-keep class com.omni.builder.BuilderActivity { *; }

# Keep the source file and line numbers in stack traces, and nothing more.
# Diagnostics that cannot be located are not actionable (directive section 33).
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile

# No global -dontwarn. An unresolved reference is a defect to fix, and silencing
# the whole class of warnings would hide exactly the breakage this file exists to
# prevent.
