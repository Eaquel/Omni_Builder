package com.omni.builder

import android.Manifest
import android.animation.LayoutTransition
import android.app.Activity
import android.app.Application
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.ContentProvider
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.util.Log
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.widget.EditText
import android.widget.FrameLayout
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
import kotlin.math.cos
import kotlin.math.sin
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
        if (Preferences.watching(this)) {
            Sentry.arm(this)
        }
    }
}

object Sentry {

    const val JOB_ID: Int = 0x0117

    const val INTERVAL_MILLIS: Long = 15 * 60 * 1000L

    private const val FILE = "omni_sentry"
    private const val REFUSED = "refused"
    private const val LAST_STANDING = "standing"
    private const val LAST_CHECKED = "checked"

    private fun store(context: Context) =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun arm(context: Context) {
        val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
        val job = JobInfo.Builder(JOB_ID, ComponentName(context, SentryService::class.java))
            .setPeriodic(INTERVAL_MILLIS)
            .setRequiresDeviceIdle(false)
            .setRequiresCharging(false)
            .build()
        val outcome = runCatching { scheduler.schedule(job) }.getOrDefault(JobScheduler.RESULT_FAILURE)
        OmniLog.event(
            LogLevel.INFO,
            "sentry",
            if (outcome == JobScheduler.RESULT_SUCCESS) {
                "Watch armed at ${INTERVAL_MILLIS / 60000} minute intervals."
            } else {
                "Android refused to schedule the watch; the check at every start still runs."
            },
        )
    }

    fun check(context: Context): String {
        if (Builder.load() !is Builder.LoadState.Loaded) {
            return "UNKNOWN"
        }
        val expected = context.getString(R.string.omni_expected_certificate).ifEmpty { null }
        val standing = runCatching {
            SelfCheck.parse(
                Builder.nativeVerifySelf(context.applicationInfo.sourceDir, expected)
            ).standing
        }.getOrDefault("UNKNOWN")

        store(context).edit()
            .putString(LAST_STANDING, standing)
            .putLong(LAST_CHECKED, System.currentTimeMillis())
            .apply()

        if (standing == "TAMPERED") {
            store(context).edit().putBoolean(REFUSED, true).apply()
            OmniLog.event(
                LogLevel.ERROR,
                "sentry",
                "The signature on this package is not the one it was built with.",
            )
        }
        return standing
    }

    fun disarm(context: Context) {
        val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
        runCatching { scheduler.cancel(JOB_ID) }
        OmniLog.event(LogLevel.INFO, "sentry", "Watch stopped; the check at every start remains.")
    }

    fun refused(context: Context): Boolean = store(context).getBoolean(REFUSED, false)

    fun lastStanding(context: Context): String =
        store(context).getString(LAST_STANDING, "").orEmpty()

    fun lastChecked(context: Context): Long = store(context).getLong(LAST_CHECKED, 0L)
}

class PackageProvider : ContentProvider() {

    companion object {
        const val PACKAGE_TYPE: String = "application/vnd.android.package-archive"
        const val BUNDLE_TYPE: String = "application/octet-stream"
        const val FOLDER: String = "Built"

        fun authority(context: Context): String = "${context.packageName}.packages"

        fun folder(context: Context): File =
            File(context.getExternalFilesDir(null) ?: context.filesDir, FOLDER)

        fun uriFor(context: Context, file: File): Uri = Uri.Builder()
            .scheme("content")
            .authority(authority(context))
            .appendPath(file.name)
            .build()
    }

    override fun onCreate(): Boolean = true

    private fun resolve(uri: Uri): File? {
        val here = context ?: return null
        val name = uri.lastPathSegment ?: return null
        if (name.isEmpty() || name.contains('/') || name.contains('\\') || name.contains("..")) {
            return null
        }
        val root = folder(here)
        val file = File(root, name)
        val inside = runCatching {
            file.canonicalPath == File(root.canonicalPath, name).canonicalPath
        }.getOrDefault(false)
        return if (inside && file.isFile) file else null
    }

    override fun getType(uri: Uri): String =
        if (uri.lastPathSegment?.endsWith(".apk") == true) PACKAGE_TYPE else BUNDLE_TYPE

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor? {
        if (mode != "r") {
            return null
        }
        val file = resolve(uri) ?: return null
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? {
        val file = resolve(uri) ?: return null
        val columns = projection ?: arrayOf(
            OpenableColumns.DISPLAY_NAME,
            OpenableColumns.SIZE,
        )
        val cursor = MatrixCursor(columns, 1)
        val row = arrayOfNulls<Any>(columns.size)
        columns.forEachIndexed { index, column ->
            row[index] = when (column) {
                OpenableColumns.DISPLAY_NAME -> file.name
                OpenableColumns.SIZE -> file.length()
                else -> null
            }
        }
        cursor.addRow(row)
        return cursor
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0
}

class SentryService : JobService() {

    override fun onStartJob(params: JobParameters?): Boolean {
        if (params == null) {
            runCatching { Sentry.check(applicationContext) }
            return false
        }
        Thread {
            runCatching { Sentry.check(applicationContext) }
            jobFinished(params, false)
        }.start()
        return true
    }

    override fun onStopJob(params: JobParameters?): Boolean = true
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

    external fun nativeBuildAll(
        root: String,
        packagePath: String,
        bundlePath: String,
        keyPath: String,
        keyPassword: CharArray?,
    ): String

    external fun nativeVerifySelf(packagePath: String, expectedCertificate: String?): String

    external fun nativeCreateKey(directory: String, spec: String, keyPassword: CharArray): String

    external fun nativeDefaultKey(directory: String): String

    external fun nativeListKeys(directory: String): String

    external fun nativeDeleteKey(path: String): String

    external fun nativeCheckKey(path: String, keyPassword: CharArray): String

    external fun nativeListProjects(directory: String): String

    external fun nativeProjectTree(root: String): String

    external fun nativeReadFile(root: String, relative: String): String

    external fun nativeWriteFile(root: String, relative: String, contents: String): String

    external fun nativeNewFolder(root: String, relative: String): String

    external fun nativeRemovePath(root: String, relative: String, trashRoot: String): String

    external fun nativeRenamePath(root: String, from: String, to: String): String

    external fun nativeListBuilt(directory: String): String

    external fun nativeTrashSend(trashRoot: String, path: String): String

    external fun nativeTrashList(trashRoot: String): String

    external fun nativeTrashRestore(trashRoot: String, id: String): String

    external fun nativeTrashPurge(trashRoot: String, id: String): String

    external fun nativeTrashEmpty(trashRoot: String): String

    external fun nativeSetIcon(root: String, source: String): String

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
    val versionName: String,
    val versionCode: Int,
    val abis: List<String>,
    val minSdk: Int,
    val targetSdk: Int,
    val languages: List<String>,
    val locales: List<String>,
) {
    fun encode(): String = buildString {
        append("package=").append(packageName).append(';')
        append("label=").append(label).append(';')
        append("versionName=").append(versionName).append(';')
        append("versionCode=").append(versionCode).append(';')
        append("abis=").append(abis.joinToString(",")).append(';')
        append("minSdk=").append(minSdk).append(';')
        append("targetSdk=").append(targetSdk).append(';')
        append("languages=").append(languages.joinToString(",")).append(';')
        append("locales=").append(locales.joinToString(","))
    }
}

data class KeySpec(
    val alias: String,
    val commonName: String,
    val organisation: String,
    val country: String,
    val validityYears: Int,
    val bits: Int,
) {
    fun encode(): String = buildString {
        append("alias=").append(alias).append(';')
        append("commonName=").append(commonName).append(';')
        append("organisation=").append(organisation).append(';')
        append("country=").append(country).append(';')
        append("validityYears=").append(validityYears).append(';')
        append("bits=").append(bits)
    }
}

data class Built(
    val name: String,
    val path: String,
    val bytes: Long,
    val writtenAt: Long,
    val bundle: Boolean,
) {
    companion object {
        fun list(document: String): List<Built> {
            val array = JSONObject(document).optJSONArray("built") ?: return emptyList()
            return (0 until array.length()).map { index ->
                val item = array.getJSONObject(index)
                Built(
                    name = item.getString("name"),
                    path = item.getString("path"),
                    bytes = item.optLong("bytes"),
                    writtenAt = item.optLong("writtenAt"),
                    bundle = item.optBoolean("bundle", false),
                )
            }
        }
    }
}

data class Trashed(
    val id: String,
    val name: String,
    val origin: String,
    val folder: Boolean,
    val bytes: Long,
    val secondsLeft: Long,
    val restorable: Boolean,
) {
    companion object {
        fun parse(item: JSONObject) = Trashed(
            id = item.getString("id"),
            name = item.getString("name"),
            origin = item.optString("origin"),
            folder = item.optBoolean("folder", false),
            bytes = item.optLong("bytes"),
            secondsLeft = item.optLong("secondsLeft"),
            restorable = item.optBoolean("restorable", false),
        )

        fun list(document: String): List<Trashed> {
            val array = JSONObject(document).optJSONArray("trashed") ?: return emptyList()
            return (0 until array.length()).map { parse(array.getJSONObject(it)) }
        }
    }
}

data class Refusal(
    val code: String?,
    val message: String?,
    val suggestion: String?,
    val context: List<String>,
) {
    companion object {
        fun parse(root: JSONObject): Refusal {
            val context = mutableListOf<String>()
            root.optJSONArray("context")?.let { array ->
                for (index in 0 until array.length()) context.add(array.getString(index))
            }
            return Refusal(
                code = root.optString("code").ifEmpty { null },
                message = root.optString("error").ifEmpty { null },
                suggestion = root.optString("suggestion").ifEmpty { null },
                context = context,
            )
        }
    }
}

data class SigningKey(
    val alias: String,
    val path: String,
    val subject: String,
    val issued: String,
    val expires: String,
    val fingerprint: String,
    val bits: Int,
) {
    companion object {
        fun parse(item: JSONObject) = SigningKey(
            alias = item.getString("alias"),
            path = item.getString("path"),
            subject = item.optString("subject"),
            issued = item.optString("issued"),
            expires = item.optString("expires"),
            fingerprint = item.optString("fingerprint"),
            bits = item.optInt("bits"),
        )

        fun list(document: String): List<SigningKey> {
            val array = JSONObject(document).optJSONArray("keys") ?: return emptyList()
            return (0 until array.length()).map { parse(array.getJSONObject(it)) }
        }
    }
}

data class ProjectSummary(
    val name: String,
    val root: String,
    val packageName: String,
    val label: String,
    val versionName: String,
    val versionCode: Int,
    val minSdk: Int,
    val targetSdk: Int,
    val files: Int,
    val icon: String?,
) {
    companion object {
        fun parse(item: JSONObject): ProjectSummary {
            val image = item.optJSONObject("icon")
            return ProjectSummary(
                name = item.getString("name"),
                root = item.getString("root"),
                packageName = item.optString("package"),
                label = item.optString("label"),
                versionName = item.optString("versionName"),
                versionCode = item.optInt("versionCode"),
                minSdk = item.optInt("minSdk"),
                targetSdk = item.optInt("targetSdk"),
                files = item.optInt("files"),
                icon = image?.let { "${it.optInt("width")}x${it.optInt("height")} ${it.optString("colour")}" },
            )
        }

        fun list(document: String): List<ProjectSummary> {
            val array = JSONObject(document).optJSONArray("projects") ?: return emptyList()
            return (0 until array.length()).map { parse(array.getJSONObject(it)) }
        }
    }
}

data class FileEntry(val path: String, val folder: Boolean, val bytes: Long) {
    companion object {
        fun list(document: String): List<FileEntry> {
            val array = JSONObject(document).optJSONArray("entries") ?: return emptyList()
            return (0 until array.length()).map { index ->
                val item = array.getJSONObject(index)
                FileEntry(
                    path = item.getString("path"),
                    folder = item.optBoolean("folder", false),
                    bytes = item.optLong("bytes"),
                )
            }
        }
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
    val developerKey: Boolean,
    val signedBy: String?,
    val bundlePath: String?,
    val bundleBytes: Long,
    val locales: Long,
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
                developerKey = root.optBoolean("developerKey", false),
                signedBy = root.optJSONObject("signedBy")?.optString("subject")?.ifEmpty { null },
                bundlePath = root.optString("bundlePath").ifEmpty { null },
                bundleBytes = root.optJSONObject("bundle")?.optLong("bytes") ?: 0L,
                locales = root.optLong("locales"),
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

data class CoreState(
    val version: String,
    val status: String,
    val abiVersion: Int,
    val selfHosted: Boolean,
    val subsystems: Int,
    val toolchainVerified: Int,
    val toolchainTotal: Int,
) {
    companion object {
        fun parse(document: String): CoreState {
            val root = JSONObject(document)
            val core = root.getJSONObject("core")
            val toolchain = root.getJSONObject("toolchain")
            return CoreState(
                version = core.getString("version"),
                status = core.getString("status"),
                abiVersion = core.getInt("abiVersion"),
                selfHosted = core.getBoolean("selfHosted"),
                subsystems = root.getJSONObject("subsystems").getInt("count"),
                toolchainVerified = toolchain.getInt("verified"),
                toolchainTotal = toolchain.getJSONArray("components").length(),
            )
        }
    }
}

data class Palette(
    val key: String,
    val label: String,
    val background: Int,
    val surface: Int,
    val raised: Int,
    val foreground: Int,
    val muted: Int,
    val accent: Int,
    val ok: Int,
    val warning: Int,
    val error: Int,
    val divider: Int,
    val glowFirst: Int,
    val glowSecond: Int,
    val glowThird: Int,
) {
    companion object {
        val MIDNIGHT = Palette(
            key = "midnight",
            label = "Midnight",
            background = 0xFF05070B.toInt(),
            surface = 0xFF0D1119.toInt(),
            raised = 0xFF141A25.toInt(),
            foreground = 0xFFE8EEF6.toInt(),
            muted = 0xFF8A94A6.toInt(),
            accent = 0xFF6CB6FF.toInt(),
            ok = 0xFF5BD48A.toInt(),
            warning = 0xFFE9B44C.toInt(),
            error = 0xFFE5534B.toInt(),
            divider = 0xFF1C2432.toInt(),
            glowFirst = 0xFF1B4B8F.toInt(),
            glowSecond = 0xFF0E5C52.toInt(),
            glowThird = 0xFF3A2166.toInt(),
        )

        val SLATE = Palette(
            key = "slate",
            label = "Slate",
            background = 0xFF0E1116.toInt(),
            surface = 0xFF171C24.toInt(),
            raised = 0xFF202733.toInt(),
            foreground = 0xFFE6EDF3.toInt(),
            muted = 0xFF9198A1.toInt(),
            accent = 0xFF6CB6FF.toInt(),
            ok = 0xFF57C98A.toInt(),
            warning = 0xFFE3B341.toInt(),
            error = 0xFFE5534B.toInt(),
            divider = 0xFF262C36.toInt(),
            glowFirst = 0xFF23508C.toInt(),
            glowSecond = 0xFF135E55.toInt(),
            glowThird = 0xFF43286E.toInt(),
        )

        val DAYLIGHT = Palette(
            key = "daylight",
            label = "Daylight",
            background = 0xFFF6F8FB.toInt(),
            surface = 0xFFFFFFFF.toInt(),
            raised = 0xFFEDF1F7.toInt(),
            foreground = 0xFF121821.toInt(),
            muted = 0xFF5A6472.toInt(),
            accent = 0xFF1A6FD4.toInt(),
            ok = 0xFF1E8E4F.toInt(),
            warning = 0xFF9A6A00.toInt(),
            error = 0xFFC0362C.toInt(),
            divider = 0xFFD8DFE9.toInt(),
            glowFirst = 0xFFBBD5F5.toInt(),
            glowSecond = 0xFFBEE6DD.toInt(),
            glowThird = 0xFFD6C9F0.toInt(),
        )

        val ALL: List<Palette> = listOf(MIDNIGHT, SLATE, DAYLIGHT)

        fun of(key: String): Palette = ALL.firstOrNull { it.key == key } ?: MIDNIGHT
    }
}

object Preferences {

    private const val FILE = "omni_settings"
    private const val LANGUAGE = "language"
    private const val THEME = "theme"
    private const val SIGNING_KEY = "signing_key"
    private const val WATCH = "watch"
    private const val SHARE_LOG = "share_log"
    private const val ABI = "abi"
    private const val MIN_SDK = "min_sdk"
    private const val TARGET_SDK = "target_sdk"

    val LANGUAGES: List<Pair<String, String>> = listOf(
        "en" to "English",
        "tr" to "Türkçe",
        "de" to "Deutsch",
        "es" to "Español",
        "fr" to "Français",
        "it" to "Italiano",
        "pt" to "Português",
        "ru" to "Русский",
        "ar" to "العربية",
        "zh" to "中文",
    )

    private fun store(context: Context) =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun language(context: Context): String = store(context).getString(LANGUAGE, "").orEmpty()

    fun setLanguage(context: Context, tag: String) {
        store(context).edit().putString(LANGUAGE, tag).apply()
    }

    fun palette(context: Context): Palette =
        Palette.of(store(context).getString(THEME, Palette.MIDNIGHT.key).orEmpty())

    fun setPalette(context: Context, key: String) {
        store(context).edit().putString(THEME, key).apply()
    }

    fun signingKey(context: Context): String = store(context).getString(SIGNING_KEY, "").orEmpty()

    fun setSigningKey(context: Context, path: String) {
        store(context).edit().putString(SIGNING_KEY, path).apply()
    }

    fun watching(context: Context): Boolean = store(context).getBoolean(WATCH, true)

    fun setWatching(context: Context, on: Boolean) {
        store(context).edit().putBoolean(WATCH, on).apply()
    }

    fun sharingLog(context: Context): Boolean = store(context).getBoolean(SHARE_LOG, true)

    fun setSharingLog(context: Context, on: Boolean) {
        store(context).edit().putBoolean(SHARE_LOG, on).apply()
    }

    fun abi(context: Context): Int = store(context).getInt(ABI, 2)

    fun setAbi(context: Context, index: Int) {
        store(context).edit().putInt(ABI, index).apply()
    }

    fun minSdk(context: Context): Int = store(context).getInt(MIN_SDK, 28)

    fun setMinSdk(context: Context, level: Int) {
        store(context).edit().putInt(MIN_SDK, level).apply()
    }

    fun targetSdk(context: Context): Int = store(context).getInt(TARGET_SDK, 36)

    fun setTargetSdk(context: Context, level: Int) {
        store(context).edit().putInt(TARGET_SDK, level).apply()
    }
}

class AuroraView(context: Context, private var palette: Palette) : View(context) {

    private companion object {
        const val PERIOD_MILLIS = 24_000f
        const val BLOB_ALPHA = 190
        const val FRAME_MILLIS = 32L
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val started = SystemClock.uptimeMillis()
    private var running = false

    private val step = object : Runnable {
        override fun run() {
            if (!running) {
                return
            }
            invalidate()
            postDelayed(this, FRAME_MILLIS)
        }
    }

    fun repaint(next: Palette) {
        palette = next
        invalidate()
    }

    fun resumeDrawing() {
        if (running) {
            return
        }
        running = true
        post(step)
    }

    fun pauseDrawing() {
        running = false
        removeCallbacks(step)
    }

    override fun onDetachedFromWindow() {
        pauseDrawing()
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0f || h <= 0f) {
            return
        }
        canvas.drawColor(palette.background)

        val phase = ((SystemClock.uptimeMillis() - started) % PERIOD_MILLIS.toLong()) /
            PERIOD_MILLIS
        val turn = (phase * 2.0 * Math.PI).toFloat()
        val radius = maxOf(w, h) * 0.72f

        blob(canvas, w * (0.22f + 0.16f * sin(turn)), h * (0.18f + 0.10f * cos(turn * 0.8f)),
            radius, palette.glowFirst)
        blob(canvas, w * (0.82f + 0.12f * cos(turn * 1.3f)), h * (0.34f + 0.12f * sin(turn * 1.1f)),
            radius * 0.85f, palette.glowSecond)
        blob(canvas, w * (0.48f + 0.20f * sin(turn * 0.7f + 1.4f)),
            h * (0.86f + 0.08f * cos(turn * 0.9f)), radius * 0.95f, palette.glowThird)

        paint.shader = LinearGradient(
            0f, 0f, 0f, h,
            Color.argb(0, Color.red(palette.background), Color.green(palette.background),
                Color.blue(palette.background)),
            Color.argb(210, Color.red(palette.background), Color.green(palette.background),
                Color.blue(palette.background)),
            Shader.TileMode.CLAMP,
        )
        paint.alpha = 255
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
    }

    private fun blob(canvas: Canvas, x: Float, y: Float, radius: Float, colour: Int) {
        paint.shader = RadialGradient(
            x, y, radius,
            Color.argb(BLOB_ALPHA, Color.red(colour), Color.green(colour), Color.blue(colour)),
            Color.argb(0, Color.red(colour), Color.green(colour), Color.blue(colour)),
            Shader.TileMode.CLAMP,
        )
        canvas.drawCircle(x, y, radius, paint)
        paint.shader = null
    }
}

private enum class Tab { PROJECTS, FILES, BUILD, TRASH, SETTINGS }

private sealed interface Screen {
    val tab: Tab

    data object Projects : Screen {
        override val tab = Tab.PROJECTS
    }

    data object NewProject : Screen {
        override val tab = Tab.PROJECTS
    }

    data class Files(val root: String, val folder: String) : Screen {
        override val tab = Tab.FILES
    }

    data class Editor(val root: String, val path: String) : Screen {
        override val tab = Tab.FILES
    }

    data class Build(val root: String) : Screen {
        override val tab = Tab.BUILD
    }

    data object Trash : Screen {
        override val tab = Tab.TRASH
    }

    data object Settings : Screen {
        override val tab = Tab.SETTINGS
    }

    data object Keys : Screen {
        override val tab = Tab.SETTINGS
    }

    data object NewKey : Screen {
        override val tab = Tab.SETTINGS
    }
}
class BuilderActivity : Activity() {

    private companion object {
        const val STORAGE_PERMISSION_REQUEST = 1
        const val IMAGE_REQUEST = 2
        const val ENTER_MILLIS = 260L
        const val LEAVE_MILLIS = 140L
        const val RISE_DP = 18

        const val DEFAULT_PACKAGE = "com.my.app"
        const val DEFAULT_LABEL = "My App"
        const val BASE_LOCALE = "en"

        const val DEFAULT_KEY_ALIAS = "My_Key"
        const val DEFAULT_KEY_COMMON_NAME = "Builder"
        const val DEFAULT_KEY_ORGANISATION = "My_App"
        const val DEFAULT_KEY_COUNTRY = "EN"
        const val DEFAULT_KEY_YEARS = 10
        const val DEFAULT_KEY_PASSWORD = "My_App_Builder"

        val ABI_CHOICES = listOf(
            "32" to listOf("armeabi-v7a"),
            "64" to listOf("arm64-v8a"),
            "32 + 64" to listOf("armeabi-v7a", "arm64-v8a"),
        )
        val ANDROID_RELEASES = listOf(
            28 to "9", 29 to "10", 30 to "11", 31 to "12", 32 to "12L",
            33 to "13", 34 to "14", 35 to "15", 36 to "16",
        )
        val LANGUAGE_CHOICES = listOf(
            "kotlin" to "Kotlin",
            "java" to "Java",
            "cpp" to "C++",
            "rust" to "Rust",
        )
        val KEY_SIZES = listOf(2048, 3072, 4096)
        val VALIDITY_YEARS = listOf(1, 5, 10, 25, 100)
    }

    private lateinit var palette: Palette
    private lateinit var aurora: AuroraView
    private lateinit var bar: LinearLayout
    private lateinit var scroller: ScrollView
    private lateinit var content: LinearLayout
    private lateinit var results: LinearLayout

    private var screen: Screen = Screen.Projects
    private var standing = "UNKNOWN"
    private var openProject: String? = null

    private var formPackage = DEFAULT_PACKAGE
    private var formLabel = DEFAULT_LABEL
    private var formVersionName = "1.0.0"
    private var formVersionCode = "1"
    private var formAbi = 2
    private var formMinSdk = 28
    private var formTargetSdk = 36
    private val formLanguages = linkedSetOf("kotlin")
    private val formLocales = Preferences.LANGUAGES.mapTo(linkedSetOf()) { it.first }
    private var formImage: String? = null

    private var keyAlias = DEFAULT_KEY_ALIAS
    private var keyCommonName = DEFAULT_KEY_COMMON_NAME
    private var keyOrganisation = DEFAULT_KEY_ORGANISATION
    private var keyCountry = DEFAULT_KEY_COUNTRY
    private var keyYears = VALIDITY_YEARS.indexOf(DEFAULT_KEY_YEARS)
    private var keyBits = KEY_SIZES.indexOf(4096)
    private var keyPasswordView: EditText? = null
    private var keyPasswordAgainView: EditText? = null
    private var buildPasswordView: EditText? = null

    private var editorText = ""
    private var newPathView: EditText? = null
    private var renameFromView: EditText? = null
    private var renameToView: EditText? = null

    override fun attachBaseContext(base: Context) {
        val tag = Preferences.language(base)
        super.attachBaseContext(if (tag.isEmpty()) base else localised(base, tag))
    }

    private fun localised(base: Context, tag: String): Context {
        val locale = Locale.forLanguageTag(tag)
        val configuration = Configuration(base.resources.configuration)
        configuration.setLocale(locale)
        configuration.setLayoutDirection(locale)
        return base.createConfigurationContext(configuration)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity created.")
        requestLegacyStoragePermissionIfNeeded()

        palette = Preferences.palette(this)
        formAbi = Preferences.abi(this)
        formMinSdk = Preferences.minSdk(this)
        formTargetSdk = Preferences.targetSdk(this)

        aurora = AuroraView(this, palette)
        content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(gap(4), gap(3), gap(4), gap(8))
        }
        results = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutTransition = LayoutTransition()
        }
        scroller = ScrollView(this).apply {
            isFillViewport = true
            clipToPadding = false
            overScrollMode = View.OVER_SCROLL_IF_CONTENT_SCROLLS
            addView(content, MATCH_PARENT, WRAP_CONTENT)
        }

        bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            background = sheet(palette.surface, palette.divider)
            layoutTransition = LayoutTransition()
        }

        val roof = View(this)
        val shell = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(roof, LinearLayout.LayoutParams(MATCH_PARENT, 0))
            addView(scroller, LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f))
            addView(bar, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        }

        val stack = FrameLayout(this).apply {
            setBackgroundColor(palette.background)
            addView(aurora, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(shell, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        }
        setContentView(stack)
        fitAroundTheSystemBars(stack, roof)
        hideTheNavigationBar()

        standing = examine()
        provisionSharedKey()
        render(false)
    }

    private fun hideTheNavigationBar() {
        val pale = Color.luminance(palette.background) > 0.5f
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) {
                @Suppress("DEPRECATION")
                window.setDecorFitsSystemWindows(false)
            }
            window.insetsController?.let { controller ->
                controller.hide(WindowInsets.Type.navigationBars())
                controller.systemBarsBehavior =
                    WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                controller.setSystemBarsAppearance(
                    if (pale) WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS else 0,
                    WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS,
                )
            }
            return
        }
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility =
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                if (pale) View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR else 0
    }

    private fun fitAroundTheSystemBars(stack: View, roof: View) {
        stack.setOnApplyWindowInsetsListener { _, insets ->
            val top: Int
            val bottom: Int
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val around = insets.getInsets(
                    WindowInsets.Type.statusBars() or WindowInsets.Type.displayCutout()
                )
                top = around.top
                bottom = insets.getInsets(WindowInsets.Type.navigationBars()).bottom
            } else {
                @Suppress("DEPRECATION")
                top = insets.systemWindowInsetTop
                @Suppress("DEPRECATION")
                bottom = insets.systemWindowInsetBottom
            }
            val above = roof.layoutParams as LinearLayout.LayoutParams
            if (above.height != top) {
                above.height = top
                roof.layoutParams = above
            }
            bar.setPadding(gap(2), gap(2), gap(2), gap(2) + bottom)
            insets
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            hideTheNavigationBar()
        }
    }

    private fun provisionSharedKey() {
        if (Preferences.signingKey(this).isNotEmpty()) {
            return
        }
        val folder = keysFolder().absolutePath
        Thread {
            val answer = runCatching { Builder.nativeDefaultKey(folder) }.getOrNull()
            val key = answer
                ?.let { runCatching { JSONObject(it) }.getOrNull() }
                ?.takeIf { it.optBoolean("ready", false) }
                ?.optJSONObject("key")
                ?: return@Thread
            runOnUiThread {
                if (isFinishing) {
                    return@runOnUiThread
                }
                Preferences.setSigningKey(this, key.optString("path"))
                OmniLog.event(
                    LogLevel.INFO,
                    "keystore",
                    "Shared key ${key.optString("alias")} ready, ${key.optString("fingerprint")}",
                )
                render(false)
            }
        }.start()
    }

    override fun onResume() {
        super.onResume()
        aurora.resumeDrawing()
        if (Sentry.refused(this)) {
            if (standing != "TAMPERED") {
                standing = "TAMPERED"
                render(false)
            }
            return
        }
        Thread {
            val found = runCatching { Sentry.check(this) }.getOrDefault("UNKNOWN")
            runOnUiThread {
                if (!isFinishing && found != standing) {
                    standing = found
                    render(false)
                }
            }
        }.start()
    }

    override fun onPause() {
        aurora.pauseDrawing()
        super.onPause()
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        OmniLog.setPublishListener(null)
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun examine(): String =
        if (Sentry.refused(this)) "TAMPERED" else Sentry.check(this)

    private fun go(next: Screen) {
        if (next == screen) {
            return
        }
        screen = next
        content.animate()
            .alpha(0f)
            .translationY(-gap(RISE_DP / 6).toFloat())
            .setDuration(LEAVE_MILLIS)
            .setInterpolator(AccelerateInterpolator())
            .withEndAction {
                render(true)
                scroller.scrollTo(0, 0)
            }
            .start()
    }

    private fun render(animated: Boolean) {
        content.removeAllViews()
        results.removeAllViews()

        when (val load = Builder.load()) {
            is Builder.LoadState.Failed -> {
                content.addView(notice(load.reason, palette.error))
                content.alpha = 1f
                content.translationY = 0f
                return
            }
            is Builder.LoadState.Loaded -> Unit
        }

        if (standing == "TAMPERED") {
            bar.removeAllViews()
            bar.visibility = View.GONE
            content.addView(notice(getString(R.string.omni_integrity_refused_title), palette.error))
            content.addView(body(getString(R.string.omni_integrity_refused_body)))
            content.addView(quiet(getString(R.string.omni_integrity_checked)))
            content.alpha = 1f
            content.translationY = 0f
            return
        }

        bar.visibility = View.VISIBLE
        drawBar()

        when (val here = screen) {
            is Screen.Projects -> renderProjects()
            is Screen.NewProject -> renderNewProject()
            is Screen.Files -> renderFiles(here.root, here.folder)
            is Screen.Editor -> renderEditor(here.root, here.path)
            is Screen.Build -> renderBuild(here.root)
            is Screen.Trash -> renderTrash()
            is Screen.Settings -> renderSettings()
            is Screen.Keys -> renderKeys()
            is Screen.NewKey -> renderNewKey()
        }
        content.addView(results)

        if (animated) {
            content.alpha = 0f
            content.translationY = gap(RISE_DP / 6).toFloat()
            content.animate()
                .alpha(1f)
                .translationY(0f)
                .setDuration(ENTER_MILLIS)
                .setInterpolator(DecelerateInterpolator())
                .start()
        } else {
            content.alpha = 1f
            content.translationY = 0f
        }
    }

    @Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
    override fun onBackPressed() {
        when (val here = screen) {
            is Screen.Projects -> super.onBackPressed()
            is Screen.NewProject -> go(Screen.Projects)
            is Screen.Editor -> go(Screen.Files(here.root, here.path.substringBeforeLast('/', "")))
            is Screen.Files ->
                if (here.folder.isEmpty()) {
                    go(Screen.Projects)
                } else {
                    go(Screen.Files(here.root, here.folder.substringBeforeLast('/', "")))
                }
            is Screen.Build -> go(Screen.Files(here.root, ""))
            is Screen.Keys -> go(Screen.Settings)
            is Screen.NewKey -> go(Screen.Keys)
            else -> go(Screen.Projects)
        }
    }

    private fun destination(tab: Tab): Screen {
        val root = openProject
        return when (tab) {
            Tab.PROJECTS -> Screen.Projects
            Tab.FILES -> root?.let { Screen.Files(it, "") } ?: Screen.Projects
            Tab.BUILD -> root?.let { Screen.Build(it) } ?: Screen.Projects
            Tab.TRASH -> Screen.Trash
            Tab.SETTINGS -> Screen.Settings
        }
    }

    private fun drawBar() {
        bar.removeAllViews()
        val labels = mapOf(
            Tab.PROJECTS to R.string.omni_tab_projects,
            Tab.FILES to R.string.omni_tab_files,
            Tab.BUILD to R.string.omni_tab_build,
            Tab.TRASH to R.string.omni_tab_trash,
            Tab.SETTINGS to R.string.omni_tab_settings,
        )
        for (tab in Tab.entries) {
            val active = screen.tab == tab
            val reachable = tab != Tab.FILES && tab != Tab.BUILD || openProject != null
            bar.addView(
                TextView(this).apply {
                    text = getString(labels.getValue(tab))
                    setTextColor(
                        when {
                            active -> palette.background
                            reachable -> palette.foreground
                            else -> palette.muted
                        }
                    )
                    setTypeface(Typeface.DEFAULT_BOLD)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                    letterSpacing = 0.04f
                    gravity = Gravity.CENTER
                    maxLines = 1
                    setPadding(gap(1), gap(3), gap(1), gap(3))
                    background = touchable(
                        pill(if (active) palette.accent else Color.TRANSPARENT, gap(3).toFloat()),
                        palette.accent,
                    )
                    isClickable = true
                    setOnClickListener {
                        if (reachable) {
                            go(destination(tab))
                        } else {
                            go(Screen.Projects)
                            results.addView(
                                notice(getString(R.string.omni_no_project_open), palette.warning)
                            )
                        }
                    }
                },
                LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply {
                    marginStart = gap(1) / 2
                    marginEnd = gap(1) / 2
                },
            )
        }
    }

    private fun projectsFolder() = File(getExternalFilesDir(null) ?: filesDir, "Projects")

    private fun builtFolder() = PackageProvider.folder(this).also { it.mkdirs() }

    private fun trashFolder() = File(getExternalFilesDir(null) ?: filesDir, "Trash").absolutePath

    private fun openHere(root: String) {
        openProject = root
        go(Screen.Files(root, ""))
    }

    private fun keysFolder() = File(filesDir, "Keys")

    private fun renderProjects() {
        content.addView(heading(getString(R.string.omni_projects_title)))

        val projects = runCatching {
            ProjectSummary.list(Builder.nativeListProjects(projectsFolder().absolutePath))
        }.getOrDefault(emptyList())

        val card = card()
        if (projects.isEmpty()) {
            card.addView(quiet(getString(R.string.omni_projects_none)))
        }
        projects.forEachIndexed { index, project ->
            if (index > 0) {
                card.addView(rule(), MATCH_PARENT, 1)
            }
            val open = project.root == openProject
            card.addView(
                row(
                    project.label.ifEmpty { project.name },
                    "${project.packageName}  ·  ${project.versionName}  ·  " +
                        "API ${project.minSdk}–${project.targetSdk}  ·  " +
                        getString(R.string.omni_projects_files, project.files),
                    if (open) getString(R.string.omni_projects_open_now) else "",
                    palette.ok,
                ) { openHere(project.root) }
            )
            card.addView(
                subtle(getString(R.string.omni_action_delete), palette.error) {
                    deleteProject(project)
                }
            )
        }
        content.addView(card)
        content.addView(primary(getString(R.string.omni_projects_new)) { go(Screen.NewProject) })
        content.addView(quiet(getString(R.string.omni_projects_note)))
    }

    private fun renderNewProject() {
        content.addView(heading(getString(R.string.omni_projects_new)))

        val identity = card()
        identity.addView(field(getString(R.string.omni_form_package), formPackage) { formPackage = it })
        identity.addView(field(getString(R.string.omni_form_label), formLabel) { formLabel = it })
        identity.addView(
            field(getString(R.string.omni_form_version_name), formVersionName) { formVersionName = it }
        )
        identity.addView(
            field(getString(R.string.omni_form_version_code), formVersionCode) { formVersionCode = it }
        )
        content.addView(identity)

        content.addView(label(getString(R.string.omni_form_architecture)))
        content.addView(chips(ABI_CHOICES.map { it.first }, { it == formAbi }) { formAbi = it })

        content.addView(label(getString(R.string.omni_form_min_sdk)))
        content.addView(
            chips(ANDROID_RELEASES.map { it.second }, { ANDROID_RELEASES[it].first == formMinSdk }) {
                formMinSdk = ANDROID_RELEASES[it].first
                if (formTargetSdk < formMinSdk) formTargetSdk = formMinSdk
            }
        )

        content.addView(label(getString(R.string.omni_form_target_sdk)))
        content.addView(
            chips(ANDROID_RELEASES.map { it.second }, { ANDROID_RELEASES[it].first == formTargetSdk }) {
                formTargetSdk = ANDROID_RELEASES[it].first
                if (formMinSdk > formTargetSdk) formMinSdk = formTargetSdk
            }
        )

        content.addView(label(getString(R.string.omni_form_languages)))
        content.addView(
            chips(
                LANGUAGE_CHOICES.map { it.second },
                { formLanguages.contains(LANGUAGE_CHOICES[it].first) },
            ) { index ->
                val key = LANGUAGE_CHOICES[index].first
                if (!formLanguages.remove(key)) formLanguages.add(key)
            }
        )
        content.addView(quiet(getString(R.string.omni_form_no_compiler)))

        content.addView(label(getString(R.string.omni_form_locales)))
        content.addView(
            chips(
                Preferences.LANGUAGES.map { it.second },
                { formLocales.contains(Preferences.LANGUAGES[it].first) },
            ) { index ->
                val tag = Preferences.LANGUAGES[index].first
                if (!formLocales.remove(tag)) formLocales.add(tag)
                if (formLocales.isEmpty()) formLocales.add(BASE_LOCALE)
            }
        )
        content.addView(quiet(getString(R.string.omni_form_locales_note)))

        content.addView(label(getString(R.string.omni_form_image)))
        val picture = card()
        picture.addView(quiet(formImage?.let { File(it).name } ?: getString(R.string.omni_form_image_none)))
        picture.addView(subtle(getString(R.string.omni_form_image_choose), palette.accent) { chooseImage() })
        content.addView(picture)
        content.addView(quiet(getString(R.string.omni_form_image_note)))

        content.addView(primary(getString(R.string.omni_action_create)) { createProject() })
        content.addView(subtle(getString(R.string.omni_action_cancel), palette.muted) {
            go(Screen.Projects)
        })
    }

    private fun renderFiles(root: String, folder: String) {
        val summary = summaryOf(root)
        content.addView(heading(summary?.label?.ifEmpty { null } ?: File(root).name))
        summary?.let {
            content.addView(
                quiet(
                    "${it.packageName}  ·  ${it.versionName} (${it.versionCode})  ·  " +
                        "API ${it.minSdk}–${it.targetSdk}"
                )
            )
        }

        content.addView(label(getString(R.string.omni_files_here)))
        content.addView(breadcrumb(root, folder))

        val entries = runCatching {
            FileEntry.list(Builder.nativeProjectTree(root))
        }.getOrDefault(emptyList())
        val here = entries
            .filter { it.path.substringBeforeLast('/', "") == folder }
            .sortedWith(compareBy({ !it.folder }, { it.path.lowercase(Locale.getDefault()) }))

        val tree = card()
        if (here.isEmpty()) {
            tree.addView(quiet(getString(R.string.omni_files_empty)))
        }
        here.forEachIndexed { index, entry ->
            if (index > 0) {
                tree.addView(rule(), MATCH_PARENT, 1)
            }
            val name = entry.path.substringAfterLast('/')
            val held = entries.count { it.path.startsWith("${entry.path}/") }
            tree.addView(
                row(
                    if (entry.folder) "$name/" else name,
                    if (entry.folder) {
                        getString(R.string.omni_files_holds, held)
                    } else {
                        size(entry.bytes)
                    },
                    if (entry.folder) getString(R.string.omni_action_open) else "",
                    palette.accent,
                ) {
                    if (entry.folder) {
                        go(Screen.Files(root, entry.path))
                    } else {
                        editorText = ""
                        go(Screen.Editor(root, entry.path))
                    }
                }
            )
            tree.addView(
                subtle(getString(R.string.omni_action_delete), palette.error) {
                    act(Builder.nativeRemovePath(root, entry.path, trashFolder()), "removed")
                }
            )
        }
        content.addView(tree)
        content.addView(quiet(getString(R.string.omni_trash_note)))

        content.addView(label(getString(R.string.omni_files_make)))
        val making = card()
        val prompt = input(getString(R.string.omni_editor_name_prompt), "")
        newPathView = prompt.second
        making.addView(prompt.first)
        making.addView(row(
            getString(R.string.omni_action_new_file),
            getString(R.string.omni_files_into, folderName(folder)),
            "",
            palette.accent,
        ) {
            named(folder)?.let { path ->
                act(Builder.nativeWriteFile(root, path, ""), "saved")
            }
        })
        making.addView(rule(), MATCH_PARENT, 1)
        making.addView(row(
            getString(R.string.omni_action_new_folder),
            getString(R.string.omni_files_into, folderName(folder)),
            "",
            palette.accent,
        ) {
            named(folder)?.let { path ->
                act(Builder.nativeNewFolder(root, path), "made")
            }
        })
        content.addView(making)

        content.addView(label(getString(R.string.omni_files_move)))
        val moving = card()
        val from = input(getString(R.string.omni_files_move_from), "")
        val to = input(getString(R.string.omni_files_move_to), "")
        renameFromView = from.second
        renameToView = to.second
        moving.addView(from.first)
        moving.addView(to.first)
        moving.addView(subtle(getString(R.string.omni_action_move), palette.accent) {
            val source = renameFromView?.text?.toString().orEmpty().trim()
            val target = renameToView?.text?.toString().orEmpty().trim()
            if (source.isEmpty() || target.isEmpty()) {
                results.removeAllViews()
                results.addView(notice(getString(R.string.omni_files_move_needs), palette.warning))
            } else {
                act(Builder.nativeRenamePath(root, source, target), "moved")
            }
        })
        content.addView(moving)
        content.addView(quiet(getString(R.string.omni_files_move_note)))

        content.addView(primary(getString(R.string.omni_action_build)) { go(Screen.Build(root)) })
    }

    private fun renderEditor(root: String, path: String) {
        content.addView(heading(path.substringAfterLast('/')))
        content.addView(quiet(path))

        val answer = runCatching { JSONObject(Builder.nativeReadFile(root, path)) }.getOrNull()
        if (answer == null || !answer.optBoolean("read", false)) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
                go(Screen.Files(root, path.substringBeforeLast('/', "")))
            })
            return
        }
        editorText = answer.optString("text")

        val sheet = card()
        sheet.addView(
            EditText(this).apply {
                setText(editorText)
                setTextColor(palette.foreground)
                setBackgroundColor(Color.TRANSPARENT)
                setTypeface(Typeface.MONOSPACE)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                gravity = Gravity.TOP or Gravity.START
                setHorizontallyScrolling(false)
                minLines = 14
                setPadding(gap(2))
                addTextChangedListener(object : TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                    override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                    override fun afterTextChanged(s: Editable?) {
                        editorText = s?.toString().orEmpty()
                    }
                })
            }
        )
        content.addView(sheet)

        content.addView(primary(getString(R.string.omni_action_save)) {
            results.removeAllViews()
            val saved = runCatching { JSONObject(Builder.nativeWriteFile(root, path, editorText)) }
                .getOrNull()
            if (saved != null && saved.optBoolean("saved", false)) {
                results.addView(notice(getString(R.string.omni_editor_saved, path), palette.ok))
            } else {
                saved?.let { showRefusal(Refusal.parse(it), results) }
            }
        })
        content.addView(subtle(getString(R.string.omni_action_delete), palette.error) {
            results.removeAllViews()
            val thrown = runCatching {
                JSONObject(Builder.nativeRemovePath(root, path, trashFolder()))
            }.getOrNull()
            if (thrown != null && thrown.optBoolean("removed", false)) {
                go(Screen.Files(root, path.substringBeforeLast('/', "")))
            } else {
                thrown?.let { showRefusal(Refusal.parse(it), results) }
            }
        })
        content.addView(quiet(getString(R.string.omni_trash_note)))
        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Files(root, path.substringBeforeLast('/', "")))
        })
    }

    private fun renderBuild(root: String) {
        val summary = summaryOf(root)
        content.addView(heading(getString(R.string.omni_build_title)))
        content.addView(
            quiet(summary?.label?.ifEmpty { null } ?: File(root).name)
        )

        val chosen = Preferences.signingKey(this)
        val keys = keysHere()
        val key = keys.firstOrNull { it.path == chosen }

        val signing = card()
        if (key == null) {
            signing.addView(quiet(getString(R.string.omni_build_no_key)))
        } else {
            val shared = key.alias == DEFAULT_KEY_ALIAS
            signing.addView(
                keyValue(
                    key.alias,
                    key.subject,
                    if (shared) {
                        getString(R.string.omni_keys_shared)
                    } else {
                        getString(R.string.omni_keys_yours)
                    },
                    palette.accent,
                )
            )
            val secret = secret(
                getString(R.string.omni_build_password),
                if (shared) DEFAULT_KEY_PASSWORD else "",
            )
            buildPasswordView = secret.second
            signing.addView(secret.first)
            signing.addView(
                quiet(
                    if (shared) {
                        getString(R.string.omni_build_shared_password, DEFAULT_KEY_PASSWORD)
                    } else {
                        getString(R.string.omni_build_your_password)
                    }
                )
            )
        }
        signing.addView(subtle(getString(R.string.omni_keys_title), palette.accent) {
            go(Screen.Keys)
        })
        content.addView(signing)

        content.addView(primary(getString(R.string.omni_action_build_both)) { buildProject(root) })
        content.addView(quiet(getString(R.string.omni_build_both_note)))

        content.addView(heading(getString(R.string.omni_built_title)))
        val made = runCatching {
            Built.list(Builder.nativeListBuilt(builtFolder().absolutePath))
        }.getOrDefault(emptyList())

        val shelf = card()
        if (made.isEmpty()) {
            shelf.addView(quiet(getString(R.string.omni_built_none)))
        }
        made.forEachIndexed { index, one ->
            if (index > 0) {
                shelf.addView(rule(), MATCH_PARENT, 1)
            }
            shelf.addView(
                row(
                    one.name,
                    "${size(one.bytes)}  ·  ${moment(one.writtenAt)}",
                    if (one.bundle) "AAB" else "APK",
                    if (one.bundle) palette.muted else palette.ok,
                ) { offer(File(one.path)) }
            )
            shelf.addView(
                subtle(getString(R.string.omni_action_delete), palette.error) {
                    act(Builder.nativeTrashSend(trashFolder(), one.path), "removed")
                }
            )
        }
        content.addView(shelf)
        content.addView(quiet(getString(R.string.omni_trash_note)))
    }

    private fun renderTrash() {
        content.addView(heading(getString(R.string.omni_trash_title)))

        val held = runCatching {
            Trashed.list(Builder.nativeTrashList(trashFolder()))
        }.getOrDefault(emptyList())

        val card = card()
        if (held.isEmpty()) {
            card.addView(quiet(getString(R.string.omni_trash_none)))
        }
        held.forEachIndexed { index, one ->
            if (index > 0) {
                card.addView(rule(), MATCH_PARENT, 1)
            }
            card.addView(
                keyValue(
                    if (one.folder) "${one.name}/" else one.name,
                    "${one.origin}\n${size(one.bytes)}",
                    left(one.secondsLeft),
                    if (one.secondsLeft < 3600) palette.warning else palette.muted,
                )
            )
            card.addView(
                row(
                    getString(R.string.omni_action_restore),
                    if (one.restorable) {
                        ""
                    } else {
                        getString(R.string.omni_trash_taken)
                    },
                    "",
                    palette.ok,
                ) {
                    act(Builder.nativeTrashRestore(trashFolder(), one.id), "restored")
                }
            )
            card.addView(
                subtle(getString(R.string.omni_action_purge), palette.error) {
                    act(Builder.nativeTrashPurge(trashFolder(), one.id), "purged")
                }
            )
        }
        content.addView(card)
        content.addView(quiet(getString(R.string.omni_trash_how)))

        if (held.isNotEmpty()) {
            content.addView(subtle(getString(R.string.omni_action_empty), palette.error) {
                results.removeAllViews()
                runCatching { Builder.nativeTrashEmpty(trashFolder()) }
                render(false)
            })
        }
    }

    private fun renderKeys() {
        content.addView(heading(getString(R.string.omni_keys_title)))

        val keys = keysHere()
        val chosen = Preferences.signingKey(this)

        val vault = card()
        if (keys.isEmpty()) {
            vault.addView(quiet(getString(R.string.omni_keys_none)))
        }
        keys.forEachIndexed { index, key ->
            if (index > 0) {
                vault.addView(rule(), MATCH_PARENT, 1)
            }
            val inUse = key.path == chosen
            val shared = key.alias == DEFAULT_KEY_ALIAS
            vault.addView(
                row(
                    key.alias,
                    "${key.subject}\n${getString(R.string.omni_keys_expires)} ${key.expires}  ·  " +
                        "${key.bits}\n${key.fingerprint}",
                    if (inUse) getString(R.string.omni_keys_in_use) else getString(R.string.omni_keys_use),
                    if (inUse) palette.ok else palette.accent,
                ) {
                    Preferences.setSigningKey(this, key.path)
                    render(false)
                }
            )
            if (shared) {
                vault.addView(quiet(getString(R.string.omni_keys_shared_note, DEFAULT_KEY_PASSWORD)))
            } else {
                vault.addView(subtle(getString(R.string.omni_action_delete), palette.error) {
                    runCatching { Builder.nativeDeleteKey(key.path) }
                    if (inUse) Preferences.setSigningKey(this, "")
                    render(false)
                })
            }
        }
        content.addView(vault)
        content.addView(primary(getString(R.string.omni_keys_new)) { go(Screen.NewKey) })
        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Settings)
        })
    }

    private fun renderNewKey() {
        content.addView(heading(getString(R.string.omni_keys_new)))

        val who = card()
        who.addView(field(getString(R.string.omni_key_alias), keyAlias) { keyAlias = it })
        who.addView(field(getString(R.string.omni_key_common_name), keyCommonName) { keyCommonName = it })
        who.addView(
            field(getString(R.string.omni_key_organisation), keyOrganisation) { keyOrganisation = it }
        )
        who.addView(field(getString(R.string.omni_key_country), keyCountry) { keyCountry = it })
        content.addView(who)

        content.addView(label(getString(R.string.omni_key_validity)))
        content.addView(
            chips(
                VALIDITY_YEARS.map { getString(R.string.omni_key_years, it) },
                { it == keyYears },
            ) { keyYears = it }
        )

        content.addView(label(getString(R.string.omni_key_size)))
        content.addView(chips(KEY_SIZES.map { it.toString() }, { it == keyBits }) { keyBits = it })

        val secrets = card()
        val first = secret(getString(R.string.omni_key_password))
        val again = secret(getString(R.string.omni_key_password_again))
        keyPasswordView = first.second
        keyPasswordAgainView = again.second
        secrets.addView(first.first)
        secrets.addView(again.first)
        content.addView(secrets)
        content.addView(warning(getString(R.string.omni_key_password_warning)))

        content.addView(primary(getString(R.string.omni_action_create)) { createKey() })
        content.addView(subtle(getString(R.string.omni_action_cancel), palette.muted) {
            go(Screen.Keys)
        })
    }

    private fun summaryOf(root: String): ProjectSummary? = runCatching {
        ProjectSummary.list(Builder.nativeListProjects(projectsFolder().absolutePath))
            .firstOrNull { it.root == root }
    }.getOrNull()

    private fun keysHere(): List<SigningKey> = runCatching {
        SigningKey.list(Builder.nativeListKeys(keysFolder().absolutePath))
    }.getOrDefault(emptyList())

    private fun named(folder: String): String? {
        val name = newPathView?.text?.toString().orEmpty().trim()
        if (name.isEmpty()) {
            results.removeAllViews()
            results.addView(notice(getString(R.string.omni_files_needs_name), palette.warning))
            return null
        }
        return if (folder.isEmpty()) name else "$folder/$name"
    }

    private fun folderName(folder: String): String =
        folder.substringAfterLast('/').ifEmpty { getString(R.string.omni_files_root) }

    private fun breadcrumb(root: String, folder: String): View {
        val steps = mutableListOf(getString(R.string.omni_files_root) to "")
        var walked = ""
        for (step in folder.split('/').filter { it.isNotEmpty() }) {
            walked = if (walked.isEmpty()) step else "$walked/$step"
            steps.add(step to walked)
        }
        return chips(steps.map { it.first }, { it == steps.lastIndex }) { index ->
            go(Screen.Files(root, steps[index].second))
        }
    }

    private fun size(bytes: Long): String = when {
        bytes >= 1_048_576L -> getString(R.string.omni_size_mb, bytes / 1_048_576L)
        bytes >= 1_024L -> getString(R.string.omni_size_kb, bytes / 1_024L)
        else -> getString(R.string.omni_size_b, bytes)
    }

    private fun left(seconds: Long): String = when {
        seconds >= 3600L -> getString(R.string.omni_left_hours, seconds / 3600L)
        seconds >= 60L -> getString(R.string.omni_left_minutes, seconds / 60L)
        else -> getString(R.string.omni_left_seconds, seconds)
    }

    private fun moment(epochSeconds: Long): String =
        SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US).format(Date(epochSeconds * 1000L))

    private fun deleteProject(project: ProjectSummary) {
        results.removeAllViews()
        val thrown = runCatching {
            JSONObject(Builder.nativeTrashSend(trashFolder(), project.root))
        }.getOrNull()
        if (thrown == null || !thrown.optBoolean("removed", false)) {
            thrown?.let { showRefusal(Refusal.parse(it), results) }
            return
        }
        if (openProject == project.root) {
            openProject = null
        }
        OmniLog.event(LogLevel.INFO, "project", "Deleted ${project.root}")
        render(false)
        results.addView(notice(getString(R.string.omni_trash_sent, project.name), palette.ok))
    }

    private fun renderSettings() {
        content.addView(heading(getString(R.string.omni_settings_language)))
        val current = Preferences.language(this)
        content.addView(
            chips(
                Preferences.LANGUAGES.map { it.second },
                { Preferences.LANGUAGES[it].first == current },
            ) { index ->
                Preferences.setLanguage(this, Preferences.LANGUAGES[index].first)
                recreate()
            }
        )

        content.addView(heading(getString(R.string.omni_settings_theme)))
        content.addView(
            chips(
                Palette.ALL.map { it.label },
                { Palette.ALL[it].key == palette.key },
            ) { index ->
                Preferences.setPalette(this, Palette.ALL[index].key)
                recreate()
            }
        )

        content.addView(heading(getString(R.string.omni_settings_defaults)))
        val defaults = card()
        defaults.addView(label(getString(R.string.omni_form_architecture)))
        defaults.addView(
            chips(ABI_CHOICES.map { it.first }, { it == Preferences.abi(this) }) { index ->
                Preferences.setAbi(this, index)
                formAbi = index
            }
        )
        defaults.addView(label(getString(R.string.omni_form_min_sdk)))
        defaults.addView(
            chips(
                ANDROID_RELEASES.map { it.second },
                { ANDROID_RELEASES[it].first == Preferences.minSdk(this) },
            ) { index ->
                val level = ANDROID_RELEASES[index].first
                Preferences.setMinSdk(this, level)
                formMinSdk = level
                if (Preferences.targetSdk(this) < level) {
                    Preferences.setTargetSdk(this, level)
                    formTargetSdk = level
                }
            }
        )
        defaults.addView(label(getString(R.string.omni_form_target_sdk)))
        defaults.addView(
            chips(
                ANDROID_RELEASES.map { it.second },
                { ANDROID_RELEASES[it].first == Preferences.targetSdk(this) },
            ) { index ->
                val level = ANDROID_RELEASES[index].first
                Preferences.setTargetSdk(this, level)
                formTargetSdk = level
                if (Preferences.minSdk(this) > level) {
                    Preferences.setMinSdk(this, level)
                    formMinSdk = level
                }
            }
        )
        content.addView(defaults)

        content.addView(heading(getString(R.string.omni_keys_title)))
        val chosen = Preferences.signingKey(this)
        val key = keysHere().firstOrNull { it.path == chosen }
        val signing = card()
        signing.addView(
            keyValue(
                key?.alias ?: getString(R.string.omni_keys_none),
                key?.subject.orEmpty(),
                when {
                    key == null -> ""
                    key.alias == DEFAULT_KEY_ALIAS -> getString(R.string.omni_keys_shared)
                    else -> getString(R.string.omni_keys_yours)
                },
                if (key == null) palette.warning else palette.accent,
            )
        )
        signing.addView(subtle(getString(R.string.omni_keys_manage), palette.accent) {
            go(Screen.Keys)
        })
        content.addView(signing)

        content.addView(heading(getString(R.string.omni_settings_watch)))
        val watching = card()
        watching.addView(
            toggle(getString(R.string.omni_settings_watch), Preferences.watching(this)) { on ->
                Preferences.setWatching(this, on)
                if (on) Sentry.arm(this) else Sentry.disarm(this)
                render(false)
            }
        )
        watching.addView(
            keyValue(
                getString(R.string.omni_settings_integrity),
                Sentry.lastChecked(this).takeIf { it > 0L }?.let {
                    SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US).format(Date(it))
                } ?: "",
                standing,
                when (standing) {
                    "TRUSTED" -> palette.ok
                    "TAMPERED" -> palette.error
                    else -> palette.warning
                },
            )
        )
        content.addView(watching)
        content.addView(quiet(getString(R.string.omni_settings_watch_note)))

        content.addView(heading(getString(R.string.omni_settings_logs)))
        val logging = card()
        logging.addView(
            toggle(getString(R.string.omni_settings_logs), Preferences.sharingLog(this)) { on ->
                Preferences.setSharingLog(this, on)
                render(false)
            }
        )
        if (Preferences.sharingLog(this)) {
            OmniLog.lastCopies().forEach { copy ->
                logging.addView(
                    keyValue(
                        copy.label,
                        copy.error ?: copy.location,
                        if (copy.succeeded) "OK" else "—",
                        if (copy.succeeded) palette.ok else palette.warning,
                    )
                )
            }
        }
        content.addView(logging)

        content.addView(heading(getString(R.string.omni_settings_about)))
        val state = runCatching {
            CoreState.parse(Builder.nativeStateReport(Builder.observedEnvironment(this)))
        }.getOrNull()
        val about = card()
        if (state == null) {
            about.addView(quiet(getString(R.string.omni_integrity_unknown)))
        } else {
            about.addView(keyValue(getString(R.string.omni_settings_core), state.status, state.version, palette.accent))
            about.addView(
                keyValue(
                    getString(R.string.omni_settings_toolchain),
                    "",
                    "${state.toolchainVerified}/${state.toolchainTotal}",
                    if (state.toolchainVerified == state.toolchainTotal) palette.ok else palette.warning,
                )
            )
            about.addView(
                keyValue(getString(R.string.omni_settings_abi), "", state.abiVersion.toString(), palette.muted)
            )
        }
        content.addView(about)
    }

    private fun spec() = ProjectSpec(
        packageName = formPackage.trim(),
        label = formLabel.trim(),
        versionName = formVersionName.trim(),
        versionCode = formVersionCode.trim().toIntOrNull() ?: 0,
        abis = ABI_CHOICES[formAbi].second,
        minSdk = formMinSdk,
        targetSdk = formTargetSdk,
        languages = formLanguages.toList(),
        locales = (formLocales + BASE_LOCALE).toList(),
    )

    private fun createProject() {
        results.removeAllViews()
        val root = File(projectsFolder(), formLabel.trim().ifEmpty { "Project" })
        val image = formImage
        working({ Builder.nativeCreateProject(root.absolutePath, spec().encode()) }) finished@{ answer ->
            val outcome = runCatching { CreateOutcome.parse(answer) }.getOrElse {
                results.addView(notice(it.message ?: it.javaClass.simpleName, palette.error))
                return@finished
            }
            if (!outcome.created) {
                showRefusal(Refusal.parse(JSONObject(answer)), results)
                return@finished
            }
            OmniLog.event(LogLevel.INFO, "project", "Created ${outcome.root}")
            results.addView(notice(getString(R.string.omni_created), palette.ok))
            if (image != null) {
                val stored = runCatching {
                    JSONObject(Builder.nativeSetIcon(root.absolutePath, image))
                }.getOrNull()
                if (stored == null || !stored.optBoolean("stored", false)) {
                    stored?.let { showRefusal(Refusal.parse(it), results) }
                }
            }
            formImage = null
            openHere(root.absolutePath)
        }
    }

    private fun createKey() {
        results.removeAllViews()
        val first = readSecret(keyPasswordView)
        val again = readSecret(keyPasswordAgainView)
        if (!first.contentEquals(again)) {
            first.fill(' ')
            again.fill(' ')
            results.addView(notice(getString(R.string.omni_key_password_mismatch), palette.error))
            return
        }
        again.fill(' ')

        val request = KeySpec(
            alias = keyAlias.trim(),
            commonName = keyCommonName.trim(),
            organisation = keyOrganisation.trim(),
            country = keyCountry.trim().uppercase(Locale.US),
            validityYears = VALIDITY_YEARS[keyYears],
            bits = KEY_SIZES[keyBits],
        )
        val folder = keysFolder().absolutePath

        working({ Builder.nativeCreateKey(folder, request.encode(), first) }) finished@{ answer ->
            first.fill(' ')
            keyPasswordView?.text?.clear()
            keyPasswordAgainView?.text?.clear()
            val root = runCatching { JSONObject(answer) }.getOrNull() ?: return@finished
            if (!root.optBoolean("created", false)) {
                showRefusal(Refusal.parse(root), results)
                return@finished
            }
            val made = SigningKey.parse(root.getJSONObject("key"))
            OmniLog.event(LogLevel.INFO, "keystore", "Key ${made.alias} created, ${made.fingerprint}")
            Preferences.setSigningKey(this, made.path)
            go(Screen.Keys)
        }
    }

    private fun buildProject(root: String) {
        results.removeAllViews()
        val keyPath = Preferences.signingKey(this)
        if (keyPath.isEmpty()) {
            results.addView(notice(getString(R.string.omni_build_no_key), palette.warning))
            return
        }
        if (readSecret(buildPasswordView).isEmpty()) {
            results.addView(notice(getString(R.string.omni_build_needs_password), palette.warning))
            return
        }

        val name = File(root).name
        val stamp = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())
        val here = builtFolder()
        val apk = File(here, "$name-$stamp.apk")
        val aab = File(here, "$name-$stamp.aab")
        val password = readSecret(buildPasswordView)
        val started = System.nanoTime()

        working({
            Builder.nativeBuildAll(
                root,
                apk.absolutePath,
                aab.absolutePath,
                keyPath,
                if (password.isEmpty()) null else password,
            )
        }) finished@{ answer ->
            password.fill(' ')
            buildPasswordView?.text?.clear()
            val elapsed = (System.nanoTime() - started) / 1_000_000
            val outcome = runCatching { BuildOutcome.parse(answer) }.getOrElse {
                results.addView(notice(it.message ?: it.javaClass.simpleName, palette.error))
                return@finished
            }
            if (!outcome.built) {
                OmniLog.event(LogLevel.ERROR, "build", "Refused: ${outcome.error}")
                results.addView(notice(getString(R.string.omni_refused), palette.error))
                showRefusal(Refusal.parse(JSONObject(answer)), results)
                outcome.findings.forEach { results.addView(quiet(it)) }
                return@finished
            }

            OmniLog.event(
                LogLevel.INFO,
                "build",
                "Built ${outcome.bytes} bytes and bundled ${outcome.bundleBytes} in $elapsed ms",
            )
            results.addView(
                notice(getString(R.string.omni_build_done, elapsed), palette.ok)
            )
            val facts = card()
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_package),
                    apk.name,
                    size(outcome.bytes),
                    palette.ok,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_bundle),
                    aab.name,
                    size(outcome.bundleBytes),
                    palette.ok,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_contents),
                    if (outcome.carriesCode) "AndroidManifest.xml + classes.dex" else "AndroidManifest.xml",
                    outcome.entries.toString(),
                    palette.muted,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_languages),
                    "",
                    outcome.locales.toString(),
                    palette.muted,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_signature),
                    outcome.signedBy.orEmpty(),
                    "v2 + v3",
                    if (outcome.signed) palette.ok else palette.error,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_policy),
                    "${outcome.rulesApplied}",
                    outcome.guardVerdict ?: "?",
                    if (outcome.guardVerdict == "PASSED") palette.ok else palette.error,
                )
            )
            results.addView(facts)
            outcome.path?.let { offer(File(it)) }
        }
    }

    private fun offer(file: File) {
        if (!file.isFile) {
            return
        }
        val uri = PackageProvider.uriFor(this, file)
        val type = contentResolver.getType(uri) ?: PackageProvider.BUNDLE_TYPE
        val installable = type == PackageProvider.PACKAGE_TYPE

        if (installable) {
            results.addView(primary(getString(R.string.omni_action_install)) {
                hand(
                    Intent(Intent.ACTION_VIEW)
                        .setDataAndType(uri, type)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            })
        }
        results.addView(subtle(getString(R.string.omni_action_share), palette.accent) {
            val sending = Intent(Intent.ACTION_SEND)
                .setType(type)
                .putExtra(Intent.EXTRA_STREAM, uri)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            hand(
                Intent.createChooser(sending, file.name)
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            )
        })
        if (installable) {
            results.addView(quiet(getString(R.string.omni_install_note)))
        }
    }

    private fun hand(intent: Intent) {
        runCatching { startActivity(intent) }.onFailure { why ->
            OmniLog.event(LogLevel.WARN, "handoff", why.message ?: why.javaClass.simpleName)
            results.addView(notice(why.message ?: why.javaClass.simpleName, palette.error))
        }
    }

    private fun chooseImage() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "image/png"
        }
        runCatching { startActivityForResult(intent, IMAGE_REQUEST) }
            .onFailure {
                results.addView(
                    notice(
                        it.message ?: getString(R.string.omni_form_image_none),
                        palette.error,
                    )
                )
            }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != IMAGE_REQUEST || resultCode != RESULT_OK) {
            return
        }
        val uri = data?.data ?: return
        val staged = File(cacheDir, "chosen_image.png")
        formImage = runCatching {
            contentResolver.openInputStream(uri).use { source ->
                requireNotNull(source)
                FileOutputStream(staged).use { sink -> source.copyTo(sink) }
            }
            staged.absolutePath
        }.getOrNull()
        render(false)
    }

    private fun working(work: () -> String, finished: (String) -> Unit) {
        results.removeAllViews()
        results.addView(notice(getString(R.string.omni_working), palette.accent))
        Thread {
            val answer = runCatching(work)
            runOnUiThread {
                results.removeAllViews()
                answer.fold(finished) { error ->
                    OmniLog.recordCrash(Thread.currentThread(), error)
                    results.addView(
                        notice(error.message ?: error.javaClass.simpleName, palette.error)
                    )
                }
            }
        }.start()
    }

    private fun act(answer: String, field: String) {
        results.removeAllViews()
        val root = runCatching { JSONObject(answer) }.getOrNull()
        if (root == null) {
            results.addView(notice(getString(R.string.omni_refused), palette.error))
            return
        }
        if (!root.optBoolean(field, false)) {
            showRefusal(Refusal.parse(root), results)
            return
        }
        render(false)
    }

    private fun showRefusal(refusal: Refusal, into: LinearLayout) {
        val heading = listOfNotNull(refusal.code, refusal.message).joinToString("  ")
        into.addView(notice(heading.ifEmpty { getString(R.string.omni_refused) }, palette.error))
        val detail = card()
        refusal.context.forEach { detail.addView(quiet(it)) }
        refusal.suggestion?.let { detail.addView(body(it)) }
        if (detail.childCount > 0) {
            into.addView(detail)
        }
    }

    private fun readSecret(view: EditText?): CharArray {
        val editable = view?.text ?: return CharArray(0)
        val chars = CharArray(editable.length)
        editable.getChars(0, editable.length, chars, 0)
        return chars
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

    private fun gap(units: Int): Int =
        (units * resources.displayMetrics.density * 4f).toInt().coerceAtLeast(1)

    private fun pill(colour: Int, radius: Float) = GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        cornerRadius = radius
        setColor(colour)
    }

    private fun sheet(fill: Int, stroke: Int) = GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        cornerRadius = gap(4).toFloat()
        setColor(fill)
        setStroke(1, stroke)
    }

    private fun touchable(background: GradientDrawable, ripple: Int): Drawable =
        RippleDrawable(ColorStateList.valueOf(ripple), background, null)

    private fun card(): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = sheet(palette.surface, palette.divider)
        setPadding(gap(3), gap(2), gap(3), gap(2))
        layoutTransition = LayoutTransition()
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(2)
            bottomMargin = gap(1)
        }
    }

    private fun heading(value: String) = TextView(this).apply {
        text = value
        setTextColor(palette.foreground)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
        setPadding(0, gap(4), 0, gap(1))
    }

    private fun label(value: String) = TextView(this).apply {
        text = value.uppercase(Locale.getDefault())
        setTextColor(palette.muted)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
        letterSpacing = 0.12f
        setPadding(0, gap(3), 0, gap(1))
    }

    private fun body(value: String) = TextView(this).apply {
        text = value
        setTextColor(palette.foreground)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        setLineSpacing(gap(1).toFloat(), 1f)
        setPadding(0, gap(1), 0, gap(1))
    }

    private fun quiet(value: String) = TextView(this).apply {
        text = value
        setTextColor(palette.muted)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        setLineSpacing(gap(1).toFloat(), 1f)
        setPadding(0, gap(1), 0, gap(1))
    }

    private fun warning(value: String) = TextView(this).apply {
        text = value
        setTextColor(palette.warning)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        setLineSpacing(gap(1).toFloat(), 1f)
        setPadding(gap(3), gap(2), gap(3), gap(2))
        background = sheet(palette.surface, palette.warning)
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(1)
            bottomMargin = gap(1)
        }
    }

    private fun notice(value: String, colour: Int) = TextView(this).apply {
        text = value
        setTextColor(colour)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        setPadding(gap(3), gap(3), gap(3), gap(3))
        background = sheet(palette.surface, colour)
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(2)
            bottomMargin = gap(1)
        }
    }

    private fun rule() = View(this).apply {
        setBackgroundColor(palette.divider)
    }

    private fun primary(text: String, onPress: () -> Unit) = TextView(this).apply {
        this.text = text
        setTextColor(palette.background)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        letterSpacing = 0.06f
        gravity = Gravity.CENTER
        setPadding(gap(4), gap(4), gap(4), gap(4))
        background = touchable(pill(palette.accent, gap(3).toFloat()), palette.background)
        isClickable = true
        setOnClickListener { onPress() }
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(2)
            bottomMargin = gap(1)
        }
    }

    private fun subtle(text: String, colour: Int, onPress: () -> Unit) = TextView(this).apply {
        this.text = text
        setTextColor(colour)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        gravity = Gravity.CENTER
        setPadding(gap(4), gap(3), gap(4), gap(3))
        background = touchable(pill(Color.TRANSPARENT, gap(3).toFloat()), colour)
        isClickable = true
        setOnClickListener { onPress() }
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(1)
        }
    }

    private fun row(
        title: String,
        detail: String,
        trailing: String,
        trailingColour: Int = palette.muted,
        onPress: () -> Unit,
    ) = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(0, gap(2), 0, gap(2))
        background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
        isClickable = true
        setOnClickListener { onPress() }

        addView(
            LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                addView(
                    TextView(context).apply {
                        text = title
                        setTextColor(palette.foreground)
                        setTypeface(Typeface.DEFAULT_BOLD)
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                    }
                )
                if (detail.isNotEmpty()) {
                    addView(
                        TextView(context).apply {
                            text = detail
                            setTextColor(palette.muted)
                            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                            setLineSpacing(gap(1).toFloat(), 1f)
                        }
                    )
                }
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
        )

        if (trailing.isNotEmpty()) {
            addView(
                TextView(context).apply {
                    text = trailing
                    setTextColor(trailingColour)
                    setTypeface(Typeface.DEFAULT_BOLD)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                    letterSpacing = 0.08f
                    setPadding(gap(2), gap(1), gap(2), gap(1))
                    background = pill(palette.raised, gap(2).toFloat())
                }
            )
        }
    }

    private fun keyValue(title: String, detail: String, trailing: String, colour: Int) =
        LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, gap(2), 0, gap(2))
            addView(
                LinearLayout(context).apply {
                    orientation = LinearLayout.VERTICAL
                    addView(
                        TextView(context).apply {
                            text = title
                            setTextColor(palette.foreground)
                            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                        }
                    )
                    if (detail.isNotEmpty()) {
                        addView(
                            TextView(context).apply {
                                text = detail
                                setTextColor(palette.muted)
                                setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                            }
                        )
                    }
                },
                LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
            )
            if (trailing.isNotEmpty()) {
                addView(
                    TextView(context).apply {
                        text = trailing
                        setTextColor(colour)
                        setTypeface(Typeface.MONOSPACE, Typeface.BOLD)
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                    }
                )
            }
        }

    private fun toggle(title: String, on: Boolean, onChange: (Boolean) -> Unit) =
        LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, gap(2), 0, gap(2))
            isClickable = true
            background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
            setOnClickListener { onChange(!on) }
            addView(
                TextView(context).apply {
                    text = title
                    setTextColor(palette.foreground)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                },
                LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
            )
            addView(
                View(context).apply {
                    background = pill(if (on) palette.ok else palette.divider, gap(2).toFloat())
                    layoutParams = LinearLayout.LayoutParams(gap(10), gap(5))
                }
            )
        }

    private fun input(label: String, initial: String): Pair<View, EditText> {
        val holder = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, gap(2), 0, gap(1))
        }
        holder.addView(
            TextView(this).apply {
                text = label.uppercase(Locale.getDefault())
                setTextColor(palette.muted)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 10f)
                letterSpacing = 0.12f
            }
        )
        val editor = EditText(this).apply {
            setText(initial)
            setTextColor(palette.foreground)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            setBackgroundColor(Color.TRANSPARENT)
            setPadding(0, gap(1), 0, gap(1))
            isSingleLine = true
        }
        holder.addView(editor)
        holder.addView(rule(), LinearLayout.LayoutParams(MATCH_PARENT, 1))
        return holder to editor
    }

    private fun field(label: String, initial: String, onChange: (String) -> Unit): View {
        val (holder, editor) = input(label, initial)
        editor.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun afterTextChanged(s: Editable?) {
                onChange(s?.toString().orEmpty())
            }
        })
        return holder
    }

    private fun secret(label: String, initial: String = ""): Pair<View, EditText> {
        val (holder, editor) = input(label, initial)
        editor.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
        editor.typeface = Typeface.MONOSPACE
        return holder to editor
    }

    private fun chips(
        labels: List<String>,
        selected: (Int) -> Boolean,
        onPick: (Int) -> Unit,
    ): View {
        val holder = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, gap(1), 0, gap(1))
        }
        val scroller = HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            clipToPadding = false
        }
        val views = mutableListOf<TextView>()

        fun repaint() {
            views.forEachIndexed { index, view ->
                val on = selected(index)
                view.setTextColor(if (on) palette.background else palette.foreground)
                view.background = touchable(
                    pill(if (on) palette.accent else palette.raised, gap(4).toFloat()),
                    palette.accent,
                )
            }
        }

        labels.forEachIndexed { index, text ->
            val chip = TextView(this).apply {
                this.text = text
                setTypeface(Typeface.DEFAULT_BOLD)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                setPadding(gap(4), gap(2), gap(4), gap(2))
                isClickable = true
                setOnClickListener {
                    onPick(index)
                    repaint()
                    animate().scaleX(0.94f).scaleY(0.94f).setDuration(70L).withEndAction {
                        animate().scaleX(1f).scaleY(1f).setDuration(110L).start()
                    }.start()
                }
                layoutParams = LinearLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT).apply {
                    marginEnd = gap(2)
                }
            }
            views.add(chip)
            holder.addView(chip)
        }
        repaint()
        scroller.addView(holder)
        return scroller
    }

    private fun View.setPadding(all: Int) = setPadding(all, all, all, all)
}
