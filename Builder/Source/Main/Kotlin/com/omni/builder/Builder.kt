package com.omni.builder

import android.Manifest
import android.app.Activity
import android.app.Application
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Typeface
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.text.Editable
import android.text.TextWatcher
import android.widget.EditText
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import org.json.JSONArray
import org.json.JSONObject

enum class LogLevel {
    TRACE,

    INFO,

    WARN,

    ERROR,
}

sealed interface LogDestination {
    data class Published(val location: String) : LogDestination

    data class Pending(val location: String) : LogDestination

    data class PrivateOnly(val location: String, val reason: String) : LogDestination
}

data class LogCopy(
    val label: String,
    val location: String,
    val error: String?,
) {
    val succeeded: Boolean get() = error == null
}

object OmniLog {

    const val DIRECTORY_NAME: String = "Omni_Builder"

    const val SESSION_FILE: String = "Session_Log.txt"

    const val CRASH_FILE: String = "Crash_Log.txt"

    const val MAX_BYTES: Int = 256 * 1024

    private const val TAG = "OmniBuilder"

    private val lock = Any()
    private val session = StringBuilder(8 * 1024)

    private val publisher = java.util.concurrent.Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "omni-log-publisher").apply { isDaemon = true }
    }

    @Volatile
    private var lastPublished: LogDestination? = null

    @Volatile
    private var copies: List<LogCopy> = emptyList()

    @Volatile
    private var publishListener: ((LogDestination) -> Unit)? = null

    @Volatile
    private var context: Context? = null

    @Volatile
    private var installedAt: String = "not started"

    private fun timestamp(): String =
        SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US).format(Date())

    fun install(application: Application) {
        context = application.applicationContext
        installedAt = timestamp()

        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            try {
                recordCrash(thread, error)
            } catch (secondary: Throwable) {
                Log.e(TAG, "The crash could not be written to the log.", secondary)
            }
            if (previous != null) {
                previous.uncaughtException(thread, error)
            } else {
                android.os.Process.killProcess(android.os.Process.myPid())
            }
        }

        event(LogLevel.INFO, "session", "Session started.")
        event(LogLevel.INFO, "session", describeEnvironment(application))
    }

    fun event(level: LogLevel, tag: String, message: String) {
        val line = "${timestamp()}  ${level.name.padEnd(5)}  ${tag.padEnd(12)}  $message"
        synchronized(lock) { session.append(line).append('\n') }
        when (level) {
            LogLevel.ERROR -> Log.e(TAG, "$tag: $message")
            LogLevel.WARN -> Log.w(TAG, "$tag: $message")
            else -> Log.i(TAG, "$tag: $message")
        }
    }

    fun flushSession(): LogDestination {
        val started = System.nanoTime()
        val destination = write(SESSION_FILE, sessionDocument(), append = false)

        val elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000
        event(
            LogLevel.TRACE,
            "log",
            "Session written in ${elapsedMilliseconds} ms; " +
                describeDestination(destination),
        )
        return destination
    }

    fun recordCrash(thread: Thread, error: Throwable) {
        event(LogLevel.ERROR, "crash", "${error.javaClass.name}: ${error.message}")

        val record = buildString {
            append('\n').append("=".repeat(72)).append('\n')
            append("CRASH  ").append(timestamp()).append('\n')
            append("=".repeat(72)).append('\n')
            append("Thread: ").append(thread.name).append('\n')
            context?.let { append(describeEnvironment(it)).append('\n') }
            append('\n')
            append(Log.getStackTraceString(error))
            append('\n')
            append("Events leading up to the failure:").append('\n')
            append(synchronized(lock) { session.toString() })
        }

        writeBlocking(CRASH_FILE, record, append = true)
        writeBlocking(SESSION_FILE, sessionDocument(), append = false)
    }

    private fun sessionDocument(): String = buildString {
        append("Omni_Builder session log\n")
        append("Started: ").append(installedAt).append('\n')
        append("Written: ").append(timestamp()).append('\n')
        append("-".repeat(72)).append('\n')
        append(synchronized(lock) { session.toString() })
    }

    fun lastCrash(): String? {
        val file = privateFile(CRASH_FILE) ?: return null
        if (!file.isFile || file.length() == 0L) {
            return null
        }
        return runCatching {
            val text = file.readText()
            val start = text.lastIndexOf("CRASH  ")
            if (start < 0) null else text.substring(start).lineSequence().take(12).joinToString("\n")
        }.getOrNull()
    }

    fun describeDestination(destination: LogDestination): String = when (destination) {
        is LogDestination.Published -> destination.location
        is LogDestination.Pending -> "${destination.location} (publishing to Documents)"
        is LogDestination.PrivateOnly ->
            "${destination.location} (shared storage unavailable: ${destination.reason})"
    }

    fun lastPublishOutcome(): LogDestination? = lastPublished

    fun lastCopies(): List<LogCopy> = copies

    fun setPublishListener(listener: ((LogDestination) -> Unit)?) {
        publishListener = listener
        lastPublished?.let { outcome -> listener?.invoke(outcome) }
    }

    private fun describeEnvironment(context: Context): String {
        val info = context.applicationInfo
        return buildString {
            append("device=").append(Build.MANUFACTURER).append(' ').append(Build.MODEL)
            append("; android=").append(Build.VERSION.RELEASE)
            append(" (API ").append(Build.VERSION.SDK_INT).append(')')
            append("; abis=").append(Build.SUPPORTED_ABIS.joinToString("/"))
            append("; package=").append(context.packageName)
            append("; minSdk=").append(info.minSdkVersion)
            append("; targetSdk=").append(info.targetSdkVersion)
        }
    }

    private fun privateFile(name: String): File? = context?.let { File(it.filesDir, name) }

    private sealed interface PrivateWrite {
        data class Ok(val path: String) : PrivateWrite

        data class Failed(val destination: LogDestination) : PrivateWrite
    }

    private fun write(name: String, text: String, append: Boolean): LogDestination =
        when (val written = writePrivate(name, text, append)) {
            is PrivateWrite.Failed -> written.destination
            is PrivateWrite.Ok -> {
                publisher.execute {
                    val outcome = publishNow(name, written.path)
                    lastPublished = outcome
                    if (outcome is LogDestination.PrivateOnly) {
                        event(
                            LogLevel.WARN,
                            "log",
                            "Publishing $name to Documents failed: ${outcome.reason}. " +
                                "The private copy at ${outcome.location} is complete.",
                        )
                    }
                    publishListener?.invoke(outcome)
                }
                LogDestination.Pending(written.path)
            }
        }

    private fun writeBlocking(name: String, text: String, append: Boolean): LogDestination =
        when (val written = writePrivate(name, text, append)) {
            is PrivateWrite.Failed -> written.destination
            is PrivateWrite.Ok -> publishNow(name, written.path).also {
                lastPublished = it
                publishListener?.invoke(it)
            }
        }

    private fun writePrivate(name: String, text: String, append: Boolean): PrivateWrite {
        val target = privateFile(name)
            ?: return PrivateWrite.Failed(
                LogDestination.PrivateOnly("(not started)", "no application context")
            )

        return try {
            FileOutputStream(target, append).use { it.write(text.toByteArray(Charsets.UTF_8)) }
            trim(target)
            PrivateWrite.Ok(target.absolutePath)
        } catch (failure: Exception) {
            Log.e(TAG, "The private log copy could not be written.", failure)
            PrivateWrite.Failed(
                LogDestination.PrivateOnly("(unwritable)", failure.describe())
            )
        }
    }

    private fun trim(file: File) {
        if (file.length() <= MAX_BYTES) {
            return
        }
        val keep = file.readBytes().let { it.copyOfRange(it.size - MAX_BYTES / 2, it.size) }
        FileOutputStream(file, false).use { stream ->
            stream.write("[earlier entries dropped at ${timestamp()}]\n".toByteArray(Charsets.UTF_8))
            stream.write(keep)
        }
    }

    private fun publishNow(name: String, privatePath: String): LogDestination {
        val target = privateFile(name)
            ?: return LogDestination.PrivateOnly(privatePath, "no application context")

        val bytes = try {
            target.readBytes()
        } catch (failure: Exception) {
            return LogDestination.PrivateOnly(privatePath, failure.describe())
        }

        val attempts = mutableListOf(
            LogCopy("Application storage", privatePath, null),
            publishToApplicationExternal(name, bytes),
            publishToSharedDocuments(name, bytes),
        )
        copies = attempts

        val shared = attempts.last()
        if (shared.succeeded) {
            return LogDestination.Published(shared.location)
        }
        val external = attempts[1]
        if (external.succeeded) {
            return LogDestination.Published(external.location)
        }
        return LogDestination.PrivateOnly(
            privatePath,
            shared.error ?: external.error ?: "unknown",
        )
    }

    private fun publishToApplicationExternal(name: String, bytes: ByteArray): LogCopy {
        val label = "Shared storage"
        val context = context ?: return LogCopy(label, "(not started)", "no application context")

        return try {
            val root = context.getExternalFilesDir(null)
                ?: return LogCopy(label, "(unavailable)", "external storage is not mounted")
            val directory = File(root, DIRECTORY_NAME)
            if (!directory.isDirectory && !directory.mkdirs()) {
                return LogCopy(label, directory.absolutePath, "the directory could not be created")
            }
            val file = File(directory, name)
            FileOutputStream(file, false).use { stream ->
                stream.write(bytes)
                stream.flush()
            }
            if (file.length() != bytes.size.toLong()) {
                return LogCopy(
                    label,
                    file.absolutePath,
                    "wrote ${bytes.size} bytes but the file holds ${file.length()}",
                )
            }
            LogCopy(label, file.absolutePath, null)
        } catch (failure: Exception) {
            LogCopy(label, "(failed)", failure.describe())
        }
    }

    private fun publishToSharedDocuments(name: String, bytes: ByteArray): LogCopy {
        val label = "Documents"
        val context = context ?: return LogCopy(label, "(not started)", "no application context")
        val location = "${Environment.DIRECTORY_DOCUMENTS}/$DIRECTORY_NAME/$name"

        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                publishThroughMediaStore(context, name, bytes)
            } else {
                publishThroughLegacyStorage(context, name, bytes)
            }
            LogCopy(label, location, null)
        } catch (failure: Exception) {
            LogCopy(label, location, failure.describe())
        }
    }

    private fun Exception.describe(): String {
        val detail = message?.takeIf { it.isNotBlank() }
        return if (detail == null) javaClass.simpleName else "${javaClass.simpleName}: $detail"
    }

    private fun publishThroughMediaStore(context: Context, name: String, bytes: ByteArray) {
        val resolver = context.contentResolver
        val collection = MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        val relativePath = "${Environment.DIRECTORY_DOCUMENTS}/$DIRECTORY_NAME"
        val absolutePath = expectedAbsolutePath(name)
        val tried = StringBuilder()

        findByData(resolver, collection, absolutePath)?.let { existing ->
            clearPendingAndTrashed(resolver, existing)
            writeTruncating(resolver, existing, bytes)
            return
        }
        tried.append("no row holds $absolutePath")

        findByName(resolver, collection, relativePath, name)?.let { existing ->
            clearPendingAndTrashed(resolver, existing)
            writeTruncating(resolver, existing, bytes)
            return
        }
        tried.append("; no row named $name under $relativePath")

        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, "text/plain")
            put(MediaStore.MediaColumns.RELATIVE_PATH, relativePath)
        }

        val created = try {
            resolver.insert(collection, values)
        } catch (refused: Exception) {
            tried.append("; insert refused: ${refused.describe()}")

            val removed = try {
                resolver.delete(
                    collection,
                    "${dataColumn()}=?",
                    arrayOf(absolutePath),
                )
            } catch (denied: Exception) {
                tried.append("; delete refused: ${denied.describe()}")
                -1
            }
            tried.append("; deleted $removed row(s)")

            if (removed <= 0) {
                throw IOException("$tried")
            }
            try {
                resolver.insert(collection, values)
            } catch (again: Exception) {
                throw IOException("$tried; insert refused again: ${again.describe()}")
            }
        } ?: throw IOException("MediaStore refused to create the entry ($tried)")

        writeTruncating(resolver, created, bytes)
    }

    @Suppress("DEPRECATION")
    private fun expectedAbsolutePath(name: String): String {
        val documents =
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS)
        return File(File(documents, DIRECTORY_NAME), name).absolutePath
    }

    @Suppress("DEPRECATION")
    private fun dataColumn(): String = MediaStore.MediaColumns.DATA

    private fun queryIncludingHidden(
        resolver: android.content.ContentResolver,
        collection: Uri,
        selection: String,
        arguments: Array<String>,
    ): Uri? = runCatching {
        val projection = arrayOf(MediaStore.MediaColumns._ID)
        val cursor = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val query = Bundle().apply {
                putString(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION, selection)
                putStringArray(
                    android.content.ContentResolver.QUERY_ARG_SQL_SELECTION_ARGS,
                    arguments,
                )
                putInt(MediaStore.QUERY_ARG_MATCH_PENDING, MediaStore.MATCH_INCLUDE)
                putInt(MediaStore.QUERY_ARG_MATCH_TRASHED, MediaStore.MATCH_INCLUDE)
            }
            resolver.query(collection, projection, query, null)
        } else {
            resolver.query(collection, projection, selection, arguments, null)
        }
        cursor?.use {
            if (it.moveToFirst()) ContentUris.withAppendedId(collection, it.getLong(0)) else null
        }
    }.getOrNull()

    private fun findByData(
        resolver: android.content.ContentResolver,
        collection: Uri,
        absolutePath: String,
    ): Uri? = queryIncludingHidden(
        resolver,
        collection,
        "${dataColumn()}=?",
        arrayOf(absolutePath),
    )

    private fun findByName(
        resolver: android.content.ContentResolver,
        collection: Uri,
        relativePath: String,
        name: String,
    ): Uri? = queryIncludingHidden(
        resolver,
        collection,
        "${MediaStore.MediaColumns.DISPLAY_NAME}=? AND " +
            "${MediaStore.MediaColumns.RELATIVE_PATH} LIKE ?",
        arrayOf(name, "$relativePath%"),
    )

    private fun clearPendingAndTrashed(
        resolver: android.content.ContentResolver,
        uri: Uri,
    ) {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.IS_PENDING, 0)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                put(MediaStore.MediaColumns.IS_TRASHED, 0)
            }
        }
        runCatching { resolver.update(uri, values, null, null) }
    }

    private fun writeTruncating(
        resolver: android.content.ContentResolver,
        uri: Uri,
        bytes: ByteArray,
    ) {
        val descriptor = resolver.openFileDescriptor(uri, "rwt")
            ?: throw IOException("MediaStore returned no descriptor")

        android.os.ParcelFileDescriptor.AutoCloseOutputStream(descriptor).use { stream ->
            stream.write(bytes)
            stream.flush()
        }

        val stored = runCatching {
            resolver.query(uri, arrayOf(MediaStore.MediaColumns.SIZE), null, null, null)
                ?.use { cursor -> if (cursor.moveToFirst()) cursor.getLong(0) else -1L }
                ?: -1L
        }.getOrDefault(-1L)

        if (stored > 0 && stored != bytes.size.toLong()) {
            throw IOException("stored $stored bytes of ${bytes.size}")
        }
    }

    @Suppress("DEPRECATION")
    private fun publishThroughLegacyStorage(context: Context, name: String, bytes: ByteArray) {
        if (context.checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
            != PackageManager.PERMISSION_GRANTED
        ) {
            throw IOException("the storage permission has not been granted")
        }

        val documents = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS)
        val directory = File(documents, DIRECTORY_NAME)
        if (!directory.isDirectory && !directory.mkdirs()) {
            throw IOException("could not create ${directory.absolutePath}")
        }

        FileOutputStream(File(directory, name), false).use { stream ->
            stream.write(bytes)
            stream.flush()
        }
    }
}

class BuilderApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        OmniLog.install(this)
    }
}

object Builder {

    sealed interface LoadState {
        data object Loaded : LoadState

        data class Failed(val reason: String) : LoadState
    }

    @Volatile
    private var loadState: LoadState? = null

    @Synchronized
    fun load(): LoadState {
        loadState?.let { return it }
        val state = try {
            System.loadLibrary("omni_builder")
            OmniLog.event(LogLevel.INFO, "native", "libomni_builder.so loaded.")
            LoadState.Loaded
        } catch (error: UnsatisfiedLinkError) {
            val reason = "The native library could not be loaded: " +
                "${error.message ?: "no detail"}. This usually means the Omni Core " +
                "was not linked into this build, or the bridge and the Core " +
                "disagree on the ABI version."
            OmniLog.event(LogLevel.ERROR, "native", reason)
            LoadState.Failed(reason)
        }
        loadState = state
        return state
    }

    external fun nativeAbiVersion(): Int

    external fun nativeCoreVersion(): String

    external fun nativeStateReport(observedEnvironment: String?): String

    external fun nativeCreateProject(root: String, spec: String): String

    external fun nativeBuildProject(root: String, outputPath: String, keyPath: String): String

    external fun nativeVerifySelf(packagePath: String, expectedCertificate: String?): String

    fun observedEnvironment(context: Context): String {
        val info = context.applicationInfo
        return buildString {
            append("minSdk=").append(info.minSdkVersion)
            append(';')
            append("targetSdk=").append(info.targetSdkVersion)
        }
    }
}

data class ToolchainRow(
    val displayName: String,
    val pinned: String,
    val observed: String?,
    val state: String,
    val checksumPinned: Boolean,
)

data class PluginRow(
    val displayName: String,
    val status: String,
    val roadmapPhase: String,
)

data class ProjectSpec(
    val packageName: String,
    val label: String,
    val abis: List<String>,
    val minSdk: Int,
    val targetSdk: Int,
    val languages: List<String>,
) {
    fun encode(): String = buildString {
        append("package=").append(packageName).append(';')
        append("label=").append(label).append(';')
        append("abis=").append(abis.joinToString(",")).append(';')
        append("minSdk=").append(minSdk).append(';')
        append("targetSdk=").append(targetSdk).append(';')
        append("languages=").append(languages.joinToString(","))
    }
}

data class CreateOutcome(
    val created: Boolean,
    val root: String?,
    val folders: List<String>,
    val files: List<String>,
    val error: String?,
    val suggestion: String?,
) {
    companion object {
        fun parse(document: String): CreateOutcome {
            val root = JSONObject(document)
            val layout = root.optJSONObject("layout")
            fun strings(name: String): List<String> {
                val array = layout?.optJSONArray(name) ?: return emptyList()
                return (0 until array.length()).map { array.getString(it) }
            }
            return CreateOutcome(
                created = root.optBoolean("created", false),
                root = layout?.optString("root")?.ifEmpty { null },
                folders = strings("folders"),
                files = strings("files"),
                error = root.optString("error").ifEmpty { null },
                suggestion = root.optString("suggestion").ifEmpty { null },
            )
        }
    }
}

data class BuildOutcome(
    val built: Boolean,
    val path: String?,
    val bytes: Long,
    val entries: Long,
    val signed: Boolean,
    val carriesCode: Boolean,
    val guardVerdict: String?,
    val rulesApplied: Long,
    val findings: List<String>,
    val error: String?,
) {
    companion object {
        fun parse(document: String): BuildOutcome {
            val root = JSONObject(document)
            val packaged = root.optJSONObject("package")
            val guard = packaged?.optJSONObject("guard")
            val findings = mutableListOf<String>()
            guard?.optJSONArray("findings")?.let { array ->
                for (index in 0 until array.length()) {
                    val item = array.getJSONObject(index)
                    findings.add("${item.getString("what")} ${item.getString("remedy")}")
                }
            }
            root.optJSONArray("diagnostics")?.let { array ->
                for (index in 0 until array.length()) {
                    val item = array.getJSONObject(index)
                    val text = "${item.getString("code")}: ${item.getString("message")}"
                    if (!findings.contains(text)) findings.add(text)
                }
            }
            return BuildOutcome(
                built = root.optBoolean("built", false),
                path = root.optString("path").ifEmpty { null },
                bytes = packaged?.optLong("bytes") ?: 0L,
                entries = packaged?.optLong("entries") ?: 0L,
                signed = packaged?.optBoolean("signed", false) ?: false,
                carriesCode = packaged?.optBoolean("carriesCode", false) ?: false,
                guardVerdict = guard?.optString("verdict")?.ifEmpty { null },
                rulesApplied = guard?.optLong("rulesApplied") ?: 0L,
                findings = findings,
                error = root.optString("error").ifEmpty { null },
            )
        }
    }
}

data class SelfCheck(
    val standing: String,
    val reason: String,
    val certificate: String?,
) {
    companion object {
        fun parse(document: String): SelfCheck {
            val integrity = JSONObject(document).getJSONObject("integrity")
            return SelfCheck(
                standing = integrity.getString("standing"),
                reason = integrity.getString("reason"),
                certificate = integrity.optString("certificate").ifEmpty { null },
            )
        }
    }
}

data class PhaseRow(
    val number: Int,
    val name: String,
    val state: String,
    val delivers: String,
)

data class SubsystemRow(
    val name: String,
    val status: String,
    val directiveSection: Int,
    val summary: String,
    val missing: List<String>,
)

data class DiagnosticRow(
    val code: String,
    val severity: String,
    val message: String,
    val suggestion: String?,
)

data class CoreState(
    val version: String,
    val status: String,
    val phase: String,
    val roadmap: List<PhaseRow>,
    val roadmapDelivered: Int,
    val abiVersion: Int,
    val selfHosted: Boolean,
    val selfHostingNote: String,
    val bootstrapDependencies: List<String>,
    val subsystems: List<SubsystemRow>,
    val subsystemsProduction: Int,
    val toolchain: List<ToolchainRow>,
    val toolchainVerified: Int,
    val plugins: List<PluginRow>,
    val pluginsImplemented: Int,
    val diagnostics: List<DiagnosticRow>,
) {
    companion object {
        fun parse(document: String): CoreState {
            val root = JSONObject(document)
            val core = root.getJSONObject("core")
            val roadmap = root.getJSONObject("roadmap")
            val subsystems = root.getJSONObject("subsystems")
            val toolchain = root.getJSONObject("toolchain")
            val plugins = root.getJSONObject("plugins")

            return CoreState(
                version = core.getString("version"),
                status = core.getString("status"),
                phase = core.getString("phase"),
                roadmap = roadmap.getJSONArray("phases").map { item ->
                    PhaseRow(
                        number = item.getInt("number"),
                        name = item.getString("name"),
                        state = item.getString("state"),
                        delivers = item.getString("delivers"),
                    )
                },
                roadmapDelivered = roadmap.getInt("delivered"),
                abiVersion = core.getInt("abiVersion"),
                selfHosted = core.getBoolean("selfHosted"),
                selfHostingNote = core.getString("selfHostingNote"),
                bootstrapDependencies = core.getJSONArray("bootstrapDependencies").strings(),
                subsystems = subsystems.getJSONArray("detail").map { item ->
                    SubsystemRow(
                        name = item.getString("name"),
                        status = item.getString("status"),
                        directiveSection = item.getInt("directiveSection"),
                        summary = item.getString("summary"),
                        missing = item.getJSONArray("missing").strings(),
                    )
                },
                subsystemsProduction = subsystems.getInt("production"),
                toolchain = toolchain.getJSONArray("components").map { item ->
                    ToolchainRow(
                        displayName = item.getString("displayName"),
                        pinned = item.getString("pinned"),
                        observed = item.optString("observed").ifEmpty { null },
                        state = item.getString("state"),
                        checksumPinned = item.has("checksum"),
                    )
                },
                toolchainVerified = toolchain.getInt("verified"),
                plugins = plugins.getJSONArray("contracts").map { item ->
                    PluginRow(
                        displayName = item.getString("displayName"),
                        status = item.getString("status"),
                        roadmapPhase = item.getString("roadmapPhase"),
                    )
                },
                pluginsImplemented = plugins.getInt("implemented"),
                diagnostics = root.getJSONArray("diagnostics").map { item ->
                    DiagnosticRow(
                        code = item.getString("code"),
                        severity = item.getString("severity"),
                        message = item.getString("message"),
                        suggestion = item.optString("suggestion").ifEmpty { null },
                    )
                },
            )
        }

        private fun JSONArray.strings(): List<String> =
            (0 until length()).map { getString(it) }

        private fun <T> JSONArray.map(transform: (JSONObject) -> T): List<T> =
            (0 until length()).map { transform(getJSONObject(it)) }
    }
}

class BuilderActivity : Activity() {

    private companion object {
        const val STORAGE_PERMISSION_REQUEST = 1
        val ABI_CHOICES = listOf(
            "32 bit" to listOf("armeabi-v7a"),
            "64 bit" to listOf("arm64-v8a"),
            "32 + 64 bit" to listOf("armeabi-v7a", "arm64-v8a"),
        )
        val ANDROID_RELEASES = listOf(
            28 to "Android 9",
            29 to "Android 10",
            30 to "Android 11",
            31 to "Android 12",
            32 to "Android 12L",
            33 to "Android 13",
            34 to "Android 14",
            35 to "Android 15",
            36 to "Android 16",
        )
        val LANGUAGE_CHOICES = listOf(
            "rust" to "Rust",
            "cpp" to "C++",
            "kotlin" to "Kotlin",
            "java" to "Java",
        )
    }

    private var packageName_ = "com.tr.yt"
    private var appLabel = "My App"
    private var abiChoice = 1
    private var minSdk = 28
    private var targetSdk = 36
    private val languages = linkedSetOf("kotlin")

    private lateinit var results: LinearLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity created.")
        requestLegacyStoragePermissionIfNeeded()

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(color(R.color.omni_background))
            setPadding(dp(R.dimen.omni_screen_padding))
        }

        when (val load = Builder.load()) {
            is Builder.LoadState.Failed -> {
                root.addView(banner(load.reason, R.color.omni_error))
                setContentView(scroll(root))
                return
            }
            is Builder.LoadState.Loaded -> Unit
        }

        root.addView(renderIntegrity())
        root.addView(title(getString(R.string.omni_app_name)))

        section(root, R.string.omni_form_identity)
        root.addView(field(getString(R.string.omni_form_package), packageName_) { packageName_ = it })
        root.addView(field(getString(R.string.omni_form_label), appLabel) { appLabel = it })

        section(root, R.string.omni_form_architecture)
        root.addView(chips(ABI_CHOICES.map { it.first }, { it == abiChoice }) { abiChoice = it })

        section(root, R.string.omni_form_platform)
        root.addView(
            chips(ANDROID_RELEASES.map { it.second }, { ANDROID_RELEASES[it].first == minSdk }) {
                minSdk = ANDROID_RELEASES[it].first
                if (targetSdk < minSdk) targetSdk = minSdk
            }
        )

        section(root, R.string.omni_form_languages)
        root.addView(
            chips(
                LANGUAGE_CHOICES.map { it.second },
                { languages.contains(LANGUAGE_CHOICES[it].first) },
            ) { index ->
                val key = LANGUAGE_CHOICES[index].first
                if (!languages.remove(key)) languages.add(key)
            }
        )
        root.addView(body(getString(R.string.omni_form_no_compiler)))

        results = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        root.addView(action(getString(R.string.omni_action_create)) { createProject() })
        root.addView(action(getString(R.string.omni_action_build)) { buildProject() })
        root.addView(results)

        setContentView(scroll(root))
    }

    private fun scroll(inner: LinearLayout) = ScrollView(this).apply {
        setBackgroundColor(color(R.color.omni_background))
        isFillViewport = true
        addView(inner, MATCH_PARENT, WRAP_CONTENT)
    }

    private fun projectRoot() = File(getExternalFilesDir(null) ?: filesDir, "Projects/$appLabel")

    private fun spec() = ProjectSpec(
        packageName = packageName_.trim(),
        label = appLabel.trim(),
        abis = ABI_CHOICES[abiChoice].second,
        minSdk = minSdk,
        targetSdk = targetSdk,
        languages = languages.toList(),
    )

    private fun createProject() {
        results.removeAllViews()
        val outcome = runCatching {
            CreateOutcome.parse(Builder.nativeCreateProject(projectRoot().absolutePath, spec().encode()))
        }.getOrElse {
            OmniLog.recordCrash(Thread.currentThread(), it)
            results.addView(banner(it.message ?: it.javaClass.simpleName, R.color.omni_error))
            return
        }

        if (!outcome.created) {
            results.addView(banner(outcome.error ?: "refused", R.color.omni_error))
            outcome.suggestion?.let { results.addView(body(it)) }
            return
        }
        OmniLog.event(LogLevel.INFO, "project", "Created ${outcome.root}")
        results.addView(banner(getString(R.string.omni_created), R.color.omni_ok))
        outcome.root?.let { results.addView(body(it)) }
        outcome.files.forEach { results.addView(bullet(it)) }
    }

    private fun buildProject() {
        results.removeAllViews()
        val destination = File(getExternalFilesDir(null) ?: filesDir, "${appLabel.trim()}.apk")
        val key = File(filesDir, "signing.pk8")
        val started = System.nanoTime()
        val outcome = runCatching {
            BuildOutcome.parse(
                Builder.nativeBuildProject(
                    projectRoot().absolutePath,
                    destination.absolutePath,
                    key.absolutePath,
                )
            )
        }.getOrElse {
            OmniLog.recordCrash(Thread.currentThread(), it)
            results.addView(banner(it.message ?: it.javaClass.simpleName, R.color.omni_error))
            return
        }
        val elapsed = (System.nanoTime() - started) / 1_000_000

        if (!outcome.built) {
            OmniLog.event(LogLevel.ERROR, "build", "Refused: ${outcome.error}")
            results.addView(banner(getString(R.string.omni_refused), R.color.omni_error))
            outcome.error?.let { results.addView(body(it)) }
            outcome.findings.forEach { results.addView(bullet(it)) }
            return
        }

        OmniLog.event(LogLevel.INFO, "build", "Built ${outcome.bytes} bytes in $elapsed ms")
        results.addView(banner("${outcome.bytes} bytes · $elapsed ms", R.color.omni_ok))
        results.addView(
            keyValue(
                getString(R.string.omni_result_contents),
                if (outcome.carriesCode) "manifest + classes.dex" else "manifest",
                R.color.omni_muted,
                trailing = "${outcome.entries}",
            )
        )
        results.addView(
            keyValue(
                getString(R.string.omni_result_signature),
                "v2 · RSA-2048",
                if (outcome.signed) R.color.omni_ok else R.color.omni_error,
                trailing = if (outcome.signed) "SIGNED" else "NONE",
            )
        )
        results.addView(
            keyValue(
                getString(R.string.omni_result_policy),
                "${outcome.rulesApplied} rules",
                if (outcome.guardVerdict == "PASSED") R.color.omni_ok else R.color.omni_error,
                trailing = outcome.guardVerdict ?: "?",
            )
        )
        outcome.path?.let { results.addView(bullet(it)) }
    }

    private fun renderIntegrity(): TextView {
        val expected = getString(R.string.omni_expected_certificate).ifEmpty { null }
        val check = runCatching {
            SelfCheck.parse(Builder.nativeVerifySelf(applicationInfo.sourceDir, expected))
        }.getOrNull()

        if (check == null) {
            return banner(getString(R.string.omni_integrity_unknown), R.color.omni_warning)
        }
        OmniLog.event(LogLevel.INFO, "integrity", "${check.standing}: ${check.reason}")
        val colour = when (check.standing) {
            "TRUSTED" -> R.color.omni_ok
            "TAMPERED" -> R.color.omni_error
            else -> R.color.omni_warning
        }
        return banner("${check.standing} · ${check.reason}", colour)
    }

    private fun action(label: String, onPress: () -> Unit) = TextView(this).apply {
        text = label
        setTextColor(color(R.color.omni_ok))
        setTypeface(typeface, Typeface.BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
        gravity = Gravity.CENTER
        setPadding(dp(R.dimen.omni_gap))
        isClickable = true
        setOnClickListener { onPress() }
    }

    private fun field(label: String, initial: String, onChange: (String) -> Unit): View {
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
        }
        row.addView(TextView(this).apply {
            text = label
            setTextColor(color(R.color.omni_muted))
            setTextSize(TypedValue.COMPLEX_UNIT_PX, resources.getDimension(R.dimen.omni_text_small))
        })
        row.addView(EditText(this).apply {
            setText(initial)
            setTextColor(color(R.color.omni_foreground))
            setTextSize(TypedValue.COMPLEX_UNIT_PX, resources.getDimension(R.dimen.omni_text_body))
            isSingleLine = true
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                override fun afterTextChanged(s: Editable?) {
                    onChange(s?.toString().orEmpty())
                }
            })
        })
        return row
    }

    private fun chips(
        labels: List<String>,
        selected: (Int) -> Boolean,
        onPick: (Int) -> Unit,
    ): View {
        val holder = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
        }
        val scroller = HorizontalScrollView(this).apply { isHorizontalScrollBarEnabled = false }
        val views = mutableListOf<TextView>()

        fun repaint() {
            views.forEachIndexed { index, view ->
                view.setTextColor(
                    color(if (selected(index)) R.color.omni_ok else R.color.omni_muted)
                )
            }
        }

        labels.forEachIndexed { index, label ->
            val chip = TextView(this).apply {
                text = label
                setTextSize(TypedValue.COMPLEX_UNIT_PX, resources.getDimension(R.dimen.omni_text_body))
                setPadding(dp(R.dimen.omni_gap), dp(R.dimen.omni_gap_small), dp(R.dimen.omni_gap), dp(R.dimen.omni_gap_small))
                isClickable = true
                setOnClickListener {
                    onPick(index)
                    repaint()
                }
            }
            views.add(chip)
            holder.addView(chip)
        }
        repaint()
        scroller.addView(holder)
        return scroller
    }

    private fun requestLegacyStoragePermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            return
        }
        if (checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
            != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(
                arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE),
                STORAGE_PERMISSION_REQUEST,
            )
        }
    }

    override fun onPause() {
        super.onPause()
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        OmniLog.setPublishListener(null)
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun section(root: LinearLayout, titleRes: Int) {
        root.addView(divider())
        root.addView(
            TextView(this).apply {
                text = getString(titleRes)
                setTextColor(color(R.color.omni_accent))
                setTypeface(Typeface.DEFAULT_BOLD)
                setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_section).toFloat())
                setPadding(0, dp(R.dimen.omni_gap), 0, dp(R.dimen.omni_gap_small))
            }
        )
    }

    private fun title(value: String) = TextView(this).apply {
        text = value
        setTextColor(color(R.color.omni_foreground))
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_title).toFloat())
    }

    private fun body(value: String) = TextView(this).apply {
        text = value
        setTextColor(color(R.color.omni_muted))
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_body).toFloat())
        setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
    }

    private fun mono(value: String) = TextView(this).apply {
        text = value
        setTextColor(color(R.color.omni_muted))
        setTypeface(Typeface.MONOSPACE)
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_small).toFloat())
        setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
    }

    private fun bullet(value: String) = TextView(this).apply {
        text = getString(R.string.omni_bullet, value)
        setTextColor(color(R.color.omni_muted))
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_small).toFloat())
        setPadding(dp(R.dimen.omni_gap), 0, 0, dp(R.dimen.omni_gap_small))
    }

    private fun banner(value: String, colorRes: Int) = TextView(this).apply {
        text = value
        setTextColor(color(R.color.omni_background))
        setBackgroundColor(color(colorRes))
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_body).toFloat())
        setPadding(dp(R.dimen.omni_gap_small))
        gravity = Gravity.START
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = dp(R.dimen.omni_gap)
            bottomMargin = dp(R.dimen.omni_gap)
        }
    }

    private fun keyValue(
        key: String,
        value: String,
        accentRes: Int,
        trailing: String? = null,
    ) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
        addView(
            TextView(context).apply {
                text = if (trailing == null) key else "$key   [$trailing]"
                setTextColor(color(accentRes))
                setTypeface(Typeface.MONOSPACE, Typeface.BOLD)
                setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_body).toFloat())
            }
        )
        addView(
            TextView(context).apply {
                text = value
                setTextColor(color(R.color.omni_muted))
                setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_small).toFloat())
            }
        )
    }

    private fun divider() = View(this).apply {
        setBackgroundColor(color(R.color.omni_divider))
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, dp(R.dimen.omni_divider_height))
    }

    private fun destinationColor(destination: LogDestination) = when (destination) {
        is LogDestination.Published -> R.color.omni_ok
        is LogDestination.Pending -> R.color.omni_accent
        is LogDestination.PrivateOnly -> R.color.omni_warning
    }

    private fun stateColor(state: String) = when (state) {
        "MATCH" -> R.color.omni_ok
        "MISMATCH", "MISSING" -> R.color.omni_error
        else -> R.color.omni_muted
    }

    private fun phaseColor(state: String) = when (state) {
        "DELIVERED" -> R.color.omni_ok
        "CURRENT" -> R.color.omni_warning
        else -> R.color.omni_muted
    }

    private fun statusColor(status: String) = when (status) {
        "PRODUCTION", "BETA" -> R.color.omni_ok
        "EXPERIMENTAL", "PARTIAL" -> R.color.omni_warning
        else -> R.color.omni_muted
    }

    private fun severityColor(severity: String) = when (severity) {
        "FATAL", "ERROR" -> R.color.omni_error
        "WARNING" -> R.color.omni_warning
        else -> R.color.omni_muted
    }

    private fun color(id: Int): Int = getColor(id)

    private fun dp(id: Int): Int = resources.getDimensionPixelSize(id)

    private fun View.setPadding(all: Int) = setPadding(all, all, all, all)
}
