package com.omni.builder

import android.Manifest
import android.app.Activity
import android.app.Application
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
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
import android.text.InputType
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
        Sentry.arm(this)
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

    fun refused(context: Context): Boolean = store(context).getBoolean(REFUSED, false)

    fun lastStanding(context: Context): String =
        store(context).getString(LAST_STANDING, "").orEmpty()

    fun lastChecked(context: Context): Long = store(context).getLong(LAST_CHECKED, 0L)
}

class SentryService : JobService() {

    override fun onStartJob(params: JobParameters?): Boolean {
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

    external fun nativeBuildProject(
        root: String,
        outputPath: String,
        keyPath: String,
        keyPassword: CharArray?,
    ): String

    external fun nativeVerifySelf(packagePath: String, expectedCertificate: String?): String

    external fun nativeCreateKey(directory: String, spec: String, keyPassword: CharArray): String

    external fun nativeListKeys(directory: String): String

    external fun nativeDeleteKey(path: String): String

    external fun nativeCheckKey(path: String, keyPassword: CharArray): String

    external fun nativeListProjects(directory: String): String

    external fun nativeProjectTree(root: String): String

    external fun nativeReadFile(root: String, relative: String): String

    external fun nativeWriteFile(root: String, relative: String, contents: String): String

    external fun nativeNewFolder(root: String, relative: String): String

    external fun nativeRemovePath(root: String, relative: String): String

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
) {
    fun encode(): String = buildString {
        append("package=").append(packageName).append(';')
        append("label=").append(label).append(';')
        append("versionName=").append(versionName).append(';')
        append("versionCode=").append(versionCode).append(';')
        append("abis=").append(abis.joinToString(",")).append(';')
        append("minSdk=").append(minSdk).append(';')
        append("targetSdk=").append(targetSdk).append(';')
        append("languages=").append(languages.joinToString(","))
    }
}

data class KeySpec(
    val alias: String,
    val commonName: String,
    val organisation: String,
    val country: String,
    val validityDays: Int,
    val bits: Int,
) {
    fun encode(): String = buildString {
        append("alias=").append(alias).append(';')
        append("commonName=").append(commonName).append(';')
        append("organisation=").append(organisation).append(';')
        append("country=").append(country).append(';')
        append("validityDays=").append(validityDays).append(';')
        append("bits=").append(bits)
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

object Preferences {

    private const val FILE = "omni_settings"
    private const val LANGUAGE = "language"
    private const val SIGNING_KEY = "signing_key"

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

    fun signingKey(context: Context): String = store(context).getString(SIGNING_KEY, "").orEmpty()

    fun setSigningKey(context: Context, path: String) {
        store(context).edit().putString(SIGNING_KEY, path).apply()
    }
}

private sealed interface Screen {
    data object Projects : Screen

    data object NewProject : Screen

    data object NewKey : Screen

    data class Open(val root: String) : Screen

    data class Editor(val root: String, val path: String) : Screen

    data object Settings : Screen
}

class BuilderActivity : Activity() {

    private companion object {
        const val STORAGE_PERMISSION_REQUEST = 1
        const val IMAGE_REQUEST = 2

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
        const val DEFAULT_VALIDITY_DAYS = 10950
    }

    private var screen: Screen = Screen.Projects
    private var standing = "UNKNOWN"

    private var formPackage = "com.tr.yt"
    private var formLabel = "My App"
    private var formVersionName = "1.0.0"
    private var formVersionCode = "1"
    private var formAbi = 2
    private var formMinSdk = 28
    private var formTargetSdk = 36
    private val formLanguages = linkedSetOf("kotlin")
    private var formImage: String? = null

    private var keyAlias = "release"
    private var keyCommonName = ""
    private var keyOrganisation = "Omni"
    private var keyCountry = "TR"
    private var keyValidity = DEFAULT_VALIDITY_DAYS.toString()
    private var keyBits = 0
    private var keyPasswordView: EditText? = null
    private var keyPasswordAgainView: EditText? = null
    private var buildPasswordView: EditText? = null

    private var editorText = ""
    private var newPathView: EditText? = null

    private lateinit var content: LinearLayout
    private lateinit var results: LinearLayout

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

        content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(color(R.color.omni_background))
            setPadding(dp(R.dimen.omni_screen_padding))
        }
        results = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        setContentView(ScrollView(this).apply {
            setBackgroundColor(color(R.color.omni_background))
            isFillViewport = true
            addView(content, MATCH_PARENT, WRAP_CONTENT)
        })
        standing = examine()
        render()
    }

    override fun onResume() {
        super.onResume()
        val before = standing
        standing = examine()
        if (standing != before) {
            render()
        }
    }

    private fun examine(): String {
        if (Sentry.refused(this)) {
            return "TAMPERED"
        }
        return Sentry.check(this)
    }

    private fun render() {
        content.removeAllViews()
        results.removeAllViews()

        when (val load = Builder.load()) {
            is Builder.LoadState.Failed -> {
                content.addView(banner(load.reason, R.color.omni_error))
                return
            }
            is Builder.LoadState.Loaded -> Unit
        }

        val where = standing
        if (where == "TAMPERED") {
            content.addView(banner(getString(R.string.omni_integrity_refused_title), R.color.omni_error))
            content.addView(body(getString(R.string.omni_integrity_refused_body)))
            content.addView(body(getString(R.string.omni_integrity_checked)))
            return
        }
        if (where == "UNKNOWN") {
            content.addView(banner(getString(R.string.omni_integrity_unknown), R.color.omni_warning))
        }

        content.addView(tabs())
        when (val here = screen) {
            is Screen.Projects -> renderProjects()
            is Screen.NewProject -> renderNewProject()
            is Screen.NewKey -> renderNewKey()
            is Screen.Open -> renderProject(here.root)
            is Screen.Editor -> renderEditor(here.root, here.path)
            is Screen.Settings -> renderSettings()
        }
        content.addView(results)
    }

    private fun go(next: Screen) {
        screen = next
        render()
    }

    @Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
    override fun onBackPressed() {
        when (val here = screen) {
            is Screen.Projects -> super.onBackPressed()
            is Screen.Editor -> go(Screen.Open(here.root))
            else -> go(Screen.Projects)
        }
    }

    private fun tabs(): View {
        val strip = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val onProjects = screen !is Screen.Settings
        strip.addView(tab(getString(R.string.omni_tab_projects), onProjects) { go(Screen.Projects) })
        strip.addView(tab(getString(R.string.omni_tab_settings), !onProjects) { go(Screen.Settings) })
        return strip
    }

    private fun tab(label: String, active: Boolean, onPress: () -> Unit) = TextView(this).apply {
        text = label
        setTextColor(color(if (active) R.color.omni_accent else R.color.omni_muted))
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_section).toFloat())
        setPadding(0, dp(R.dimen.omni_gap), dp(R.dimen.omni_screen_padding), dp(R.dimen.omni_gap))
        isClickable = true
        setOnClickListener { onPress() }
    }

    private fun projectsFolder() = File(getExternalFilesDir(null) ?: filesDir, "Projects")

    private fun keysFolder() = File(filesDir, "Keys")

    private fun renderProjects() {
        section(R.string.omni_projects_title)
        content.addView(action(getString(R.string.omni_projects_new)) { go(Screen.NewProject) })

        val projects = runCatching {
            ProjectSummary.list(Builder.nativeListProjects(projectsFolder().absolutePath))
        }.getOrDefault(emptyList())

        if (projects.isEmpty()) {
            content.addView(body(getString(R.string.omni_projects_none)))
        }
        projects.forEach { project ->
            content.addView(
                row(
                    project.label.ifEmpty { project.name },
                    "${project.packageName} · ${project.versionName} (${project.versionCode}) · " +
                        "API ${project.minSdk}-${project.targetSdk} · ${project.files}",
                    R.color.omni_foreground,
                ) { go(Screen.Open(project.root)) }
            )
            project.icon?.let { content.addView(bullet(it)) }
        }

        section(R.string.omni_keys_title)
        content.addView(action(getString(R.string.omni_keys_new)) { go(Screen.NewKey) })
        renderKeyList()
    }

    private fun renderKeyList() {
        val keys = runCatching {
            SigningKey.list(Builder.nativeListKeys(keysFolder().absolutePath))
        }.getOrDefault(emptyList())

        if (keys.isEmpty()) {
            content.addView(body(getString(R.string.omni_keys_none)))
            return
        }
        val chosen = Preferences.signingKey(this)
        keys.forEach { key ->
            val inUse = key.path == chosen
            content.addView(
                row(
                    if (inUse) "${key.alias}   [${getString(R.string.omni_keys_in_use)}]" else key.alias,
                    "${key.subject} · ${key.bits} · " +
                        "${getString(R.string.omni_keys_expires)} ${key.expires}",
                    if (inUse) R.color.omni_ok else R.color.omni_foreground,
                ) {
                    Preferences.setSigningKey(this, key.path)
                    render()
                }
            )
            content.addView(bullet("${getString(R.string.omni_keys_fingerprint)}: ${key.fingerprint}"))
            content.addView(
                action(getString(R.string.omni_action_delete), R.color.omni_error) {
                    runCatching { Builder.nativeDeleteKey(key.path) }
                    if (inUse) Preferences.setSigningKey(this, "")
                    render()
                }
            )
        }
    }

    private fun renderNewProject() {
        section(R.string.omni_projects_new)
        content.addView(field(getString(R.string.omni_form_package), formPackage) { formPackage = it })
        content.addView(field(getString(R.string.omni_form_label), formLabel) { formLabel = it })
        content.addView(
            field(getString(R.string.omni_form_version_name), formVersionName) { formVersionName = it }
        )
        content.addView(
            field(getString(R.string.omni_form_version_code), formVersionCode) { formVersionCode = it }
        )

        section(R.string.omni_form_architecture)
        content.addView(chips(ABI_CHOICES.map { it.first }, { it == formAbi }) { formAbi = it })

        section(R.string.omni_form_min_sdk)
        content.addView(
            chips(ANDROID_RELEASES.map { it.second }, { ANDROID_RELEASES[it].first == formMinSdk }) {
                formMinSdk = ANDROID_RELEASES[it].first
                if (formTargetSdk < formMinSdk) formTargetSdk = formMinSdk
            }
        )

        section(R.string.omni_form_target_sdk)
        content.addView(
            chips(ANDROID_RELEASES.map { it.second }, { ANDROID_RELEASES[it].first == formTargetSdk }) {
                formTargetSdk = ANDROID_RELEASES[it].first
                if (formMinSdk > formTargetSdk) formMinSdk = formTargetSdk
            }
        )

        section(R.string.omni_form_languages)
        content.addView(
            chips(
                LANGUAGE_CHOICES.map { it.second },
                { formLanguages.contains(LANGUAGE_CHOICES[it].first) },
            ) { index ->
                val key = LANGUAGE_CHOICES[index].first
                if (!formLanguages.remove(key)) formLanguages.add(key)
            }
        )
        content.addView(body(getString(R.string.omni_form_no_compiler)))

        section(R.string.omni_form_image)
        content.addView(action(getString(R.string.omni_form_image_choose)) { chooseImage() })
        content.addView(body(formImage ?: getString(R.string.omni_form_image_none)))

        content.addView(action(getString(R.string.omni_action_create)) { createProject() })
        content.addView(action(getString(R.string.omni_action_cancel), R.color.omni_muted) {
            go(Screen.Projects)
        })
    }

    private fun renderNewKey() {
        section(R.string.omni_keys_new)
        content.addView(field(getString(R.string.omni_key_alias), keyAlias) { keyAlias = it })
        content.addView(
            field(getString(R.string.omni_key_common_name), keyCommonName) { keyCommonName = it }
        )
        content.addView(
            field(getString(R.string.omni_key_organisation), keyOrganisation) { keyOrganisation = it }
        )
        content.addView(field(getString(R.string.omni_key_country), keyCountry) { keyCountry = it })
        content.addView(field(getString(R.string.omni_key_validity), keyValidity) { keyValidity = it })

        section(R.string.omni_key_size)
        content.addView(chips(KEY_SIZES.map { it.toString() }, { it == keyBits }) { keyBits = it })

        val first = secret(getString(R.string.omni_key_password))
        val again = secret(getString(R.string.omni_key_password_again))
        keyPasswordView = first.second
        keyPasswordAgainView = again.second
        content.addView(first.first)
        content.addView(again.first)
        content.addView(body(getString(R.string.omni_key_password_warning)))

        content.addView(action(getString(R.string.omni_action_create)) { createKey() })
        content.addView(action(getString(R.string.omni_action_cancel), R.color.omni_muted) {
            go(Screen.Projects)
        })
    }

    private fun renderProject(root: String) {
        val entries = runCatching {
            FileEntry.list(Builder.nativeProjectTree(root))
        }.getOrDefault(emptyList())

        section(R.string.omni_editor_title)
        content.addView(mono(root))
        entries.forEach { entry ->
            val trailing = if (entry.folder) "/" else " ${entry.bytes}"
            content.addView(
                row(entry.path + trailing, "", R.color.omni_foreground) {
                    if (!entry.folder) {
                        editorText = ""
                        go(Screen.Editor(root, entry.path))
                    }
                }
            )
        }

        val prompt = input(getString(R.string.omni_editor_name_prompt), "")
        newPathView = prompt.second
        content.addView(prompt.first)
        content.addView(action(getString(R.string.omni_action_new_file)) {
            val path = newPathView?.text?.toString().orEmpty()
            if (path.isNotEmpty()) {
                report(Builder.nativeWriteFile(root, path, ""), "saved")
                render()
            }
        })
        content.addView(action(getString(R.string.omni_action_new_folder)) {
            val path = newPathView?.text?.toString().orEmpty()
            if (path.isNotEmpty()) {
                report(Builder.nativeNewFolder(root, path), "made")
                render()
            }
        })

        section(R.string.omni_result_signature)
        val chosen = Preferences.signingKey(this)
        if (chosen.isEmpty()) {
            content.addView(body(getString(R.string.omni_build_no_key)))
        } else {
            content.addView(mono(File(chosen).name))
            val secret = secret(getString(R.string.omni_build_password))
            buildPasswordView = secret.second
            content.addView(secret.first)
            content.addView(action(getString(R.string.omni_action_build)) { buildProject(root, chosen) })
        }
        content.addView(action(getString(R.string.omni_action_back), R.color.omni_muted) {
            go(Screen.Projects)
        })
    }

    private fun renderEditor(root: String, path: String) {
        section(R.string.omni_editor_title)
        content.addView(mono(path))

        val answer = runCatching { JSONObject(Builder.nativeReadFile(root, path)) }.getOrNull()
        if (answer == null || !answer.optBoolean("read", false)) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(action(getString(R.string.omni_action_back), R.color.omni_muted) {
                go(Screen.Open(root))
            })
            return
        }
        editorText = answer.optString("text")

        val editor = EditText(this).apply {
            setText(editorText)
            setTextColor(color(R.color.omni_foreground))
            setTypeface(Typeface.MONOSPACE)
            setTextSize(TypedValue.COMPLEX_UNIT_PX, resources.getDimension(R.dimen.omni_text_small))
            gravity = Gravity.TOP or Gravity.START
            setHorizontallyScrolling(false)
            minLines = 12
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
                override fun afterTextChanged(s: Editable?) {
                    editorText = s?.toString().orEmpty()
                }
            })
        }
        content.addView(editor)
        content.addView(action(getString(R.string.omni_action_save)) {
            results.removeAllViews()
            val saved = runCatching { JSONObject(Builder.nativeWriteFile(root, path, editorText)) }
                .getOrNull()
            if (saved != null && saved.optBoolean("saved", false)) {
                results.addView(banner(getString(R.string.omni_editor_saved, path), R.color.omni_ok))
            } else {
                saved?.let { showRefusal(Refusal.parse(it), results) }
            }
        })
        content.addView(action(getString(R.string.omni_action_delete), R.color.omni_error) {
            report(Builder.nativeRemovePath(root, path), "removed")
            go(Screen.Open(root))
        })
        content.addView(action(getString(R.string.omni_action_back), R.color.omni_muted) {
            go(Screen.Open(root))
        })
    }

    private fun renderSettings() {
        section(R.string.omni_settings_language)
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
        content.addView(body(getString(R.string.omni_settings_language_note)))

        val state = runCatching {
            CoreState.parse(Builder.nativeStateReport(Builder.observedEnvironment(this)))
        }.getOrNull()

        section(R.string.omni_settings_core)
        if (state == null) {
            content.addView(body(getString(R.string.omni_integrity_unknown)))
        } else {
            content.addView(mono("${state.version} / ${state.phase} / ABI ${state.abiVersion}"))
            content.addView(body(state.selfHostingNote))

            section(R.string.omni_settings_unfinished)
            state.subsystems.filter { it.missing.isNotEmpty() }.forEach { subsystem ->
                content.addView(
                    keyValue(
                        subsystem.name,
                        subsystem.summary,
                        statusColor(subsystem.status),
                        trailing = subsystem.status,
                    )
                )
                subsystem.missing.forEach { content.addView(bullet(it)) }
            }
        }

        section(R.string.omni_settings_integrity)
        content.addView(mono(standing))
        content.addView(body(getString(R.string.omni_integrity_checked)))

        section(R.string.omni_settings_watch)
        val checked = Sentry.lastChecked(this)
        content.addView(
            mono(
                if (checked == 0L) {
                    Sentry.lastStanding(this)
                } else {
                    "${Sentry.lastStanding(this)} " +
                        SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date(checked))
                }
            )
        )
        content.addView(body(getString(R.string.omni_settings_watch_note)))

        section(R.string.omni_settings_logs)
        OmniLog.lastCopies().forEach { copy ->
            content.addView(
                keyValue(
                    copy.label,
                    copy.error ?: copy.location,
                    if (copy.succeeded) R.color.omni_ok else R.color.omni_warning,
                )
            )
        }
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
    )

    private fun createProject() {
        results.removeAllViews()
        val root = File(projectsFolder(), formLabel.trim().ifEmpty { "Project" })
        val image = formImage
        working({ Builder.nativeCreateProject(root.absolutePath, spec().encode()) }) finished@{ answer ->
            val outcome = runCatching { CreateOutcome.parse(answer) }.getOrElse {
                results.addView(banner(it.message ?: it.javaClass.simpleName, R.color.omni_error))
                return@finished
            }
            if (!outcome.created) {
                showRefusal(Refusal.parse(JSONObject(answer)), results)
                return@finished
            }
            OmniLog.event(LogLevel.INFO, "project", "Created ${outcome.root}")
            results.addView(banner(getString(R.string.omni_created), R.color.omni_ok))
            outcome.files.forEach { results.addView(bullet(it)) }
            if (image != null) {
                val stored = runCatching {
                    JSONObject(Builder.nativeSetIcon(root.absolutePath, image))
                }.getOrNull()
                if (stored != null && stored.optBoolean("stored", false)) {
                    results.addView(bullet(stored.optString("note")))
                } else {
                    stored?.let { showRefusal(Refusal.parse(it), results) }
                }
            }
            screen = Screen.Open(root.absolutePath)
        }
    }

    private fun createKey() {
        results.removeAllViews()
        val first = readSecret(keyPasswordView)
        val again = readSecret(keyPasswordAgainView)
        if (!first.contentEquals(again)) {
            first.fill(' ')
            again.fill(' ')
            results.addView(
                banner(getString(R.string.omni_key_password_mismatch), R.color.omni_error)
            )
            return
        }
        again.fill(' ')

        val request = KeySpec(
            alias = keyAlias.trim(),
            commonName = keyCommonName.trim(),
            organisation = keyOrganisation.trim(),
            country = keyCountry.trim().uppercase(Locale.US),
            validityDays = keyValidity.trim().toIntOrNull() ?: 0,
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
            screen = Screen.Projects
            render()
            results.addView(banner(made.alias, R.color.omni_ok))
            results.addView(bullet(made.fingerprint))
        }
    }

    private fun buildProject(root: String, keyPath: String) {
        results.removeAllViews()
        val password = readSecret(buildPasswordView)
        val destination = File(getExternalFilesDir(null) ?: filesDir, "${File(root).name}.apk")
        val started = System.nanoTime()

        working({
            Builder.nativeBuildProject(
                root,
                destination.absolutePath,
                keyPath,
                if (password.isEmpty()) null else password,
            )
        }) finished@{ answer ->
            password.fill(' ')
            buildPasswordView?.text?.clear()
            val elapsed = (System.nanoTime() - started) / 1_000_000
            val outcome = runCatching { BuildOutcome.parse(answer) }.getOrElse {
                results.addView(banner(it.message ?: it.javaClass.simpleName, R.color.omni_error))
                return@finished
            }
            if (!outcome.built) {
                OmniLog.event(LogLevel.ERROR, "build", "Refused: ${outcome.error}")
                results.addView(banner(getString(R.string.omni_refused), R.color.omni_error))
                showRefusal(Refusal.parse(JSONObject(answer)), results)
                outcome.findings.forEach { results.addView(bullet(it)) }
                return@finished
            }

            OmniLog.event(LogLevel.INFO, "build", "Built ${outcome.bytes} bytes in $elapsed ms")
            results.addView(banner("${outcome.bytes} / $elapsed ms", R.color.omni_ok))
            results.addView(
                keyValue(
                    getString(R.string.omni_result_contents),
                    if (outcome.carriesCode) {
                        "AndroidManifest.xml + classes.dex"
                    } else {
                        "AndroidManifest.xml"
                    },
                    R.color.omni_muted,
                    trailing = "${outcome.entries}",
                )
            )
            results.addView(
                keyValue(
                    getString(R.string.omni_result_signature),
                    "v2 + v3",
                    if (outcome.signed) R.color.omni_ok else R.color.omni_error,
                    trailing = if (outcome.signed) "OK" else "NONE",
                )
            )
            outcome.signedBy?.let {
                results.addView(
                    keyValue(getString(R.string.omni_result_signed_by), it, R.color.omni_muted)
                )
            }
            results.addView(
                keyValue(
                    getString(R.string.omni_result_policy),
                    "${outcome.rulesApplied}",
                    if (outcome.guardVerdict == "PASSED") R.color.omni_ok else R.color.omni_error,
                    trailing = outcome.guardVerdict ?: "?",
                )
            )
            outcome.path?.let { results.addView(mono(it)) }
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
                    banner(
                        it.message ?: getString(R.string.omni_form_image_none),
                        R.color.omni_error,
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
        render()
    }

    private fun working(work: () -> String, finished: (String) -> Unit) {
        results.removeAllViews()
        results.addView(banner(getString(R.string.omni_working), R.color.omni_accent))
        Thread {
            val answer = runCatching(work)
            runOnUiThread {
                results.removeAllViews()
                answer.fold(finished) { error ->
                    OmniLog.recordCrash(Thread.currentThread(), error)
                    results.addView(
                        banner(error.message ?: error.javaClass.simpleName, R.color.omni_error)
                    )
                }
            }
        }.start()
    }

    private fun report(answer: String, field: String) {
        val root = runCatching { JSONObject(answer) }.getOrNull() ?: return
        if (!root.optBoolean(field, false)) {
            showRefusal(Refusal.parse(root), results)
        }
    }

    private fun showRefusal(refusal: Refusal, into: LinearLayout) {
        val heading = listOfNotNull(refusal.code, refusal.message).joinToString(" ")
        into.addView(banner(heading.ifEmpty { getString(R.string.omni_refused) }, R.color.omni_error))
        refusal.context.forEach { into.addView(bullet(it)) }
        refusal.suggestion?.let { into.addView(body(it)) }
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

    override fun onPause() {
        super.onPause()
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        OmniLog.setPublishListener(null)
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun section(titleRes: Int) {
        content.addView(divider())
        content.addView(
            TextView(this).apply {
                text = getString(titleRes)
                setTextColor(color(R.color.omni_accent))
                setTypeface(Typeface.DEFAULT_BOLD)
                setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_section).toFloat())
                setPadding(0, dp(R.dimen.omni_gap), 0, dp(R.dimen.omni_gap_small))
            }
        )
    }

    private fun action(label: String, colorRes: Int = R.color.omni_ok, onPress: () -> Unit) =
        TextView(this).apply {
            text = label
            setTextColor(color(colorRes))
            setTypeface(typeface, Typeface.BOLD)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            gravity = Gravity.CENTER
            setPadding(dp(R.dimen.omni_gap))
            isClickable = true
            setOnClickListener { onPress() }
        }

    private fun row(title: String, detail: String, colorRes: Int, onPress: () -> Unit) =
        LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
            isClickable = true
            setOnClickListener { onPress() }
            addView(
                TextView(context).apply {
                    text = title
                    setTextColor(color(colorRes))
                    setTypeface(Typeface.MONOSPACE, Typeface.BOLD)
                    setTextSize(TypedValue.COMPLEX_UNIT_PX, dp(R.dimen.omni_text_body).toFloat())
                }
            )
            if (detail.isNotEmpty()) {
                addView(
                    TextView(context).apply {
                        text = detail
                        setTextColor(color(R.color.omni_muted))
                        setTextSize(
                            TypedValue.COMPLEX_UNIT_PX,
                            dp(R.dimen.omni_text_small).toFloat(),
                        )
                    }
                )
            }
        }

    private fun input(label: String, initial: String): Pair<View, EditText> {
        val holder = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, dp(R.dimen.omni_gap_small), 0, dp(R.dimen.omni_gap_small))
        }
        holder.addView(
            TextView(this).apply {
                text = label
                setTextColor(color(R.color.omni_muted))
                setTextSize(
                    TypedValue.COMPLEX_UNIT_PX,
                    resources.getDimension(R.dimen.omni_text_small),
                )
            }
        )
        val editor = EditText(this).apply {
            setText(initial)
            setTextColor(color(R.color.omni_foreground))
            setTextSize(
                TypedValue.COMPLEX_UNIT_PX,
                resources.getDimension(R.dimen.omni_text_body),
            )
            isSingleLine = true
        }
        holder.addView(editor)
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

    private fun secret(label: String): Pair<View, EditText> {
        val (holder, editor) = input(label, "")
        editor.inputType =
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
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
                setTextSize(
                    TypedValue.COMPLEX_UNIT_PX,
                    resources.getDimension(R.dimen.omni_text_body),
                )
                setPadding(
                    dp(R.dimen.omni_gap),
                    dp(R.dimen.omni_gap_small),
                    dp(R.dimen.omni_gap),
                    dp(R.dimen.omni_gap_small),
                )
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

    private fun statusColor(status: String) = when (status) {
        "PRODUCTION", "BETA" -> R.color.omni_ok
        "EXPERIMENTAL", "PARTIAL" -> R.color.omni_warning
        else -> R.color.omni_muted
    }

    private fun color(id: Int): Int = getColor(id)

    private fun dp(id: Int): Int = resources.getDimensionPixelSize(id)

    private fun View.setPadding(all: Int) = setPadding(all, all, all, all)
}
