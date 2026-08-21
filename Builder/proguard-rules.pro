-keepclasseswithmembernames class com.omni.builder.Builder {
    native <methods>;
}
-keep class com.omni.builder.Builder { *; }

-keep class com.omni.builder.BuilderActivity { *; }
-keep class com.omni.builder.BuilderApplication { *; }

-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile
