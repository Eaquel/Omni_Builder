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
    }

    private var logDestinationView: TextView? = null

    private var logCopiesView: LinearLayout? = null

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
            is Builder.LoadState.Failed -> renderLoadFailure(root, load.reason)
            is Builder.LoadState.Loaded -> renderCoreState(root)
        }

        renderLogSection(root)

        setContentView(
            ScrollView(this).apply {
                setBackgroundColor(color(R.color.omni_background))
                isFillViewport = true
                addView(root, MATCH_PARENT, WRAP_CONTENT)
            }
        )
    }

    override fun onResume() {
        super.onResume()
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity resumed.")
    }

    override fun onPause() {
        super.onPause()
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity paused.")
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        OmniLog.setPublishListener(null)
        logDestinationView = null
        logCopiesView = null
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity destroyed.")
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun requestLegacyStoragePermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            return
        }
        if (checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
            == PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        OmniLog.event(
            LogLevel.INFO,
            "log",
            "Requesting the storage permission; on Android 9 it is the only way " +
                "to write into shared Documents.",
        )
        requestPermissions(
            arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE),
            STORAGE_PERMISSION_REQUEST,
        )
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != STORAGE_PERMISSION_REQUEST) {
            return
        }
        val granted = grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED
        OmniLog.event(
            if (granted) LogLevel.INFO else LogLevel.WARN,
            "log",
            if (granted) {
                "Storage permission granted; logs will be published to Documents."
            } else {
                "Storage permission denied; logs stay in the application's own " +
                    "storage and are not published to Documents."
            },
        )
        OmniLog.flushSession()
    }

    private fun renderLoadFailure(root: LinearLayout, reason: String) {
        root.addView(title(getString(R.string.omni_app_name)))
        root.addView(banner(getString(R.string.omni_core_unavailable), R.color.omni_error))
        root.addView(body(reason))
    }

    private fun renderCoreState(root: LinearLayout) {
        val started = System.nanoTime()
        val state = try {
            CoreState.parse(Builder.nativeStateReport(Builder.observedEnvironment(this)))
        } catch (error: RuntimeException) {
            val reason = "The Core produced a report this build cannot read: " +
                "${error.message}. The interface and the Core are out of step."
            OmniLog.event(LogLevel.ERROR, "core", reason)
            renderLoadFailure(root, reason)
            return
        }

        val elapsedMicroseconds = (System.nanoTime() - started) / 1_000
        OmniLog.event(
            LogLevel.INFO,
            "core",
            "State report read in ${elapsedMicroseconds} us: core ${state.version} " +
                "(${state.status}), ABI ${state.abiVersion}, " +
                "${state.toolchainVerified}/${state.toolchain.size} pins verified, " +
                "${state.pluginsImplemented}/${state.plugins.size} plugins implemented, " +
                "${state.diagnostics.size} diagnostics.",
        )

        root.addView(title(getString(R.string.omni_app_name)))
        root.addView(
            body(
                getString(
                    R.string.omni_core_line,
                    state.version,
                    state.status,
                    state.abiVersion,
                )
            )
        )
        root.addView(body(state.phase))

        root.addView(
            banner(
                getString(R.string.omni_not_a_builder_yet),
                R.color.omni_warning,
            )
        )

        section(root, R.string.omni_section_roadmap)
        root.addView(
            body(
                getString(
                    R.string.omni_roadmap_summary,
                    state.roadmapDelivered,
                    state.roadmap.size,
                )
            )
        )
        state.roadmap.forEach { phase ->
            root.addView(
                keyValue(
                    phase.name,
                    phase.delivers,
                    phaseColor(phase.state),
                    trailing = phase.state,
                )
            )
        }

        section(root, R.string.omni_section_self_hosting)
        root.addView(
            keyValue(
                getString(R.string.omni_self_hosted),
                if (state.selfHosted) getString(R.string.omni_yes) else getString(R.string.omni_no),
                if (state.selfHosted) R.color.omni_ok else R.color.omni_warning,
            )
        )
        root.addView(body(state.selfHostingNote))
        state.bootstrapDependencies.forEach { root.addView(bullet(it)) }

        section(root, R.string.omni_section_subsystems)
        root.addView(
            body(
                getString(
                    R.string.omni_subsystems_summary,
                    state.subsystemsProduction,
                    state.subsystems.size,
                )
            )
        )
        state.subsystems.forEach { row ->
            root.addView(
                keyValue(
                    row.name,
                    getString(R.string.omni_subsystem_detail, row.directiveSection, row.summary),
                    statusColor(row.status),
                    trailing = row.status,
                )
            )
            row.missing.forEach { root.addView(bullet(it)) }
        }

        section(root, R.string.omni_section_toolchain)
        root.addView(
            body(
                getString(
                    R.string.omni_toolchain_summary,
                    state.toolchainVerified,
                    state.toolchain.size,
                )
            )
        )
        state.toolchain.forEach { row ->
            root.addView(
                keyValue(
                    row.displayName,
                    buildString {
                        append(row.pinned)
                        row.observed?.let { append(" · seen ").append(it) }
                        if (row.checksumPinned) append(" · checksum pinned")
                    },
                    stateColor(row.state),
                    trailing = row.state,
                )
            )
        }

        section(root, R.string.omni_section_plugins)
        root.addView(
            body(
                getString(
                    R.string.omni_plugins_summary,
                    state.pluginsImplemented,
                    state.plugins.size,
                )
            )
        )
        state.plugins.forEach { row ->
            root.addView(
                keyValue(
                    row.displayName,
                    row.roadmapPhase,
                    statusColor(row.status),
                    trailing = row.status,
                )
            )
        }

        section(root, R.string.omni_section_diagnostics)
        if (state.diagnostics.isEmpty()) {
            root.addView(body(getString(R.string.omni_no_diagnostics)))
        } else {
            state.diagnostics.forEach { diagnostic ->
                root.addView(
                    keyValue(
                        "${diagnostic.code}  ${diagnostic.severity}",
                        diagnostic.message,
                        severityColor(diagnostic.severity),
                    )
                )
                diagnostic.suggestion?.let { root.addView(bullet(it)) }
            }
        }
    }

    private fun renderLogSection(root: LinearLayout) {
        section(root, R.string.omni_section_logs)

        val destination = OmniLog.flushSession()
        val row = keyValue(
            OmniLog.SESSION_FILE,
            OmniLog.describeDestination(destination),
            destinationColor(destination),
        )
        logDestinationView = row.getChildAt(1) as TextView
        root.addView(row)

        val copiesHolder = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        logCopiesView = copiesHolder
        root.addView(copiesHolder)
        renderCopies(copiesHolder, OmniLog.lastCopies())

        root.addView(body(getString(R.string.omni_log_explanation, OmniLog.DIRECTORY_NAME)))

        OmniLog.setPublishListener { outcome ->
            runOnUiThread {
                logDestinationView?.apply {
                    text = OmniLog.describeDestination(outcome)
                    setTextColor(color(destinationColor(outcome)))
                }
                logCopiesView?.let { holder ->
                    holder.removeAllViews()
                    renderCopies(holder, OmniLog.lastCopies())
                }
            }
        }

        val crash = OmniLog.lastCrash()
        if (crash == null) {
            root.addView(body(getString(R.string.omni_no_crash)))
        } else {
            root.addView(
                keyValue(
                    OmniLog.CRASH_FILE,
                    getString(R.string.omni_crash_recorded),
                    R.color.omni_error,
                )
            )
            root.addView(mono(crash))
        }
    }

    private fun renderCopies(holder: LinearLayout, copies: List<LogCopy>) {
        if (copies.isEmpty()) {
            holder.addView(body(getString(R.string.omni_log_not_written_yet)))
            return
        }
        copies.forEach { copy ->
            holder.addView(
                keyValue(
                    copy.label,
                    copy.error?.let { getString(R.string.omni_log_copy_failed, copy.location, it) }
                        ?: copy.location,
                    if (copy.succeeded) R.color.omni_ok else R.color.omni_error,
                    trailing = if (copy.succeeded) {
                        getString(R.string.omni_log_written)
                    } else {
                        getString(R.string.omni_log_failed)
                    },
                )
            )
        }
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
