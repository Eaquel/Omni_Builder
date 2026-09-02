package com.omni.builder

import android.animation.LayoutTransition
import android.app.Activity
import android.app.AlertDialog
import android.app.Application
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.ContentProvider
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.content.res.Resources
import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Bitmap
import android.graphics.BitmapFactory
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
import android.provider.OpenableColumns
import android.provider.Settings
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.util.Log
import android.util.TypedValue
import android.view.inputmethod.EditorInfo
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
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
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import org.json.JSONObject

enum class LogLevel {
    TRACE,

    INFO,

    WARN,

    ERROR,
}

object OmniLog {

    const val DIRECTORY_NAME: String = "Omni_Builder"

    const val SESSION_FILE: String = "Session_Log.txt"

    const val CRASH_FILE: String = "Crash_Log.txt"

    const val MAX_BYTES: Int = 256 * 1024

    private const val TAG = "OmniBuilder"

    private val lock = Any()
    private val session = StringBuilder(8 * 1024)

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

    fun flushSession(): String {
        val started = System.nanoTime()
        val where = write(SESSION_FILE, sessionDocument(), append = false)
        val elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000
        event(LogLevel.TRACE, "log", "Session written in $elapsedMilliseconds ms to $where")
        return where
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

    /** Everything this run has recorded, newest last, for the screen that shows it. */
    fun transcript(): String = synchronized(lock) { session.toString() }

    fun clearTranscript() {
        synchronized(lock) { session.setLength(0) }
        event(LogLevel.INFO, "log", "The log was cleared from the screen.")
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

    /**
     * The log is written into this application's own storage and nowhere else.
     * It used to be copied out into shared Documents as well, which put files on
     * the device that nobody asked for; the screen in Settings shows the same
     * text and copies it where you want it.
     */
    private fun write(name: String, text: String, append: Boolean): String =
        writePrivate(name, text, append)

    private fun writeBlocking(name: String, text: String, append: Boolean): String =
        writePrivate(name, text, append)

    private fun Exception.describe(): String =
        "${javaClass.simpleName}: ${message ?: "no detail"}"

    private fun writePrivate(name: String, text: String, append: Boolean): String {
        val target = privateFile(name) ?: return "(not started)"
        return try {
            FileOutputStream(target, append).use { it.write(text.toByteArray(Charsets.UTF_8)) }
            trim(target)
            target.absolutePath
        } catch (failure: Exception) {
            Log.e(TAG, "The log could not be written.", failure)
            "(unwritable: ${failure.describe()})"
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

}

class BuilderApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        OmniLog.install(this)
        bindTheSharedKeyToThisDevice()
        Sentry.arm(this)
    }

    /**
     * Ties the shared signing key to this installation on this device.
     *
     * The password on the shared key is written in this application and shown
     * to whoever asks, because it is a way to start rather than a secret. What
     * keeps the key itself worth having is that the file is also sealed with
     * something that does not travel: the identifier Android gives this
     * installation, which is different on every device and for every
     * application signing key, and which is not in this source.
     *
     * A key that leaves this device -- in a backup, in a folder somebody
     * shared, in any of the ways a file ends up somewhere it was not meant to
     * -- does not open there, published password or not.
     *
     * If Android will not say, the Core is told so and falls back to the
     * published password alone. That is the behaviour every version before
     * this one had, so nothing stops working; it is simply not bound.
     */
    private fun bindTheSharedKeyToThisDevice() {
        val identifier = runCatching {
            Settings.Secure.getString(contentResolver, Settings.Secure.ANDROID_ID)
        }.getOrNull().orEmpty()

        val answer = runCatching { Builder.nativeBindDevice(identifier) }.getOrNull()
        val bound = answer?.contains("\"bound\":true") == true
        OmniLog.event(
            LogLevel.INFO,
            "keys",
            if (bound) {
                "The shared key is sealed to this device as well as to its password."
            } else {
                "Android would not identify this device, so the shared key is sealed " +
                    "to its password alone."
            },
        )
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

    /**
     * Where the build running right now has got to.
     *
     * Answers at any time and from any thread. A build that is not running
     * says so; one that is says which stage it is in, how far through it is,
     * and how long is left. Polled while a build runs, so it does nothing but
     * read a handful of numbers.
     */
    external fun nativeBuildProgress(): String

    /**
     * Hands the timings of the last build back before the next one starts,
     * which is what turns a guess into this device's own measurement.
     */
    external fun nativeBuildExpect(timings: String?)

    external fun nativeVerifySelf(packagePath: String, expectedCertificate: String?): String

    external fun nativeCreateKey(directory: String, spec: String, keyPassword: CharArray): String

    external fun nativeDefaultKey(directory: String): String

    external fun nativeBindDevice(secret: String): String

    external fun nativeListKeys(directory: String): String

    external fun nativeDeleteKey(path: String): String

    external fun nativeCheckKey(path: String, keyPassword: CharArray): String

    external fun nativeListProjects(directory: String): String

    external fun nativeProjectTree(root: String): String

    /**
     * Looks through every text file in a project for a piece of text.
     *
     * The search is over the project as it is on disk, so what it finds is
     * what a build would compile rather than what an editor is showing.
     */
    external fun nativeSearchProject(
        root: String,
        needle: String,
        sensitive: Boolean,
        wholeWord: Boolean,
    ): String

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
    /**
     * How long each stage took, in microseconds, comma separated.
     *
     * Only a build that finished has a full set of these, so only a build
     * that finished is worth keeping them from.
     */
    val timings: String?,
    /**
     * What re-reading the finished package said about its own signature.
     *
     * The Core opens what it wrote and checks it the way an installer would,
     * so these are measurements of the file on disk rather than a note that
     * the signing code returned without complaining.
     */
    val signatureSchemes: List<String>,
    val signaturesVerified: Long,
    val signatureKeyMatches: Boolean,
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
                timings = root.optString("timings").ifEmpty { null },
                signatureSchemes = packaged?.optJSONObject("verified")
                    ?.optJSONArray("schemes")
                    ?.let { array -> (0 until array.length()).map { array.getString(it) } }
                    .orEmpty(),
                signaturesVerified =
                    packaged?.optJSONObject("verified")?.optLong("signaturesVerified") ?: 0L,
                signatureKeyMatches = packaged?.optJSONObject("verified")
                    ?.optBoolean("keyMatchesCertificate", false) ?: false,
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

        /**
         * The one this application was drawn for, and the one it opens in.
         *
         * It is not a dark grey with a blue accent. The ground is the near
         * black of a screen with nothing lit on it, the surfaces above it are
         * separated by four points of brightness rather than by borders, and
         * the accent is the cyan a cathode tube throws rather than the blue
         * every framework ships. The three glows are what the aurora behind
         * the content is painted from: a deep sea blue, a green that only
         * shows where it overlaps, and a violet that never quite arrives.
         */
        val FORGE = Palette(
            key = "forge",
            label = "Forge",
            background = 0xFF04070C.toInt(),
            surface = 0xFF0A0F17.toInt(),
            raised = 0xFF101724.toInt(),
            foreground = 0xFFDCE9F5.toInt(),
            muted = 0xFF6F7F94.toInt(),
            accent = 0xFF35B7FF.toInt(),
            ok = 0xFF2FE0B0.toInt(),
            warning = 0xFFF2B441.toInt(),
            error = 0xFFFF5C6C.toInt(),
            divider = 0xFF16202E.toInt(),
            glowFirst = 0xFF0B3F6E.toInt(),
            glowSecond = 0xFF0A5646.toInt(),
            glowThird = 0xFF2A1B4E.toInt(),
        )

        val ALL: List<Palette> = listOf(FORGE, MIDNIGHT, SLATE, DAYLIGHT)

        fun of(key: String): Palette = ALL.firstOrNull { it.key == key } ?: FORGE
    }
}

object Preferences {

    private const val FILE = "omni_settings"
    private const val LANGUAGE = "language"
    private const val THEME = "theme"
    private const val SIGNING_KEY = "signing_key"
    private const val TIMINGS = "build_timings"

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
        Palette.of(store(context).getString(THEME, Palette.FORGE.key).orEmpty())

    fun setPalette(context: Context, key: String) {
        store(context).edit().putString(THEME, key).apply()
    }

    fun signingKey(context: Context): String = store(context).getString(SIGNING_KEY, "").orEmpty()

    fun setSigningKey(context: Context, path: String) {
        store(context).edit().putString(SIGNING_KEY, path).apply()
    }

    /**
     * How long each stage of the last successful build took on this device.
     *
     * The Core hands this back at the end of every build it finished, and it
     * is given straight back to the Core at the start of the next one, which
     * is what turns the estimate on the build screen from a guess made on
     * some other machine into a measurement made on this one. It is a
     * measurement of this phone and nothing else, so it is kept here rather
     * than anywhere a project could carry it somewhere new.
     */
    fun timings(context: Context): String = store(context).getString(TIMINGS, "").orEmpty()

    fun setTimings(context: Context, measured: String) {
        store(context).edit().putString(TIMINGS, measured).apply()
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

/** Lays its children out left to right, starting a new line when one will not fit. */
class FlowLayout(
    context: Context,
    private val betweenX: Int,
    private val betweenY: Int,
) : ViewGroup(context) {

    private fun measureRows(width: Int, place: Boolean): Int {
        val limit = width - paddingLeft - paddingRight
        var x = paddingLeft
        var y = paddingTop
        var tallest = 0
        for (index in 0 until childCount) {
            val child = getChildAt(index)
            if (child.visibility == GONE) {
                continue
            }
            if (x > paddingLeft && x - paddingLeft + child.measuredWidth > limit) {
                x = paddingLeft
                y += tallest + betweenY
                tallest = 0
            }
            if (place) {
                child.layout(x, y, x + child.measuredWidth, y + child.measuredHeight)
            }
            x += child.measuredWidth + betweenX
            tallest = maxOf(tallest, child.measuredHeight)
        }
        return y + tallest + paddingBottom
    }

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        val width = MeasureSpec.getSize(widthSpec)
        val room = MeasureSpec.makeMeasureSpec(
            (width - paddingLeft - paddingRight).coerceAtLeast(0),
            MeasureSpec.AT_MOST,
        )
        for (index in 0 until childCount) {
            val child = getChildAt(index)
            if (child.visibility != GONE) {
                child.measure(room, MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
            }
        }
        setMeasuredDimension(width, measureRows(width, false))
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        measureRows(right - left, true)
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

    data class Search(val root: String) : Screen {
        override val tab = Tab.FILES
    }

    data class Picture(val root: String, val path: String) : Screen {
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
        const val IMAGE_REQUEST = 2
        const val ENTER_MILLIS = 260L
        const val LEAVE_MILLIS = 140L
        /** How long a refusal is left on a ceremony before it comes down. */
        const val REFUSAL_MILLIS = 1_400L
        /** How often a ceremony is asked whether it is finished. */
        const val LOOK_MILLIS = 60L
        /** How often the Core is asked where the build has got to. */
        const val WATCH_MILLIS = 40L
        /** How long the finished figure is left on screen before it comes down. */
        const val SETTLE_MILLIS = 620L

        /** The Core's build stages, in its order, in this application's words. */
        val STAGE_NAMES = listOf(
            R.string.omni_stage_project,
            R.string.omni_stage_resources,
            R.string.omni_stage_java,
            R.string.omni_stage_manifest,
            R.string.omni_stage_dex,
            R.string.omni_stage_package,
            R.string.omni_stage_signing,
            R.string.omni_stage_bundle,
            R.string.omni_stage_verify,
        )
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

        /**
         * The one machine anything built here runs on.
         *
         * There was a chooser with three answers. Two of them produced packages
         * for devices Google Play has not accepted since 2019 and Android has
         * shipped without since 2023, and every one of them was a second build
         * to make and a second thing nobody tested on. Asking a question with
         * one right answer is not a choice; it is a way to get it wrong.
         */
        val ONLY_ABI = listOf("arm64-v8a")

        val ANDROID_RELEASES = listOf(
            30 to "11", 31 to "12", 32 to "12L",
            33 to "13", 34 to "14", 35 to "15", 36 to "16",
        )
        val LANGUAGE_CHOICES = listOf(
            "kotlin" to "Kotlin",
            "java" to "Java",
            "cpp" to "C++",
            "rust" to "Rust",
        )
        const val LARGEST_ICON_EDGE = 512
        const val PROJECT_RES = "Res"
        const val PROJECT_ICON = "Icon.png"
        const val FOLDER_MARK = "\u25B8"
        const val FILE_MARK = "\u2022"
        const val MENU_MARK = "\u22EE"

        val PICTURE_SUFFIXES = listOf(".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp")

        val KEY_SIZES = listOf(2048, 3072, 4096)
        val VALIDITY_YEARS = listOf(1, 5, 10, 25, 100)
    }

    private lateinit var palette: Palette
    private lateinit var aurora: AuroraView
    private lateinit var veil: BinaryVeil
    private lateinit var ceremony: FrameLayout
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
    private var formMinSdk = 30
    private var formTargetSdk = 36
    private val formLanguages = linkedSetOf("kotlin")
    private val formLocales = Preferences.LANGUAGES.mapTo(linkedSetOf()) { it.first }
    private var formImage: String? = null
    private var imageForProject: String? = null
    private var held: String? = null

    private var keyAlias = DEFAULT_KEY_ALIAS
    private var keyCommonName = DEFAULT_KEY_COMMON_NAME
    private var keyOrganisation = DEFAULT_KEY_ORGANISATION
    private var keyCountry = DEFAULT_KEY_COUNTRY
    private var keyYears = VALIDITY_YEARS.indexOf(DEFAULT_KEY_YEARS)
    private var keyBits = KEY_SIZES.indexOf(4096)
    private var keyPasswordView: EditText? = null
    private var keyPasswordAgainView: EditText? = null
    private var buildPasswordView: EditText? = null

    private var searchFor = ""
    private var searchCase = false
    private var searchWord = false

    private var editorText = ""
    private var editor: CodeEditor? = null
    /** The project and the file the open editor is showing, if one is open. */
    private var editorAt: Pair<String, String>? = null
    /** What the open file held when it was read, so a draft knows it differs. */
    private var editorOnDisk = ""

    override fun attachBaseContext(base: Context) {
        // Nothing chosen means the language the phone is set to, which is what
        // the base context already carries.
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

        palette = Preferences.palette(this)
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

        // A ceremony covers the application while it runs, and the veil
        // covers the ceremony too: both sit above the shell rather than
        // inside it, so no screen has to know either of them exists.
        ceremony = FrameLayout(this).apply {
            visibility = View.GONE
            isClickable = true
            setBackgroundColor(palette.background)
        }
        veil = BinaryVeil(this).apply { visibility = View.GONE }

        val stack = FrameLayout(this).apply {
            setBackgroundColor(palette.background)
            addView(aurora, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(shell, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(ceremony, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(veil, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
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

    /**
     * Puts whatever is in the open editor into its draft, now.
     *
     * The editor writes a draft after a pause in typing, which covers a person
     * who stops to think. It does not cover a person who is closed by Android
     * a keystroke later, so leaving the screen and leaving the application
     * both come through here first.
     */
    private fun keepTheDraft() {
        val open = editor ?: return
        val (root, path) = editorAt ?: return
        val held = open.text?.toString().orEmpty()
        if (held != editorOnDisk) {
            Drafts.write(this, root, path, held)
        }
    }

    override fun onPause() {
        keepTheDraft()
        aurora.pauseDrawing()
        super.onPause()
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        keepTheDraft()
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun examine(): String =
        if (Sentry.refused(this)) "TAMPERED" else Sentry.check(this)

    /**
     * Goes to another screen, through the field of bits.
     *
     * The screen being left fades down while a wipe of 0s and 1s crosses the
     * display; the swap happens under the brightest part of that wipe, and the
     * screen being arrived at rises into place behind it. Nothing waits on
     * anything: the whole thing is over in under half a second, and a second
     * press during it lands on the screen that is arriving.
     */
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
            .start()
        veil.sweep(palette.accent) {
            render(true)
            scroller.scrollTo(0, 0)
        }
    }

    /**
     * Puts a ceremony over the whole application until it is finished.
     *
     * What was on screen underneath is left exactly as it was. A ceremony is
     * something happening on top of the application rather than a screen of
     * its own, so when it ends the person is looking at what they were looking
     * at before it started.
     */
    private fun showCeremony(view: View) {
        ceremony.removeAllViews()
        ceremony.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        ceremony.alpha = 0f
        ceremony.visibility = View.VISIBLE
        ceremony.animate().alpha(1f).setDuration(ENTER_MILLIS).start()
    }

    private fun hideCeremony(then: () -> Unit) {
        ceremony.animate()
            .alpha(0f)
            .setDuration(LEAVE_MILLIS)
            .withEndAction {
                ceremony.removeAllViews()
                ceremony.visibility = View.GONE
                then()
            }
            .start()
    }

    private fun ceremonyIsUp(): Boolean = ceremony.visibility == View.VISIBLE

    private fun render(animated: Boolean) {
        keepTheDraft()
        editor = null
        editorAt = null
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
            is Screen.Search -> renderSearch(here.root)
            is Screen.Editor -> renderEditor(here.root, here.path)
            is Screen.Picture -> renderPicture(here.root, here.path)
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
            is Screen.Picture -> go(Screen.Files(here.root, here.path.substringBeforeLast('/', "")))
            is Screen.Files ->
                if (here.folder.isEmpty()) {
                    go(Screen.Projects)
                } else {
                    go(Screen.Files(here.root, here.folder.substringBeforeLast('/', "")))
                }
            is Screen.Search -> go(Screen.Files(here.root, ""))
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
        if (openProject != root) {
            // What was picked up in one project has no place in another.
            held = null
        }
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
        picture.addView(subtle(getString(R.string.omni_form_image_choose), palette.accent) {
            chooseImage(null)
        })
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

        val identity = card()
        val face = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, gap(1), 0, gap(1))
            isClickable = true
            background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
            setOnClickListener { chooseImage(root) }
        }
        face.addView(
            thumbnail(File(root, "${'$'}{PROJECT_RES}/${'$'}{PROJECT_ICON}").absolutePath, gap(12)),
            LinearLayout.LayoutParams(gap(12), gap(12)).apply { marginEnd = gap(3) },
        )
        face.addView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                addView(
                    TextView(context).apply {
                        text = getString(R.string.omni_form_image)
                        setTextColor(palette.foreground)
                        setTypeface(Typeface.DEFAULT_BOLD)
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                    }
                )
                addView(
                    TextView(context).apply {
                        text = summary?.let {
                            "${'$'}{it.packageName}  ·  ${'$'}{it.versionName}  ·  " +
                                "API ${'$'}{it.minSdk}–${'$'}{it.targetSdk}"
                        } ?: getString(R.string.omni_form_image_choose)
                        setTextColor(palette.muted)
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                        setLineSpacing(gap(1).toFloat(), 1f)
                    }
                )
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
        )
        face.addView(
            TextView(this).apply {
                text = getString(R.string.omni_action_change)
                setTextColor(palette.accent)
                setTypeface(Typeface.DEFAULT_BOLD)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                letterSpacing = 0.08f
            }
        )
        identity.addView(face)
        content.addView(identity)

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
            tree.addView(fileRow(root, folder, entry, entries))
        }
        content.addView(tree)

        val making = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, gap(1), 0, gap(1))
        }
        making.addView(
            subtle(getString(R.string.omni_action_new_file), palette.accent) {
                askForName(getString(R.string.omni_action_new_file), "") { name ->
                    act(Builder.nativeWriteFile(root, joined(folder, name), ""), "saved")
                }
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginEnd = gap(1) },
        )
        making.addView(
            subtle(getString(R.string.omni_action_new_folder), palette.accent) {
                askForName(getString(R.string.omni_action_new_folder), "") { name ->
                    act(Builder.nativeNewFolder(root, joined(folder, name)), "made")
                }
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginStart = gap(1) },
        )
        content.addView(making)

        held?.let { moving ->
            content.addView(
                primary(
                    getString(R.string.omni_files_paste, moving.substringAfterLast('/'))
                ) {
                    val target = joined(folder, moving.substringAfterLast('/'))
                    held = null
                    act(Builder.nativeRenamePath(root, moving, target), "moved")
                }
            )
            content.addView(subtle(getString(R.string.omni_action_cancel), palette.muted) {
                held = null
                render(false)
            })
        }

        content.addView(subtle(getString(R.string.omni_search_title), palette.accent) {
            go(Screen.Search(root))
        })
        content.addView(primary(getString(R.string.omni_action_build)) { go(Screen.Build(root)) })
        content.addView(quiet(getString(R.string.omni_trash_note)))
    }

    /**
     * Every place in a project a piece of text appears.
     *
     * The searching is the Core's: it reads the files off disk, which is what
     * a build reads, so what turns up here is what is really in the project
     * rather than what some index last saw. What it did not look at -- files
     * too large, files that are not text -- is said rather than left out.
     */
    private fun renderSearch(root: String) {
        content.addView(heading(getString(R.string.omni_search_title)))

        val field = EditText(this).apply {
            setText(searchFor)
            hint = getString(R.string.omni_search_hint)
            setHintTextColor(palette.muted)
            setTextColor(palette.foreground)
            setTypeface(Typeface.MONOSPACE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            setSingleLine()
            imeOptions = EditorInfo.IME_ACTION_SEARCH
            background = sheet(palette.raised, palette.divider)
            setPadding(gap(3), gap(3), gap(3), gap(3))
        }
        content.addView(field)

        content.addView(
            chips(
                listOf(
                    getString(R.string.omni_search_case),
                    getString(R.string.omni_search_word),
                ),
                { index -> if (index == 0) searchCase else searchWord },
            ) { index ->
                if (index == 0) searchCase = !searchCase else searchWord = !searchWord
            }
        )

        fun look() {
            searchFor = field.text.toString()
            results.removeAllViews()
            if (searchFor.isEmpty()) {
                return
            }
            working({
                Builder.nativeSearchProject(root, searchFor, searchCase, searchWord)
            }) finished@{ answer ->
                val document = runCatching { JSONObject(answer) }.getOrNull() ?: return@finished
                if (!document.optBoolean("searched", false)) {
                    showRefusal(Refusal.parse(document), results)
                    return@finished
                }
                showFound(root, document.optJSONObject("result"))
            }
        }

        field.setOnEditorActionListener { _, _, _ ->
            look()
            true
        }
        content.addView(primary(getString(R.string.omni_search_go)) { look() })
        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Files(root, ""))
        })
    }

    private fun showFound(root: String, result: JSONObject?) {
        if (result == null) {
            return
        }
        val found = result.optJSONArray("found")
        val count = found?.length() ?: 0
        if (count == 0) {
            results.addView(notice(getString(R.string.omni_search_none), palette.muted))
        } else {
            results.addView(
                notice(
                    getString(
                        if (result.optBoolean("stoppedEarly", false)) {
                            R.string.omni_search_capped
                        } else {
                            R.string.omni_search_found
                        },
                        count,
                        result.optInt("filesSearched"),
                    ),
                    palette.ok,
                )
            )
        }

        // One card per file, so twelve hits in one file read as one place to
        // look rather than as twelve.
        var lastPath = ""
        var card: LinearLayout? = null
        for (index in 0 until count) {
            val one = found?.optJSONObject(index) ?: continue
            val path = one.optString("path")
            if (path != lastPath) {
                lastPath = path
                card = card()
                card.addView(quiet(path))
                results.addView(card)
            }
            card?.addView(
                row(
                    one.optString("text").trim().take(120),
                    "",
                    one.optInt("line").toString() + ":" + one.optInt("column"),
                    palette.accent,
                ) {
                    go(Screen.Editor(root, path))
                }
            )
        }

        val quiet = listOfNotNull(
            result.optInt("filesTooLarge").takeIf { it > 0 }
                ?.let { getString(R.string.omni_search_too_large, it) },
            result.optInt("filesNotText").takeIf { it > 0 }
                ?.let { getString(R.string.omni_search_not_text, it) },
        )
        quiet.forEach { results.addView(quiet(it)) }
    }

    private fun joined(folder: String, name: String): String =
        if (folder.isEmpty()) name else "${'$'}folder/${'$'}name"

    private fun looksLikeAPicture(path: String): Boolean {
        val lower = path.lowercase(Locale.US)
        return PICTURE_SUFFIXES.any { lower.endsWith(it) }
    }

    /** Decodes a picture small enough to sit in a row, or hands back a plain tile. */
    private fun thumbnail(path: String, edge: Int): View {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        runCatching { BitmapFactory.decodeFile(path, bounds) }
        if (bounds.outWidth > 0 && bounds.outHeight > 0) {
            val options = BitmapFactory.Options()
            var step = 1
            while (bounds.outWidth / (step * 2) >= edge && bounds.outHeight / (step * 2) >= edge) {
                step *= 2
            }
            options.inSampleSize = step
            val decoded = runCatching { BitmapFactory.decodeFile(path, options) }.getOrNull()
            if (decoded != null) {
                return ImageView(this).apply {
                    setImageBitmap(decoded)
                    scaleType = ImageView.ScaleType.FIT_CENTER
                    background = pill(palette.raised, gap(2).toFloat())
                    setPadding(gap(1))
                }
            }
        }
        return View(this).apply { background = pill(palette.raised, gap(2).toFloat()) }
    }

    private fun fileRow(
        root: String,
        folder: String,
        entry: FileEntry,
        all: List<FileEntry>,
    ): View {
        val name = entry.path.substringAfterLast('/')
        val absolute = File(root, entry.path).absolutePath
        val picture = !entry.folder && looksLikeAPicture(entry.path)
        val inside = all.count { it.path.startsWith("${'$'}{entry.path}/") }

        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, gap(2), 0, gap(2))
            isClickable = true
            background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
            setOnClickListener { openEntry(root, entry) }
            setOnLongClickListener {
                showActions(root, folder, entry)
                true
            }

            addView(
                if (picture) {
                    thumbnail(absolute, gap(9))
                } else {
                    TextView(context).apply {
                        text = if (entry.folder) FOLDER_MARK else FILE_MARK
                        setTextColor(if (entry.folder) palette.accent else palette.muted)
                        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
                        gravity = Gravity.CENTER
                        background = pill(palette.raised, gap(2).toFloat())
                    }
                },
                LinearLayout.LayoutParams(gap(9), gap(9)).apply { marginEnd = gap(3) },
            )

            addView(
                LinearLayout(context).apply {
                    orientation = LinearLayout.VERTICAL
                    addView(
                        TextView(context).apply {
                            text = name
                            setTextColor(
                                if (entry.path == held) palette.warning else palette.foreground
                            )
                            setTypeface(Typeface.DEFAULT_BOLD)
                            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                        }
                    )
                    addView(
                        TextView(context).apply {
                            text = when {
                                entry.path == held -> getString(R.string.omni_files_holding)
                                entry.folder -> getString(R.string.omni_files_holds, inside)
                                else -> size(entry.bytes)
                            }
                            setTextColor(palette.muted)
                            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                        }
                    )
                },
                LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
            )

            addView(
                TextView(context).apply {
                    text = MENU_MARK
                    setTextColor(palette.muted)
                    setTypeface(Typeface.DEFAULT_BOLD)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
                    gravity = Gravity.CENTER
                    isClickable = true
                    background = touchable(
                        pill(Color.TRANSPARENT, gap(5).toFloat()),
                        palette.accent,
                    )
                    setOnClickListener { showActions(root, folder, entry) }
                    layoutParams = LinearLayout.LayoutParams(gap(10), gap(10))
                }
            )
        }
    }

    private fun openEntry(root: String, entry: FileEntry) {
        when {
            entry.folder -> go(Screen.Files(root, entry.path))
            looksLikeAPicture(entry.path) -> go(Screen.Picture(root, entry.path))
            else -> {
                editorText = ""
                go(Screen.Editor(root, entry.path))
            }
        }
    }

    private fun showActions(root: String, folder: String, entry: FileEntry) {
        val name = entry.path.substringAfterLast('/')
        val actions = mutableListOf<Pair<String, () -> Unit>>()

        actions.add(getString(R.string.omni_action_open) to { openEntry(root, entry) })
        actions.add(
            getString(R.string.omni_action_rename) to {
                askForName(getString(R.string.omni_action_rename), name) { chosen ->
                    act(
                        Builder.nativeRenamePath(root, entry.path, joined(folder, chosen)),
                        "moved",
                    )
                }
            }
        )
        actions.add(
            getString(R.string.omni_action_move) to {
                held = entry.path
                render(false)
                results.addView(
                    notice(getString(R.string.omni_files_held, name), palette.accent)
                )
            }
        )
        if (!entry.folder) {
            actions.add(
                getString(R.string.omni_action_share) to {
                    shareOutside(File(root, entry.path))
                }
            )
        }
        actions.add(
            getString(R.string.omni_action_delete) to {
                act(Builder.nativeRemovePath(root, entry.path, trashFolder()), "removed")
            }
        )

        AlertDialog.Builder(this)
            .setTitle(if (entry.folder) "${'$'}name/" else name)
            .setItems(actions.map { it.first }.toTypedArray()) { _, which ->
                actions[which].second()
            }
            .show()
    }

    private fun askForName(title: String, initial: String, onChosen: (String) -> Unit) {
        val editor = EditText(this).apply {
            setText(initial)
            setSelection(initial.length)
            isSingleLine = true
            setTextColor(palette.foreground)
            setPadding(gap(4), gap(3), gap(4), gap(3))
        }
        AlertDialog.Builder(this)
            .setTitle(title)
            .setView(editor)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val chosen = editor.text.toString().trim()
                if (chosen.isEmpty()) {
                    results.removeAllViews()
                    results.addView(
                        notice(getString(R.string.omni_files_needs_name), palette.warning)
                    )
                } else {
                    onChosen(chosen)
                }
            }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
        editor.requestFocus()
    }

    private fun renderPicture(root: String, path: String) {
        content.addView(heading(path.substringAfterLast('/')))
        content.addView(quiet(path))

        val file = File(root, path)
        val sheet = card()
        sheet.addView(
            thumbnail(file.absolutePath, gap(64)),
            LinearLayout.LayoutParams(MATCH_PARENT, gap(64)).apply {
                topMargin = gap(2)
                bottomMargin = gap(2)
            },
        )

        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        runCatching { BitmapFactory.decodeFile(file.absolutePath, bounds) }
        sheet.addView(
            keyValue(
                getString(R.string.omni_picture_size),
                if (bounds.outWidth > 0) "${'$'}{bounds.outWidth} × ${'$'}{bounds.outHeight}" else "",
                size(file.length()),
                palette.muted,
            )
        )
        content.addView(sheet)

        content.addView(subtle(getString(R.string.omni_action_share), palette.accent) {
            shareOutside(file)
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
        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Files(root, path.substringBeforeLast('/', "")))
        })
    }

    private fun renderEditor(root: String, path: String) {
        content.addView(heading(path.substringAfterLast('/')))
        val where = TextView(this).apply {
            text = path
            setTextColor(palette.muted)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            setPadding(0, 0, 0, gap(1))
        }
        content.addView(where)

        val answer = runCatching { JSONObject(Builder.nativeReadFile(root, path)) }.getOrNull()
        if (answer == null || !answer.optBoolean("read", false)) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
                go(Screen.Files(root, path.substringBeforeLast('/', "")))
            })
            return
        }
        val onDisk = answer.optString("text")
        editorOnDisk = onDisk
        // Whatever was typed here last time and never saved, offered back.
        val draft = Drafts.read(this, root, path)?.takeIf { it != onDisk }
        editorText = draft ?: onDisk

        val editor = CodeEditor(this, palette).apply {
            setPadding(gap(9), gap(2), gap(3), gap(2))
            open(path.substringAfterLast('/'), editorText)
        }
        this.editor = editor
        this.editorAt = root to path
        if (draft != null) {
            content.addView(notice(getString(R.string.omni_editor_restored), palette.warning))
        }

        // What is on screen and what is on disk, said in one place. The row
        // under the name is the path when they agree and the count of what is
        // unsaved when they do not.
        var dirty = draft != null
        if (dirty) {
            where.text = getString(R.string.omni_editor_unsaved)
            where.setTextColor(palette.warning)
        }
        // Written after a pause rather than after every keystroke: a draft is
        // insurance, and insurance that runs on the typing thread is a cost.
        val keep = Runnable { Drafts.write(this, root, path, editorText) }
        fun mark() {
            editorText = editor.text?.toString().orEmpty()
            val changed = editorText != onDisk
            if (changed != dirty) {
                dirty = changed
                where.text = if (changed) getString(R.string.omni_editor_unsaved) else path
                where.setTextColor(if (changed) palette.warning else palette.muted)
            }
            editor.removeCallbacks(keep)
            if (changed) {
                editor.postDelayed(keep, Drafts.REST_MILLIS)
            } else {
                Drafts.forget(this, root, path)
            }
        }
        editor.onChanged = ::mark

        val tools = FlowLayout(this, gap(2), gap(2)).apply { setPadding(0, gap(1), 0, gap(1)) }
        tools.addView(tool(getString(R.string.omni_editor_undo), palette.accent) {
            editor.undo()
            mark()
        })
        tools.addView(tool(getString(R.string.omni_editor_redo), palette.accent) {
            editor.redo()
            mark()
        })
        tools.addView(tool(getString(R.string.omni_action_copy), palette.foreground) {
            copyOut(selectionOf(editor), path.substringAfterLast('/'))
        })
        tools.addView(tool(getString(R.string.omni_editor_paste), palette.foreground) {
            pasteInto(editor)
            mark()
        })
        content.addView(tools)

        val sheet = card()
        sheet.addView(
            editor,
            LinearLayout.LayoutParams(
                MATCH_PARENT,
                (resources.displayMetrics.heightPixels * 0.52f).toInt(),
            ),
        )
        content.addView(sheet)

        content.addView(primary(getString(R.string.omni_action_save)) {
            results.removeAllViews()
            editorText = editor.text?.toString().orEmpty()
            val saved = runCatching { JSONObject(Builder.nativeWriteFile(root, path, editorText)) }
                .getOrNull()
            if (saved != null && saved.optBoolean("saved", false)) {
                dirty = false
                where.text = path
                where.setTextColor(palette.muted)
                editor.removeCallbacks(keep)
                Drafts.forget(this, root, path)
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
                editor.removeCallbacks(keep)
                Drafts.forget(this, root, path)
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

    /** What is selected, or the whole file when nothing is. */
    private fun selectionOf(editor: CodeEditor): String {
        val held = editor.text?.toString().orEmpty()
        val from = editor.selectionStart
        val to = editor.selectionEnd
        if (from < 0 || to < 0 || from == to) return held
        return held.substring(min(from, to), max(from, to))
    }

    private fun copyOut(text: String, label: String) {
        val clipboard = getSystemService(ClipboardManager::class.java) ?: return
        clipboard.setPrimaryClip(ClipData.newPlainText(label, text))
        results.removeAllViews()
        results.addView(notice(getString(R.string.omni_settings_logs_copied), palette.ok))
    }

    /** Puts the clipboard where the caret is, replacing what is selected. */
    private fun pasteInto(editor: CodeEditor) {
        val clipboard = getSystemService(ClipboardManager::class.java) ?: return
        val held = clipboard.primaryClip?.takeIf { it.itemCount > 0 }
            ?.getItemAt(0)
            ?.coerceToText(this)
            ?.toString()
            .orEmpty()
        if (held.isEmpty()) return
        val editable = editor.text ?: return
        val from = min(editor.selectionStart, editor.selectionEnd).coerceIn(0, editable.length)
        val to = max(editor.selectionStart, editor.selectionEnd).coerceIn(0, editable.length)
        editable.replace(from, to, held)
        editor.setSelection((from + held.length).coerceIn(0, editor.text?.length ?: 0))
    }

    /** A small button for a row of them, as against one across the screen. */
    private fun tool(text: String, colour: Int, onPress: () -> Unit) = TextView(this).apply {
        this.text = text
        setTextColor(colour)
        setTypeface(Typeface.DEFAULT_BOLD)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        gravity = Gravity.CENTER
        maxLines = 1
        setPadding(gap(4), gap(2), gap(4), gap(2))
        background = touchable(pill(palette.raised, gap(4).toFloat()), colour)
        isClickable = true
        setOnClickListener {
            onPress()
            animate().scaleX(0.94f).scaleY(0.94f).setDuration(70L).withEndAction {
                animate().scaleX(1f).scaleY(1f).setDuration(110L).start()
            }.start()
        }
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
            val file = File(one.path)
            shelf.addView(
                keyValue(
                    one.name,
                    "${size(one.bytes)}  ·  ${moment(one.writtenAt)}",
                    if (one.bundle) "AAB" else "APK",
                    if (one.bundle) palette.muted else palette.ok,
                )
            )
            actionsFor(file, shelf)
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
        val current = Preferences.language(this).ifEmpty { deviceLanguage() }
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

        content.addView(heading(getString(R.string.omni_settings_logs)))
        val transcript = OmniLog.transcript().trimEnd()
        val logging = card()
        logging.addView(
            EditText(this).apply {
                setText(
                    transcript.ifEmpty { getString(R.string.omni_settings_logs_empty) }
                )
                setTextColor(palette.foreground)
                setBackgroundColor(Color.TRANSPARENT)
                setTypeface(Typeface.MONOSPACE)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                gravity = Gravity.TOP or Gravity.START
                setHorizontallyScrolling(false)
                isFocusable = true
                setTextIsSelectable(true)
                keyListener = null
                minLines = 6
                maxLines = 18
                setPadding(gap(2))
            }
        )
        content.addView(logging)

        val logActions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
        }
        logActions.addView(
            subtle(getString(R.string.omni_action_copy), palette.accent) {
                val clipboard = getSystemService(ClipboardManager::class.java)
                clipboard?.setPrimaryClip(
                    ClipData.newPlainText(getString(R.string.omni_settings_logs), transcript)
                )
                results.removeAllViews()
                results.addView(notice(getString(R.string.omni_settings_logs_copied), palette.ok))
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginEnd = gap(1) },
        )
        logActions.addView(
            subtle(getString(R.string.omni_action_clear), palette.muted) {
                OmniLog.clearTranscript()
                render(false)
            },
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f).apply { marginStart = gap(1) },
        )
        content.addView(logActions)
        content.addView(quiet(getString(R.string.omni_settings_logs_note)))

        content.addView(heading(getString(R.string.omni_settings_about)))
        val about = card()
        about.addView(
            keyValue(
                getString(R.string.omni_app_name),
                getString(R.string.omni_about_developer),
                versionShown(),
                palette.accent,
            )
        )
        about.addView(
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
        content.addView(about)
        content.addView(body(getString(R.string.omni_about_how)))
        content.addView(quiet(getString(R.string.omni_about_watch)))
    }

    private fun versionShown(): String = runCatching {
        val about = packageManager.getPackageInfo(packageName, 0)
        val code = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            about.longVersionCode
        } else {
            @Suppress("DEPRECATION")
            about.versionCode.toLong()
        }
        "${'$'}{about.versionName} (${'$'}code)"
    }.getOrDefault("")

    private fun deviceLanguage(): String {
        val spoken = Resources.getSystem().configuration.locales
        for (index in 0 until spoken.size()) {
            val tag = spoken.get(index).language
            if (Preferences.LANGUAGES.any { it.first == tag }) {
                return tag
            }
        }
        return BASE_LOCALE
    }

    private fun spec() = ProjectSpec(
        packageName = formPackage.trim(),
        label = formLabel.trim(),
        versionName = formVersionName.trim(),
        versionCode = formVersionCode.trim().toIntOrNull() ?: 0,
        abis = ONLY_ABI,
        minSdk = formMinSdk,
        targetSdk = formTargetSdk,
        languages = formLanguages.toList(),
        locales = (formLocales + BASE_LOCALE).toList(),
    )

    private fun freeFolder(named: String): File {
        val parent = projectsFolder()
        val safe = named.trim()
            .map { if (it.isLetterOrDigit() || it == ' ' || it == '_' || it == '-') it else '_' }
            .joinToString("")
            .trim()
            .ifEmpty { "Project" }
        var candidate = File(parent, safe)
        var next = 2
        while (candidate.exists()) {
            candidate = File(parent, "$safe $next")
            next += 1
        }
        return candidate
    }

    private fun createProject() {
        results.removeAllViews()
        val root = freeFolder(formLabel)
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

        // The ceremony runs for exactly as long as the key takes to make.
        // Nothing here is on a timer: the particles keep circling until the
        // Core hands back a fingerprint, and the seal at the end is played
        // because a key exists and not because time has passed.
        val forge = KeyForgeView(this, palette).apply {
            caption = getString(R.string.omni_forge_title)
            detail = getString(R.string.omni_forge_bits, request.bits)
        }
        showCeremony(forge)
        forge.begin()

        Thread {
            val answer = runCatching {
                Builder.nativeCreateKey(folder, request.encode(), first)
            }
            runOnUiThread {
                first.fill(' ')
                keyPasswordView?.text?.clear()
                keyPasswordAgainView?.text?.clear()
                answer.fold({ document -> keyForged(forge, document) }) { error ->
                    OmniLog.recordCrash(Thread.currentThread(), error)
                    keyRefused(forge, error.message ?: error.javaClass.simpleName, null)
                }
            }
        }.start()
    }

    /** What the Core said about the key, played out on the ceremony. */
    private fun keyForged(forge: KeyForgeView, answer: String) {
        val root = runCatching { JSONObject(answer) }.getOrNull()
        if (root == null || !root.optBoolean("created", false)) {
            keyRefused(forge, getString(R.string.omni_forge_refused), root)
            return
        }
        val made = SigningKey.parse(root.getJSONObject("key"))
        OmniLog.event(LogLevel.INFO, "keystore", "Key ${made.alias} created, ${made.fingerprint}")
        Preferences.setSigningKey(this, made.path)
        forge.caption = getString(R.string.omni_forge_sealed)
        forge.seal(made.fingerprint)
        // The seal is an animation with an end, and the screen underneath is
        // only changed once it has reached it.
        waitFor(forge::sealedThrough) {
            hideCeremony { go(Screen.Keys) }
        }
    }

    private fun keyRefused(forge: KeyForgeView, said: String, root: JSONObject?) {
        forge.caption = said
        forge.detail = ""
        forge.refuse()
        ceremony.postDelayed({
            hideCeremony {
                results.removeAllViews()
                if (root != null) {
                    showRefusal(Refusal.parse(root), results)
                } else {
                    results.addView(notice(said, palette.error))
                }
            }
        }, REFUSAL_MILLIS)
    }

    /**
     * Runs `then` once `ready` is true, looking again every frame or so.
     *
     * A ceremony finishes when its own animation says it has, not when a
     * duration this code guessed has elapsed, so what is waited on here is the
     * view's own answer.
     */
    private fun waitFor(ready: () -> Boolean, then: () -> Unit) {
        val look = object : Runnable {
            override fun run() {
                if (ready()) then() else ceremony.postDelayed(this, LOOK_MILLIS)
            }
        }
        ceremony.postDelayed(look, LOOK_MILLIS)
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

        // What the Core measured on this device last time is handed back
        // before the build starts, which is what makes the estimate on screen
        // this phone's own rather than the one this project shipped with.
        val learned = Preferences.timings(this)
        Builder.nativeBuildExpect(learned.ifEmpty { null })

        val stage = BuildStageView(this, palette).apply {
            title = getString(R.string.omni_stage_title)
            stages = STAGE_NAMES.map { getString(it) }
            note = getString(
                if (learned.isEmpty()) R.string.omni_stage_measuring
                else R.string.omni_stage_learned
            )
        }
        showCeremony(stage)
        stage.begin()
        watchTheBuild(stage)

        Thread {
            val answer = runCatching {
                Builder.nativeBuildAll(
                    root,
                    apk.absolutePath,
                    aab.absolutePath,
                    keyPath,
                    if (password.isEmpty()) null else password,
                )
            }
            runOnUiThread {
                password.fill(' ')
                buildPasswordView?.text?.clear()
                answer.fold({ document ->
                    settle(stage) { buildEnded(document, apk, aab, started) }
                }) { error ->
                    OmniLog.recordCrash(Thread.currentThread(), error)
                    settle(stage) {
                        results.addView(
                            notice(error.message ?: error.javaClass.simpleName, palette.error)
                        )
                    }
                }
            }
        }.start()
    }

    /**
     * Reads where the build is, every frame, for as long as it is running.
     *
     * The percentage, the stage and the time left are all the Core's: this
     * asks it what they are and hands them to the view, which is why the
     * figure on screen cannot say something the build is not doing.
     */
    private fun watchTheBuild(view: BuildStageView) {
        val look = object : Runnable {
            override fun run() {
                if (!ceremonyIsUp()) return
                val report = runCatching { JSONObject(Builder.nativeBuildProgress()) }.getOrNull()
                if (report != null) {
                    val state = report.optString("state")
                    view.observe(
                        percent = report.optInt("percent"),
                        stage = report.optInt("step", 1) - 1,
                        count = report.optInt("steps", 1),
                        finished = state == "built",
                        refused = state == "refused",
                    )
                    view.remaining = when {
                        state == "built" -> getString(R.string.omni_stage_built)
                        state == "refused" -> getString(R.string.omni_refused)
                        else -> left(report.optLong("leftMillis") / 1000L)
                    }
                }
                view.postDelayed(this, WATCH_MILLIS)
            }
        }
        view.post(look)
    }

    /**
     * Lets the arc finish arriving at what it is showing, then hands over.
     *
     * A build that took two seconds should not have its ring snap from 60% to
     * gone: the ceremony is given long enough to run the number up to where
     * the Core left it, and only then does the screen underneath come back.
     */
    private fun settle(view: BuildStageView, then: () -> Unit) {
        view.postDelayed({
            view.rest()
            hideCeremony {
                results.removeAllViews()
                then()
            }
        }, SETTLE_MILLIS)
    }

    private fun buildEnded(answer: String, apk: File, aab: File, started: Long) {
        run finished@{
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

            // Only a build that got all the way through measured every stage,
            // so only that one teaches the next build anything.
            outcome.timings?.let { Preferences.setTimings(this, it) }

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
            // What the schemes are is read off the package, not written here:
            // a row saying "v2 + v3" whatever the file holds is a row that
            // would keep saying it once it stopped being true.
            val schemes = outcome.signatureSchemes.joinToString(" + ")
            val soundly = outcome.signed &&
                outcome.signatureKeyMatches &&
                outcome.signaturesVerified > 0
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_signature),
                    outcome.signedBy.orEmpty(),
                    schemes.ifEmpty { "?" },
                    if (soundly) palette.ok else palette.error,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_verified),
                    getString(
                        if (soundly) R.string.omni_result_verified_yes
                        else R.string.omni_result_verified_no
                    ),
                    outcome.signaturesVerified.toString(),
                    if (soundly) palette.ok else palette.error,
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
            outcome.path?.let { actionsFor(File(it), results) }
        }
    }

    /** The URI the installer and any chooser read the file through. */
    private fun handleFor(file: File): Pair<Uri, String> {
        val uri = PackageProvider.uriFor(this, file)
        val type = contentResolver.getType(uri) ?: PackageProvider.BUNDLE_TYPE
        return uri to type
    }

    private fun shareOutside(file: File) {
        if (!file.isFile) {
            return
        }
        val (uri, type) = handleFor(file)
        val sending = Intent(Intent.ACTION_SEND)
            .setType(type)
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        hand(
            Intent.createChooser(sending, file.name)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        )
    }

    /**
     * Hands the package to Android's installer. Android will not let any
     * application do this until the person has allowed installs from it, so
     * when that has not happened yet the button opens the screen where it is
     * allowed rather than doing nothing at all.
     */
    private fun installPackage(file: File) {
        if (!file.isFile) {
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            !packageManager.canRequestPackageInstalls()
        ) {
            results.removeAllViews()
            results.addView(notice(getString(R.string.omni_install_allow), palette.warning))
            hand(
                Intent(
                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:${'$'}packageName"),
                )
            )
            return
        }
        val (uri, type) = handleFor(file)
        hand(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, type)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    /** The install and share actions for one built file, stacked under its row. */
    private fun actionsFor(file: File, into: LinearLayout) {
        val installable = file.name.endsWith(".apk")
        if (installable) {
            into.addView(primary(getString(R.string.omni_action_install)) {
                installPackage(file)
            })
        }
        into.addView(subtle(getString(R.string.omni_action_share), palette.accent) {
            shareOutside(file)
        })
    }

    private fun hand(intent: Intent) {
        runCatching { startActivity(intent) }.onFailure { why ->
            OmniLog.event(LogLevel.WARN, "handoff", why.message ?: why.javaClass.simpleName)
            results.addView(notice(why.message ?: why.javaClass.simpleName, palette.error))
        }
    }

    private fun chooseImage(forProject: String?) {
        imageForProject = forProject
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "image/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("image/png", "image/jpeg", "image/webp", "image/heif", "image/*"),
            )
        }
        runCatching { startActivityForResult(intent, IMAGE_REQUEST) }
            .onFailure {
                results.removeAllViews()
                results.addView(
                    notice(
                        it.message ?: getString(R.string.omni_form_image_none),
                        palette.error,
                    )
                )
            }
    }

    /**
     * Reads whatever picture the person picked with Android's own decoders, so a
     * photograph is as welcome as a drawing, and writes it back out as a square
     * PNG no larger than the biggest launcher icon any screen asks for. A camera
     * photograph is tens of megabytes; what the project keeps is a few hundred
     * kilobytes of exactly the pixels an icon can use.
     */
    private fun stageImage(uri: Uri): File? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        runCatching {
            contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, bounds) }
        }.getOrNull()
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            return null
        }

        val options = BitmapFactory.Options()
        var step = 1
        while (
            bounds.outWidth / (step * 2) >= LARGEST_ICON_EDGE &&
            bounds.outHeight / (step * 2) >= LARGEST_ICON_EDGE
        ) {
            step *= 2
        }
        options.inSampleSize = step
        options.inPreferredConfig = Bitmap.Config.ARGB_8888

        val decoded = runCatching {
            contentResolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, options) }
        }.getOrNull() ?: return null

        val edge = minOf(decoded.width, decoded.height)
        val square = runCatching {
            Bitmap.createBitmap(
                decoded,
                (decoded.width - edge) / 2,
                (decoded.height - edge) / 2,
                edge,
                edge,
            )
        }.getOrDefault(decoded)
        val wanted = minOf(edge, LARGEST_ICON_EDGE)
        val scaled = if (square.width == wanted) {
            square
        } else {
            Bitmap.createScaledBitmap(square, wanted, wanted, true)
        }

        val staged = File(cacheDir, "chosen_image.png")
        val written = runCatching {
            FileOutputStream(staged).use { sink ->
                scaled.compress(Bitmap.CompressFormat.PNG, 100, sink)
            }
        }.getOrDefault(false)

        if (scaled !== square) scaled.recycle()
        if (square !== decoded) square.recycle()
        decoded.recycle()

        return if (written && staged.isFile) staged else null
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != IMAGE_REQUEST || resultCode != RESULT_OK) {
            return
        }
        val uri = data?.data ?: return
        val root = imageForProject
        imageForProject = null

        val staged = stageImage(uri)
        if (staged == null) {
            results.removeAllViews()
            results.addView(notice(getString(R.string.omni_form_image_unreadable), palette.error))
            return
        }
        if (root == null) {
            formImage = staged.absolutePath
            render(false)
            return
        }
        applyImage(root, staged.absolutePath)
    }

    private fun applyImage(root: String, source: String) {
        results.removeAllViews()
        working({ Builder.nativeSetIcon(root, source) }) finished@{ answer ->
            val stored = runCatching { JSONObject(answer) }.getOrNull()
            if (stored == null || !stored.optBoolean("stored", false)) {
                stored?.let { showRefusal(Refusal.parse(it), results) }
                return@finished
            }
            OmniLog.event(LogLevel.INFO, "project", "Image set for $root")
            render(false)
            results.addView(notice(getString(R.string.omni_form_image_set), palette.ok))
        }
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
        val holder = FlowLayout(this, gap(2), gap(2)).apply {
            setPadding(0, gap(1), 0, gap(1))
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
                maxLines = 1
                isClickable = true
                setOnClickListener {
                    onPick(index)
                    repaint()
                    animate().scaleX(0.94f).scaleY(0.94f).setDuration(70L).withEndAction {
                        animate().scaleX(1f).scaleY(1f).setDuration(110L).start()
                    }.start()
                }
            }
            views.add(chip)
            holder.addView(chip)
        }
        repaint()
        return holder
    }

    private fun View.setPadding(all: Int) = setPadding(all, all, all, all)
}
