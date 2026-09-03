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
import android.graphics.ColorFilter
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.PathMeasure
import android.graphics.RadialGradient
import android.graphics.RectF
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
import android.text.Spannable
import android.text.SpannableStringBuilder
import android.text.TextWatcher
import android.text.style.ForegroundColorSpan
import android.text.style.StyleSpan
import android.util.Log
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.accessibility.AccessibilityNodeInfo
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.view.inputmethod.EditorInfo
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
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

    fun refused(context: Context): Boolean = store(context).getBoolean(REFUSED, false)

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

    external fun nativeBuildProgress(): String

    external fun nativeBuildExpect(timings: String?)

    external fun nativeBuildStop()

    external fun nativeVerifySelf(packagePath: String, expectedCertificate: String?): String

    external fun nativeCreateKey(directory: String, spec: String, keyPassword: CharArray): String

    external fun nativeDefaultKey(directory: String): String

    external fun nativeBindDevice(secret: String): String

    external fun nativeListKeys(directory: String): String

    external fun nativeDeleteKey(path: String): String

    external fun nativeCheckKey(path: String, keyPassword: CharArray): String

    external fun nativeListProjects(directory: String): String

    external fun nativeProjectTree(root: String): String

    external fun nativeProjectHealth(root: String): String

    external fun nativeLayOut(name: String, text: String): String

    external fun nativeDependencies(root: String): String

    external fun nativeDependencyRemove(root: String, name: String): String

    external fun nativeManifestFacts(root: String): String

    external fun nativeManifestSet(root: String, field: String, value: String): String

    external fun nativeManifestPermission(root: String, name: String, wanted: Boolean): String

    external fun nativeSymbols(root: String, needle: String): String

    external fun nativeWhereWritten(root: String, qualified: String): String

    external fun nativeInspectPackage(path: String): String

    external fun nativeCheckProject(root: String): String

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

data class CodeCheck(
    val clear: Boolean,
    val classes: Int,
    val resources: Int,
    val locales: Int,
    val packageName: String,
    val refusal: Refusal?,
    val file: String,
    val line: Int,
    val column: Int,
) {
    companion object {
        fun parse(document: String): CodeCheck {
            val root = JSONObject(document)
            val code = root.optJSONObject("code") ?: JSONObject()
            val clear = code.optBoolean("clear", false)
            val place = code.optJSONObject("location")
            return CodeCheck(
                clear = clear,
                classes = code.optInt("classes"),
                resources = code.optInt("resources"),
                locales = code.optInt("locales"),
                packageName = code.optString("package"),
                refusal = if (clear) null else Refusal.parse(code),
                file = place?.optString("file").orEmpty(),
                line = place?.optInt("line") ?: 0,
                column = place?.optInt("column") ?: 0,
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

    val timings: String?,

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

        val EMBER = Palette(
            key = "ember",
            label = "Ember",
            background = 0xFF0B0705.toInt(),
            surface = 0xFF150E09.toInt(),
            raised = 0xFF1F1610.toInt(),
            foreground = 0xFFF5E3CE.toInt(),
            muted = 0xFF9A8064.toInt(),
            accent = 0xFFFFA53A.toInt(),
            ok = 0xFFB8D14A.toInt(),
            warning = 0xFFFFD166.toInt(),
            error = 0xFFF2593F.toInt(),
            divider = 0xFF2A1E14.toInt(),
            glowFirst = 0xFF6B3A0E.toInt(),
            glowSecond = 0xFF4A2A05.toInt(),
            glowThird = 0xFF33160A.toInt(),
        )

        val PHOSPHOR = Palette(
            key = "phosphor",
            label = "Phosphor",
            background = 0xFF060A07.toInt(),
            surface = 0xFF0C120D.toInt(),
            raised = 0xFF131B15.toInt(),
            foreground = 0xFFD8F2DC.toInt(),
            muted = 0xFF6B8570.toInt(),
            accent = 0xFF4CE86A.toInt(),
            ok = 0xFF8FFFA8.toInt(),
            warning = 0xFFE8D24C.toInt(),
            error = 0xFFFF6B5B.toInt(),
            divider = 0xFF17231A.toInt(),
            glowFirst = 0xFF0C4A1E.toInt(),
            glowSecond = 0xFF06381E.toInt(),
            glowThird = 0xFF1E3D12.toInt(),
        )

        val PAPER = Palette(
            key = "paper",
            label = "Paper",
            background = 0xFFF2EDE3.toInt(),
            surface = 0xFFFBF8F1.toInt(),
            raised = 0xFFE8E1D3.toInt(),
            foreground = 0xFF1A1613.toInt(),
            muted = 0xFF6B6156.toInt(),
            accent = 0xFFC4442E.toInt(),
            ok = 0xFF2E6B4F.toInt(),
            warning = 0xFFA5701A.toInt(),
            error = 0xFF97231A.toInt(),
            divider = 0xFFD8CFBE.toInt(),
            glowFirst = 0xFFE8C9B8.toInt(),
            glowSecond = 0xFFCFDDCB.toInt(),
            glowThird = 0xFFDCD2C0.toInt(),
        )

        val ALL: List<Palette> = listOf(FORGE, EMBER, PHOSPHOR, PAPER)

        fun of(key: String): Palette = ALL.firstOrNull { it.key == key } ?: FORGE
    }
}

object Type {
    val heading: Typeface = Typeface.create("sans-serif-condensed", Typeface.BOLD)
    val label: Typeface = Typeface.create("sans-serif-condensed", Typeface.BOLD)
    val body: Typeface = Typeface.create("sans-serif", Typeface.NORMAL)
    val strong: Typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    val data: Typeface = Typeface.MONOSPACE

    const val TRACKING = 0.14f
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

internal object Motion {

    const val TOUCH_UNITS = 12

    const val TAP = 70L
    const val REBOUND = 110L
    const val LEAVE = 140L
    const val ENTER = 260L
    const val SWEEP = 420L
    const val SETTLE = 620L
    const val REFUSAL = 1_400L

    private var scale = 1f

    fun readWhatTheDeviceAsksFor(context: Context) {
        scale = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f).coerceIn(0f, 4f)
    }

    fun still(): Boolean = scale <= 0.01f

    fun of(millis: Long): Long = if (still()) 0L else (millis * scale).toLong().coerceAtLeast(1L)
}

internal class Mark(private val shape: Shape, private val colour: Int) : Drawable() {

    enum class Shape { FOLDER, FILE, MORE, AWAY }

    private companion object {
        const val GRID = 24f
        const val STROKE = 1.7f
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val path = Path()

    override fun draw(canvas: Canvas) {
        val edge = min(bounds.width(), bounds.height()).toFloat()
        if (edge <= 0f) return
        val unit = edge / GRID
        canvas.save()
        canvas.translate(
            bounds.left + (bounds.width() - edge) / 2f,
            bounds.top + (bounds.height() - edge) / 2f,
        )
        canvas.scale(unit, unit)
        paint.color = colour
        paint.strokeWidth = STROKE
        path.rewind()
        when (shape) {
            Shape.FOLDER -> {
                paint.style = Paint.Style.STROKE
                path.moveTo(3.5f, 7f)
                path.lineTo(9.5f, 7f)
                path.lineTo(11.5f, 10f)
                path.lineTo(20.5f, 10f)
                path.lineTo(20.5f, 18.5f)
                path.lineTo(3.5f, 18.5f)
                path.close()
                canvas.drawPath(path, paint)
            }
            Shape.FILE -> {
                paint.style = Paint.Style.STROKE
                path.moveTo(6.5f, 3.5f)
                path.lineTo(14f, 3.5f)
                path.lineTo(17.5f, 7.5f)
                path.lineTo(17.5f, 20.5f)
                path.lineTo(6.5f, 20.5f)
                path.close()
                path.moveTo(14f, 3.5f)
                path.lineTo(14f, 7.5f)
                path.lineTo(17.5f, 7.5f)
                canvas.drawPath(path, paint)
            }
            Shape.MORE -> {
                paint.style = Paint.Style.FILL
                canvas.drawCircle(12f, 6f, 1.6f, paint)
                canvas.drawCircle(12f, 12f, 1.6f, paint)
                canvas.drawCircle(12f, 18f, 1.6f, paint)
            }
            Shape.AWAY -> {
                paint.style = Paint.Style.STROKE
                path.moveTo(10f, 7f)
                path.lineTo(15f, 12f)
                path.lineTo(10f, 17f)
                canvas.drawPath(path, paint)
            }
        }
        canvas.restore()
    }

    override fun setAlpha(alpha: Int) {
        paint.alpha = alpha
    }

    override fun setColorFilter(filter: ColorFilter?) {
        paint.colorFilter = filter
    }

    @Deprecated("The platform asks for this and nothing reads it.")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
}

class FlowLayout(
    context: Context,
    private val betweenX: Int,
    private val betweenY: Int,
) : ViewGroup(context) {

    private fun measureRows(width: Int, place: Boolean): Int {
        val mirrored = layoutDirection == LAYOUT_DIRECTION_RTL
        val limit = width - paddingStart - paddingEnd
        var x = 0
        var y = paddingTop
        var tallest = 0
        for (index in 0 until childCount) {
            val child = getChildAt(index)
            if (child.visibility == GONE) {
                continue
            }
            if (x > 0 && x + child.measuredWidth > limit) {
                x = 0
                y += tallest + betweenY
                tallest = 0
            }
            if (place) {
                val from = if (mirrored) {
                    width - paddingStart - x - child.measuredWidth
                } else {
                    paddingStart + x
                }
                child.layout(from, y, from + child.measuredWidth, y + child.measuredHeight)
            }
            x += child.measuredWidth + betweenX
            tallest = maxOf(tallest, child.measuredHeight)
        }
        return y + tallest + paddingBottom
    }

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        val width = MeasureSpec.getSize(widthSpec)
        val room = MeasureSpec.makeMeasureSpec(
            (width - paddingStart - paddingEnd).coerceAtLeast(0),
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

    data class Manifest(val root: String) : Screen {
        override val tab = Tab.FILES
    }

    data class Depends(val root: String) : Screen {
        override val tab = Tab.FILES
    }

    data class Health(val root: String) : Screen {
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

        const val LOOK_MILLIS = 60L

        const val WATCH_MILLIS = 110L

        const val TOUCH_UNITS = Motion.TOUCH_UNITS

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

        val ONLY_ABI = listOf("arm64-v8a")

        val ANDROID_RELEASES = listOf(
            30 to "11", 31 to "12", 32 to "12L", 33 to "13",
            34 to "14", 35 to "15", 36 to "16", 37 to "17",
        )

        val LANGUAGE_CHOICES = listOf(Triple("java", "Java", true))

        val COMMON_PERMISSIONS = listOf(
            "android.permission.INTERNET",
            "android.permission.ACCESS_NETWORK_STATE",
            "android.permission.CAMERA",
            "android.permission.RECORD_AUDIO",
            "android.permission.POST_NOTIFICATIONS",
            "android.permission.VIBRATE",
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.READ_MEDIA_IMAGES",
            "android.permission.WAKE_LOCK",
        )

        const val LARGEST_ICON_EDGE = 512
        const val PROJECT_RES = "Res"
        const val PROJECT_ICON = "Icon.png"

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
    private var navigating = false
    private var building = false
    private var standing = "UNKNOWN"
    private var openProject: String? = null

    private var formPackage = DEFAULT_PACKAGE
    private var formLabel = DEFAULT_LABEL
    private var formVersionName = "1.0.0"
    private var formVersionCode = "1"
    private var formMinSdk = 30
    private var formTargetSdk = 36
    private val formLanguages = linkedSetOf("java")
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

    private var editorAt: Pair<String, String>? = null

    private var editorOnDisk = ""

    private var editorLine = 0

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

        Motion.readWhatTheDeviceAsksFor(this)
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
        answerBackTheWayThisScreenWants()
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
            val keyboard = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                insets.getInsets(WindowInsets.Type.ime()).bottom
            } else {
                0
            }
            val above = roof.layoutParams as LinearLayout.LayoutParams
            if (above.height != top) {
                above.height = top
                roof.layoutParams = above
            }
            bar.visibility = if (keyboard > 0) View.GONE else View.VISIBLE
            bar.setPadding(gap(2), gap(2), gap(2), gap(2) + bottom)
            scroller.setPadding(0, 0, 0, keyboard)
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
                if (isFinishing || isDestroyed) {
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
                if (!isFinishing && !isDestroyed && found != standing) {
                    standing = found
                    render(false)
                }
            }
        }.start()
    }

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
        stopTheBuild()
        OmniLog.flushSession()
        super.onDestroy()
    }

    private fun examine(): String =
        if (Sentry.refused(this)) "TAMPERED" else Sentry.check(this)

    private fun go(next: Screen) {
        if (next == screen && !navigating) {
            return
        }
        screen = next
        answerBackTheWayThisScreenWants()
        if (navigating) {
            return
        }
        navigating = true
        content.animate()
            .alpha(0f)
            .translationY(-gap(RISE_DP / 6).toFloat())
            .setDuration(Motion.of(Motion.LEAVE))
            .setInterpolator(AccelerateInterpolator())
            .start()
        veil.sweep(palette.accent) {
            navigating = false
            if (isFinishing || isDestroyed) {
                return@sweep
            }
            render(true)
            scroller.scrollTo(0, 0)
        }
    }

    private fun showCeremony(view: View, stop: (() -> Unit)? = null) {
        ceremony.removeAllViews()
        ceremony.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        if (stop != null) {
            ceremony.addView(
                stopControl(stop),
                FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT).apply {
                    gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
                    bottomMargin = gap(8)
                },
            )
        }
        ceremony.alpha = 0f
        ceremony.visibility = View.VISIBLE
        ceremony.animate().alpha(1f).setDuration(Motion.of(Motion.ENTER)).start()
    }

    private fun hideCeremony(then: () -> Unit) {
        ceremony.animate()
            .alpha(0f)
            .setDuration(Motion.of(Motion.LEAVE))
            .withEndAction {
                ceremony.removeAllViews()
                ceremony.visibility = View.GONE
                then()
            }
            .start()
    }

    private fun ceremonyIsUp(): Boolean = ceremony.visibility == View.VISIBLE

    private fun stopControl(stop: () -> Unit) = Button(this).apply {
        text = getString(R.string.omni_stage_stop)
        isAllCaps = false
        stateListAnimator = null
        setTextColor(palette.foreground)
        typeface = Type.strong
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        letterSpacing = Type.TRACKING
        minimumWidth = gap(30)
        minimumHeight = gap(TOUCH_UNITS)
        setPadding(gap(6), gap(3), gap(6), gap(3))
        background = touchable(pill(palette.raised, gap(5).toFloat()), palette.error)
        setOnClickListener {
            isEnabled = false
            alpha = 0.5f
            stop()
        }
    }

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
            is Screen.Manifest -> renderManifest(here.root)
            is Screen.Depends -> renderDepends(here.root)
            is Screen.Health -> renderHealth(here.root)
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
                .setDuration(Motion.of(Motion.ENTER))
                .setInterpolator(DecelerateInterpolator())
                .start()
        } else {
            content.alpha = 1f
            content.translationY = 0f
        }
    }

    @Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
    override fun onBackPressed() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            super.onBackPressed()
            return
        }
        stepBack()
    }

    private var backHere: Any? = null
    private var backRegistered = false

    private fun answerBackTheWayThisScreenWants() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return
        }
        val wanted = screen !is Screen.Projects
        if (wanted == backRegistered) {
            return
        }
        val callback = backHere as? OnBackInvokedCallback
            ?: OnBackInvokedCallback { stepBack() }.also { backHere = it }
        if (wanted) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(
                OnBackInvokedDispatcher.PRIORITY_DEFAULT,
                callback,
            )
        } else {
            onBackInvokedDispatcher.unregisterOnBackInvokedCallback(callback)
        }
        backRegistered = wanted
    }

    private fun stepBack() {
        if (ceremonyIsUp()) {
            return
        }
        when (val here = screen) {
            is Screen.Projects -> finish()
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
            is Screen.Manifest -> go(Screen.Files(here.root, ""))
            is Screen.Depends -> go(Screen.Files(here.root, ""))
            is Screen.Health -> go(Screen.Files(here.root, ""))
            is Screen.Build -> go(Screen.Files(here.root, ""))
            is Screen.Keys -> go(Screen.Settings)
            is Screen.NewKey -> go(Screen.Keys)
            else -> go(Screen.Projects)
        }
    }

    private fun openPalette() {
        val root = openProject
        val actions = mutableListOf<Pair<String, () -> Unit>>()
        actions += getString(R.string.omni_tab_projects) to { go(Screen.Projects) }
        actions += getString(R.string.omni_projects_new) to { go(Screen.NewProject) }
        if (root != null) {
            actions += getString(R.string.omni_tab_files) to { go(Screen.Files(root, "")) }
            actions += getString(R.string.omni_search_title) to { go(Screen.Search(root)) }
            actions += getString(R.string.omni_check_title) to {
                go(Screen.Files(root, ""))
                checkProject(root)
            }
            actions += getString(R.string.omni_tab_build) to { go(Screen.Build(root)) }
            actions += getString(R.string.omni_action_new_file) to {
                go(Screen.Files(root, ""))
                askForName(getString(R.string.omni_action_new_file), "") { name ->
                    act(Builder.nativeWriteFile(root, name, ""), "saved")
                }
            }
        }
        actions += getString(R.string.omni_keys_title) to { go(Screen.Keys) }
        actions += getString(R.string.omni_keys_new) to { go(Screen.NewKey) }
        actions += getString(R.string.omni_tab_trash) to { go(Screen.Trash) }
        actions += getString(R.string.omni_tab_settings) to { go(Screen.Settings) }

        val labels = actions.map { it.first }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle(getString(R.string.omni_palette_title))
            .setItems(labels) { _, index -> actions[index].second() }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
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
                control().apply {
                    text = getString(labels.getValue(tab))
                    setTextColor(
                        when {
                            active -> palette.background
                            reachable -> palette.foreground
                            else -> palette.muted
                        }
                    )
                    typeface = Type.label
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                    letterSpacing = Type.TRACKING
                    maxLines = 1
                    setPadding(gap(1), gap(3), gap(1), gap(3))
                    background = touchable(
                        pill(if (active) palette.accent else Color.TRANSPARENT, gap(3).toFloat()),
                        palette.accent,
                    )
                    isSelected = active
                    isEnabled = true
                    readAs(android.widget.ToggleButton::class.java.name)
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

                    setOnLongClickListener {
                        openPalette()
                        true
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
                pictured(
                    File(project.root, "$PROJECT_RES/$PROJECT_ICON").absolutePath,
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
                        typeface = Type.strong
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
                typeface = Type.strong
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

        content.addView(subtle(getString(R.string.omni_manifest_title), palette.accent) {
            go(Screen.Manifest(root))
        })
        content.addView(subtle(getString(R.string.omni_depends_title), palette.accent) {
            go(Screen.Depends(root))
        })
        content.addView(subtle(getString(R.string.omni_health_title), palette.accent) {
            go(Screen.Health(root))
        })
        content.addView(subtle(getString(R.string.omni_check_title), palette.accent) {
            checkProject(root)
        })
        content.addView(subtle(getString(R.string.omni_search_title), palette.accent) {
            go(Screen.Search(root))
        })
        content.addView(primary(getString(R.string.omni_action_build)) { go(Screen.Build(root)) })
        content.addView(quiet(getString(R.string.omni_trash_note)))
    }

    private fun renderHealth(root: String) {
        content.addView(heading(getString(R.string.omni_health_title)))

        val answer = runCatching { JSONObject(Builder.nativeProjectHealth(root)) }.getOrNull()
        val health = answer?.optJSONObject("health")
        if (answer == null || !answer.optBoolean("read", false) || health == null) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
                go(Screen.Files(root, ""))
            })
            return
        }

        val settled = health.optBoolean("settled", false)
        content.addView(
            notice(
                getString(if (settled) R.string.omni_health_well else R.string.omni_health_not),
                if (settled) palette.ok else palette.warning,
            )
        )

        val sizes = card()
        listOf(
            Triple(R.string.omni_health_code, "codeFiles", "codeBytes"),
            Triple(R.string.omni_health_resources, "resourceFiles", "resourceBytes"),
            Triple(R.string.omni_health_assets, "assetFiles", "assetBytes"),
            Triple(R.string.omni_health_libraries, "libraryFiles", "libraryBytes"),
        ).forEach { (label, files, bytes) ->
            sizes.addView(
                keyValue(
                    getString(label),
                    getString(R.string.omni_health_files, health.optInt(files)),
                    size(health.optLong(bytes)),
                    palette.muted,
                )
            )
        }
        sizes.addView(rule(), MATCH_PARENT, 1)
        sizes.addView(
            keyValue(
                getString(R.string.omni_health_whole),
                getString(R.string.omni_health_files, health.optInt("files")),
                size(health.optLong("bytes")),
                palette.foreground,
            )
        )
        content.addView(sizes)

        if (!health.optBoolean("hasIcon", false)) {
            content.addView(
                row(
                    getString(R.string.omni_health_no_icon),
                    "",
                    getString(R.string.omni_action_change),
                    palette.warning,
                ) { chooseImage(root) }
            )
        }

        val uncompiled = health.optJSONArray("uncompiled")
        if (uncompiled != null && uncompiled.length() > 0) {
            content.addView(
                notice(getString(R.string.omni_health_uncompiled, uncompiled.length()), palette.error)
            )
            val listed = card()
            for (index in 0 until minOf(uncompiled.length(), 20)) {
                val one = uncompiled.optJSONObject(index) ?: continue
                listed.addView(
                    keyValue(one.optString("path"), "", one.optString("language"), palette.error)
                )
            }
            content.addView(listed)
        }

        val languages = health.optJSONArray("languages")
        if (languages != null && languages.length() > 0) {
            content.addView(heading(getString(R.string.omni_health_languages)))
            val listed = card()
            for (index in 0 until languages.length()) {
                val one = languages.optJSONObject(index) ?: continue
                val short = one.optInt("missingCount")
                val spare = one.optInt("extraCount")
                if (index > 0) {
                    listed.addView(rule(), MATCH_PARENT, 1)
                }
                listed.addView(
                    keyValue(
                        one.optString("folder"),
                        when {
                            short > 0 && spare > 0 ->
                                getString(R.string.omni_health_short_and_spare, short, spare)
                            short > 0 -> getString(R.string.omni_health_short, short)
                            spare > 0 -> getString(R.string.omni_health_spare, spare)
                            else -> ""
                        },
                        one.optInt("strings").toString(),
                        if (short > 0 || spare > 0) palette.warning else palette.ok,
                    )
                )
                for (key in listOf("missing", "extra")) {
                    val named = one.optJSONArray(key) ?: continue
                    if (named.length() == 0) continue
                    listed.addView(
                        quiet((0 until named.length()).joinToString(", ") { named.getString(it) })
                    )
                }
            }
            content.addView(listed)
            content.addView(quiet(getString(R.string.omni_health_language_note)))
        }

        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Files(root, ""))
        })
    }

    private fun renderDepends(root: String) {
        content.addView(heading(getString(R.string.omni_depends_title)))

        val answer = runCatching { JSONObject(Builder.nativeDependencies(root)) }.getOrNull()
        val held = answer?.optJSONObject("dependencies")
        if (answer == null || !answer.optBoolean("read", false) || held == null) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
                go(Screen.Files(root, ""))
            })
            return
        }

        val each = held.optJSONArray("each")
        if (each == null || each.length() == 0) {
            content.addView(notice(getString(R.string.omni_depends_none), palette.muted))
        } else {
            content.addView(
                notice(
                    getString(
                        R.string.omni_depends_summary,
                        each.length(),
                        held.optInt("classes"),
                        size(held.optLong("bytes")),
                    ),
                    palette.ok,
                )
            )
            for (index in 0 until each.length()) {
                val one = each.optJSONObject(index) ?: continue
                val card = card()
                card.addView(
                    keyValue(
                        one.optString("name"),
                        joined(one, "packages"),
                        size(one.optLong("bytes")),
                        palette.foreground,
                    )
                )
                val brings = listOfNotNull(
                    one.optInt("classes").takeIf { it > 0 }
                        ?.let { getString(R.string.omni_depends_classes, it) },
                    one.optInt("resources").takeIf { it > 0 }
                        ?.let { getString(R.string.omni_depends_resources, it) },
                    one.optInt("assets").takeIf { it > 0 }
                        ?.let { getString(R.string.omni_depends_assets, it) },
                    joined(one, "native").ifEmpty { null },
                    if (one.optBoolean("manifest", false)) {
                        getString(R.string.omni_manifest_title)
                    } else {
                        null
                    },
                )
                card.addView(quiet(brings.joinToString("  ·  ")))
                one.optString("note").ifEmpty { null }?.let {
                    card.addView(notice(it, palette.warning))
                }
                card.addView(
                    subtle(getString(R.string.omni_action_delete), palette.error) {
                        results.removeAllViews()
                        val done = runCatching {
                            JSONObject(Builder.nativeDependencyRemove(root, one.optString("name")))
                        }.getOrNull()
                        if (done != null && done.optBoolean("removed", false)) {
                            render(false)
                        } else {
                            done?.let { showRefusal(Refusal.parse(it), results) }
                        }
                    }
                )
                content.addView(card)
            }
        }

        val clashes = held.optJSONArray("clashes")
        if (clashes != null && clashes.length() > 0) {
            content.addView(heading(getString(R.string.omni_depends_clashes)))
            content.addView(
                notice(getString(R.string.omni_depends_clash_note), palette.error)
            )
            val listed = card()
            for (index in 0 until clashes.length()) {
                val one = clashes.optJSONObject(index) ?: continue
                listed.addView(
                    keyValue(
                        one.optString("class"),
                        one.optString("first") + "  +  " + one.optString("second"),
                        "",
                        palette.error,
                    )
                )
            }
            content.addView(listed)
        }

        content.addView(quiet(getString(R.string.omni_depends_note, held.optString("folder"))))
        content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
            go(Screen.Files(root, ""))
        })
    }

    private fun renderManifest(root: String) {
        content.addView(heading(getString(R.string.omni_manifest_title)))

        val answer = runCatching { JSONObject(Builder.nativeManifestFacts(root)) }.getOrNull()
        val facts = answer?.optJSONObject("manifest")
        if (answer == null || !answer.optBoolean("read", false) || facts == null) {
            answer?.let { showRefusal(Refusal.parse(it), content) }
            content.addView(subtle(getString(R.string.omni_action_back), palette.muted) {
                go(Screen.Files(root, ""))
            })
            return
        }

        fun change(field: String, value: String) {
            results.removeAllViews()
            val done = runCatching {
                JSONObject(Builder.nativeManifestSet(root, field, value))
            }.getOrNull()
            if (done != null && done.optBoolean("changed", false)) {
                render(false)
                results.addView(notice(getString(R.string.omni_manifest_saved), palette.ok))
            } else {
                done?.let { showRefusal(Refusal.parse(it), results) }
            }
        }

        val identity = card()
        identity.addView(
            keyValue(
                getString(R.string.omni_form_package),
                facts.optString("package"),
                "",
                palette.muted,
            )
        )
        identity.addView(
            row(
                getString(R.string.omni_form_label),
                facts.optString("label"),
                getString(R.string.omni_action_change),
                palette.accent,
            ) {
                askForName(getString(R.string.omni_form_label), facts.optString("label")) {
                    change("label", it)
                }
            }
        )
        identity.addView(
            row(
                getString(R.string.omni_form_version_name),
                facts.optString("versionName"),
                getString(R.string.omni_action_change),
                palette.accent,
            ) {
                askForName(
                    getString(R.string.omni_form_version_name),
                    facts.optString("versionName"),
                ) { change("versionName", it) }
            }
        )
        identity.addView(
            row(
                getString(R.string.omni_form_version_code),
                facts.optString("versionCode"),
                getString(R.string.omni_action_change),
                palette.accent,
            ) {
                askForName(
                    getString(R.string.omni_form_version_code),
                    facts.optString("versionCode"),
                ) { change("versionCode", it) }
            }
        )
        content.addView(identity)

        content.addView(label(getString(R.string.omni_form_min_sdk)))
        content.addView(
            chips(
                ANDROID_RELEASES.map { it.second },
                { index ->
                    ANDROID_RELEASES[index].first.toString() == facts.optString("minSdkVersion")
                },
            ) { index -> change("minSdkVersion", ANDROID_RELEASES[index].first.toString()) }
        )

        content.addView(label(getString(R.string.omni_form_target_sdk)))
        content.addView(
            chips(
                ANDROID_RELEASES.map { it.second },
                { index ->
                    ANDROID_RELEASES[index].first.toString() == facts.optString("targetSdkVersion")
                },
            ) { index -> change("targetSdkVersion", ANDROID_RELEASES[index].first.toString()) }
        )

        content.addView(heading(getString(R.string.omni_manifest_permissions)))
        val asked = facts.optJSONArray("permissions")
        val holder = card()
        if (asked == null || asked.length() == 0) {
            holder.addView(quiet(getString(R.string.omni_manifest_none)))
        } else {
            for (index in 0 until asked.length()) {
                val name = asked.getString(index)
                if (index > 0) {
                    holder.addView(rule(), MATCH_PARENT, 1)
                }
                holder.addView(
                    row(name, "", getString(R.string.omni_action_delete), palette.error) {
                        results.removeAllViews()
                        val done = runCatching {
                            JSONObject(Builder.nativeManifestPermission(root, name, false))
                        }.getOrNull()
                        if (done != null && done.optBoolean("changed", false)) {
                            render(false)
                        } else {
                            done?.let { showRefusal(Refusal.parse(it), results) }
                        }
                    }
                )
            }
        }
        content.addView(holder)

        content.addView(
            chips(COMMON_PERMISSIONS.map { it.substringAfterLast('.') }, { false }) { index ->
                results.removeAllViews()
                val done = runCatching {
                    JSONObject(
                        Builder.nativeManifestPermission(root, COMMON_PERMISSIONS[index], true)
                    )
                }.getOrNull()
                if (done != null && done.optBoolean("changed", false)) {
                    render(false)
                } else {
                    done?.let { showRefusal(Refusal.parse(it), results) }
                }
            }
        )
        content.addView(subtle(getString(R.string.omni_manifest_other), palette.accent) {
            askForName(getString(R.string.omni_manifest_other), "android.permission.") { name ->
                results.removeAllViews()
                val done = runCatching {
                    JSONObject(Builder.nativeManifestPermission(root, name, true))
                }.getOrNull()
                if (done != null && done.optBoolean("changed", false)) {
                    render(false)
                } else {
                    done?.let { showRefusal(Refusal.parse(it), results) }
                }
            }
        })

        content.addView(quiet(getString(R.string.omni_manifest_note)))
        content.addView(subtle(getString(R.string.omni_action_open), palette.muted) {
            go(Screen.Editor(root, "AndroidManifest.xml"))
        })
    }

    private fun inspectPackage(path: String) {
        working({ Builder.nativeInspectPackage(path) }) finished@{ answer ->
            val document = runCatching { JSONObject(answer) }.getOrNull() ?: return@finished
            if (!document.optBoolean("opened", false)) {
                showRefusal(Refusal.parse(document), results)
                return@finished
            }
            val found = document.optJSONObject("package") ?: return@finished

            val sound = found.optBoolean("sound", false)
            results.addView(
                notice(
                    getString(
                        if (sound) R.string.omni_open_sound else R.string.omni_open_unsound
                    ),
                    if (sound) palette.ok else palette.error,
                )
            )

            val facts = card()
            facts.addView(
                keyValue(
                    found.optString("package"),
                    found.optString("label"),
                    found.optString("versionName") + " (" +
                        found.optString("versionCode") + ")",
                    palette.foreground,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_open_platforms),
                    joined(found, "abis"),
                    "API " + found.optString("minSdk") + "–" + found.optString("targetSdk"),
                    palette.muted,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_open_code),
                    getString(
                        R.string.omni_open_code_detail,
                        found.optInt("classes"),
                        found.optInt("methods"),
                    ),
                    found.optInt("dexFiles").toString() + " dex",
                    palette.muted,
                )
            )
            facts.addView(
                keyValue(
                    getString(R.string.omni_result_signature),
                    joined(found, "signedBy"),
                    joined(found, "schemes").replace(", ", " + ").ifEmpty { "?" },
                    if (sound) palette.ok else palette.error,
                )
            )
            found.optJSONArray("fingerprints")?.let { array ->
                for (index in 0 until array.length()) {
                    facts.addView(quiet(array.getString(index)))
                }
            }
            facts.addView(
                keyValue(
                    getString(R.string.omni_open_entries),
                    "",
                    found.optInt("entries").toString(),
                    palette.muted,
                )
            )
            results.addView(facts)

            listOf(
                R.string.omni_open_permissions to "permissions",
                R.string.omni_open_activities to "activities",
            ).forEach { (label, key) ->
                val array = found.optJSONArray(key) ?: return@forEach
                if (array.length() == 0) return@forEach
                results.addView(heading(getString(label)))
                val listed = card()
                for (index in 0 until array.length()) {
                    listed.addView(quiet(array.getString(index)))
                }
                results.addView(listed)
            }

            found.optJSONArray("held")?.let { array ->
                if (array.length() == 0) return@let
                results.addView(heading(getString(R.string.omni_open_largest)))
                val listed = card()
                for (index in 0 until minOf(array.length(), 20)) {
                    val one = array.optJSONObject(index) ?: continue
                    listed.addView(
                        keyValue(
                            one.optString("name"),
                            "",
                            size(one.optLong("stored")),
                            palette.muted,
                        )
                    )
                }
                results.addView(listed)
            }

            found.optJSONArray("notes")?.let { array ->
                for (index in 0 until array.length()) {
                    results.addView(notice(array.getString(index), palette.warning))
                }
            }
        }
    }

    private fun joined(holder: JSONObject, key: String): String {
        val array = holder.optJSONArray(key) ?: return ""
        return (0 until array.length()).joinToString(", ") { array.getString(it) }
    }

    private fun checkProject(root: String) {
        working({ Builder.nativeCheckProject(root) }) finished@{ answer ->
            val checked = runCatching { CodeCheck.parse(answer) }.getOrElse {
                results.addView(notice(it.message ?: it.javaClass.simpleName, palette.error))
                return@finished
            }
            if (checked.clear) {
                results.addView(
                    notice(
                        getString(
                            R.string.omni_check_clear,
                            checked.classes,
                            checked.resources,
                            checked.locales,
                        ),
                        palette.ok,
                    )
                )
                showPolicy(answer)
                return@finished
            }
            checked.refusal?.let { showRefusal(it, results) }

            if (checked.file.isNotEmpty()) {
                results.addView(
                    row(
                        checked.file,
                        getString(R.string.omni_check_open),
                        if (checked.line > 0) {
                            checked.line.toString() + ":" + checked.column
                        } else {
                            ""
                        },
                        palette.error,
                    ) {
                        editorLine = checked.line
                        go(Screen.Editor(root, checked.file))
                    }
                )
            }
            showPolicy(answer)
        }
    }

    private fun showPolicy(answer: String) {
        val rules = runCatching {
            JSONObject(answer).optJSONObject("code")?.optJSONArray("rules")
        }.getOrNull() ?: return
        if (rules.length() == 0) {
            return
        }
        var broken = 0
        for (index in 0 until rules.length()) {
            if (rules.optJSONObject(index)?.optBoolean("held", true) == false) broken += 1
        }
        results.addView(heading(getString(R.string.omni_policy_title)))
        results.addView(
            notice(
                if (broken == 0) {
                    getString(R.string.omni_policy_all, rules.length())
                } else {
                    getString(R.string.omni_policy_some, broken, rules.length())
                },
                if (broken == 0) palette.ok else palette.error,
            )
        )
        val listed = card()
        for (index in 0 until rules.length()) {
            val one = rules.optJSONObject(index) ?: continue
            val held = one.optBoolean("held", true)
            if (index > 0) {
                listed.addView(rule(), MATCH_PARENT, 1)
            }
            listed.addView(
                keyValue(
                    one.optString("says"),
                    one.optString("rule"),
                    if (held) "OK" else one.optString("code"),
                    if (held) palette.ok else palette.error,
                )
            )
        }
        results.addView(listed)

        val findings = runCatching {
            JSONObject(answer).optJSONObject("code")
                ?.optJSONObject("policy")?.optJSONArray("findings")
        }.getOrNull() ?: return
        for (index in 0 until findings.length()) {
            val one = findings.optJSONObject(index) ?: continue
            val said = card()
            said.addView(notice(one.optString("what"), palette.error))
            said.addView(body(one.optString("why")))
            said.addView(quiet(one.optString("remedy")))
            results.addView(said)
        }
    }

    private fun renderSearch(root: String) {
        content.addView(heading(getString(R.string.omni_search_title)))

        val field = EditText(this).apply {
            setText(searchFor)
            hint = getString(R.string.omni_search_hint)
            setHintTextColor(palette.muted)
            setTextColor(palette.foreground)
            typeface = Type.data
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
                    editorLine = one.optInt("line")
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
            minimumHeight = gap(TOUCH_UNITS)
            isClickable = true
            isFocusable = true
            readAs(Button::class.java.name)
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
                    View(context).apply {
                        background = pill(palette.raised, gap(2).toFloat())
                        foreground = Mark(
                            if (entry.folder) Mark.Shape.FOLDER else Mark.Shape.FILE,
                            if (entry.folder) palette.accent else palette.muted,
                        )
                        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
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
                            typeface = Type.strong
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
                View(context).apply {
                    contentDescription = getString(R.string.omni_action_more)
                    isClickable = true
                    isFocusable = true
                    readAs(Button::class.java.name)
                    background = touchable(
                        pill(Color.TRANSPARENT, gap(5).toFloat()),
                        palette.accent,
                    )
                    foreground = Mark(Mark.Shape.MORE, palette.muted)
                    setOnClickListener { showActions(root, folder, entry) }
                    layoutParams = LinearLayout.LayoutParams(
                        gap(TOUCH_UNITS),
                        gap(TOUCH_UNITS),
                    )
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

        val draft = Drafts.read(this, root, path)?.takeIf { it != onDisk }
        editorText = draft ?: onDisk

        val editor = CodeEditor(this, palette).apply {
            setPadding(gap(9), gap(2), gap(3), gap(2))
            open(path.substringAfterLast('/'), editorText)
        }
        this.editor = editor
        this.editorAt = root to path
        if (editorLine > 0) {
            editor.showLine(editorLine)
            editorLine = 0
        }
        if (draft != null) {
            content.addView(notice(getString(R.string.omni_editor_restored), palette.warning))
        }

        var dirty = draft != null
        if (dirty) {
            where.text = getString(R.string.omni_editor_unsaved)
            where.setTextColor(palette.warning)
        }

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
        tools.addView(tool(getString(R.string.omni_editor_name), palette.accent) {
            offerNames(root, editor)
        })
        tools.addView(tool(getString(R.string.omni_editor_go), palette.accent) {
            goToDefinition(root, editor)
        })
        tools.addView(tool(getString(R.string.omni_editor_uses), palette.accent) {
            findUses(root, editor)
        })
        tools.addView(tool(getString(R.string.omni_editor_shape), palette.accent) {
            layOut(path.substringAfterLast('/'), editor)
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

    private fun offerNames(root: String, editor: CodeEditor) {
        val held = editor.text ?: return
        val caret = editor.selectionEnd.coerceIn(0, held.length)
        var from = caret
        while (from > 0 && isNamePart(held[from - 1])) from -= 1
        var to = caret
        while (to < held.length && isNamePart(held[to])) to += 1
        val word = held.subSequence(from, to).toString()
        if (word.isEmpty()) {
            results.removeAllViews()
            results.addView(notice(getString(R.string.omni_editor_name_none), palette.muted))
            return
        }

        working({ Builder.nativeSymbols(root, word) }) finished@{ answer ->
            val document = runCatching { JSONObject(answer) }.getOrNull() ?: return@finished
            val named = document.optJSONArray("named")
            if (named == null || named.length() == 0) {
                results.addView(
                    notice(getString(R.string.omni_editor_name_nothing, word), palette.muted)
                )
                return@finished
            }
            results.addView(heading(getString(R.string.omni_editor_name_found, word)))
            val listed = card()
            for (index in 0 until minOf(named.length(), 30)) {
                val one = named.optJSONObject(index) ?: continue
                if (index > 0) {
                    listed.addView(rule(), MATCH_PARENT, 1)
                }
                val project = one.optString("from") == "project"
                listed.addView(
                    row(
                        one.optString("simple"),
                        one.optString("package"),
                        if (project) getString(R.string.omni_editor_name_yours) else "",
                        if (project) palette.ok else palette.muted,
                    ) {
                        putName(editor, from, to, one)
                        results.removeAllViews()
                    }
                )
            }
            results.addView(listed)
        }
    }

    private fun isNamePart(c: Char): Boolean =
        c.isLetterOrDigit() || c == '_' || c == '$'

    private fun layOut(name: String, editor: CodeEditor) {
        results.removeAllViews()
        val held = editor.text ?: return
        val answer = runCatching {
            JSONObject(Builder.nativeLayOut(name, held.toString()))
        }.getOrNull()
        if (answer == null || !answer.optBoolean("laid", false)) {
            answer?.let { showRefusal(Refusal.parse(it), results) }
            return
        }
        if (!answer.optBoolean("changed", false)) {
            results.addView(notice(getString(R.string.omni_editor_shaped_none), palette.muted))
            return
        }
        val caret = editor.selectionStart
        held.replace(0, held.length, answer.optString("text"))
        editor.setSelection(caret.coerceIn(0, editor.text?.length ?: 0))
        results.addView(notice(getString(R.string.omni_editor_shaped), palette.ok))
    }

    private fun wordAt(editor: CodeEditor): String {
        val held = editor.text ?: return ""
        val caret = editor.selectionEnd.coerceIn(0, held.length)
        var from = caret
        while (from > 0 && isNamePart(held[from - 1])) from -= 1
        var to = caret
        while (to < held.length && isNamePart(held[to])) to += 1
        return held.subSequence(from, to).toString()
    }

    private fun goToDefinition(root: String, editor: CodeEditor) {
        val word = wordAt(editor)
        results.removeAllViews()
        if (word.isEmpty()) {
            results.addView(notice(getString(R.string.omni_editor_name_none), palette.muted))
            return
        }
        working({ Builder.nativeSymbols(root, word) }) finished@{ answer ->
            val named = runCatching { JSONObject(answer).optJSONArray("named") }.getOrNull()
            var mine: JSONObject? = null
            var anywhere: JSONObject? = null
            for (index in 0 until (named?.length() ?: 0)) {
                val one = named?.optJSONObject(index) ?: continue
                if (one.optString("simple") != word) continue
                if (anywhere == null) anywhere = one
                if (one.optString("from") == "project") {
                    mine = one
                    break
                }
            }
            val found = mine
            if (found == null) {
                results.addView(
                    notice(
                        getString(
                            if (anywhere == null) R.string.omni_editor_name_nothing
                            else R.string.omni_editor_go_platform,
                            anywhere?.optString("qualified") ?: word,
                        ),
                        palette.muted,
                    )
                )
                return@finished
            }
            val where = runCatching {
                JSONObject(Builder.nativeWhereWritten(root, found.optString("qualified")))
            }.getOrNull()
            if (where == null || !where.optBoolean("written", false)) {
                results.addView(
                    notice(
                        getString(R.string.omni_editor_go_platform, found.optString("qualified")),
                        palette.muted,
                    )
                )
                return@finished
            }
            editorLine = where.optInt("line")
            go(Screen.Editor(root, where.optString("path")))
        }
    }

    private fun findUses(root: String, editor: CodeEditor) {
        val word = wordAt(editor)
        results.removeAllViews()
        if (word.isEmpty()) {
            results.addView(notice(getString(R.string.omni_editor_name_none), palette.muted))
            return
        }
        working({ Builder.nativeSearchProject(root, word, true, true) }) finished@{ answer ->
            val document = runCatching { JSONObject(answer) }.getOrNull() ?: return@finished
            if (!document.optBoolean("searched", false)) {
                showRefusal(Refusal.parse(document), results)
                return@finished
            }
            searchFor = word
            searchCase = true
            searchWord = true
            showFound(root, document.optJSONObject("result"))
        }
    }

    private fun putName(editor: CodeEditor, from: Int, to: Int, chosen: JSONObject) {
        val held = editor.text ?: return
        val simple = chosen.optString("simple")
        val qualified = chosen.optString("qualified")
        val whole = held.toString()

        val already = whole.contains("import ${'$'}qualified;")
        val mine = whole.lineSequence()
            .firstOrNull { it.trimStart().startsWith("package ") }
            ?.trim()
            ?.removePrefix("package ")
            ?.removeSuffix(";")
            ?.trim()
            .orEmpty()
        val needsImport = !already &&
            qualified.contains('.') &&
            chosen.optString("package") != mine

        held.replace(from, to, simple)
        if (!needsImport) {
            editor.setSelection((from + simple.length).coerceIn(0, editor.text?.length ?: 0))
            return
        }

        val text = editor.text?.toString().orEmpty()
        var at = -1
        var line = 0
        var offset = 0
        for (one in text.lineSequence()) {
            val trimmed = one.trimStart()
            if (trimmed.startsWith("import ") || trimmed.startsWith("package ")) {
                at = offset + one.length + 1
            }
            offset += one.length + 1
            line += 1
            if (line > 200) break
        }
        val statement = "import ${'$'}qualified;\n"
        val put = at.coerceIn(0, editor.text?.length ?: 0)
        editor.text?.insert(put, statement)

        val moved = if (put <= from) statement.length else 0
        editor.setSelection(
            (from + simple.length + moved).coerceIn(0, editor.text?.length ?: 0)
        )
    }

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

    private fun tool(text: String, colour: Int, onPress: () -> Unit) = control().apply {
        this.text = text
        setTextColor(colour)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        maxLines = 1
        setPadding(gap(4), gap(2), gap(4), gap(2))
        background = touchable(pill(palette.raised, gap(4).toFloat()), colour)
        setOnClickListener {
            onPress()
            press()
        }
    }

    private fun View.press() {
        if (Motion.still()) {
            return
        }
        animate().scaleX(0.94f).scaleY(0.94f).setDuration(Motion.of(Motion.TAP)).withEndAction {
            animate().scaleX(1f).scaleY(1f).setDuration(Motion.of(Motion.REBOUND)).start()
        }.start()
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
            if (!one.bundle) {
                shelf.addView(subtle(getString(R.string.omni_open_title), palette.accent) {
                    inspectPackage(one.path)
                })
            }
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
                typeface = Type.data
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
            row(
                getString(R.string.omni_app_name),
                getString(R.string.omni_about_reach),
                getString(R.string.omni_about_soon_short),
                palette.accent,
            ) {
                results.removeAllViews()
                results.addView(notice(getString(R.string.omni_about_soon), palette.muted))
            }
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
    }

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
                if (isFinishing || isDestroyed) {
                    return@runOnUiThread
                }
                answer.fold({ document -> keyForged(forge, document) }) { error ->
                    OmniLog.recordCrash(Thread.currentThread(), error)
                    keyRefused(forge, error.message ?: error.javaClass.simpleName, null)
                }
            }
        }.start()
    }

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
        }, Motion.of(Motion.REFUSAL))
    }

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
        building = true
        showCeremony(stage) { stopTheBuild() }
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
                building = false
                if (isFinishing || isDestroyed) {
                    return@runOnUiThread
                }
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

    private fun stopTheBuild() {
        if (!building) {
            return
        }
        OmniLog.event(LogLevel.INFO, "build", "Stop asked for.")
        Builder.nativeBuildStop()
    }

    private fun watchTheBuild(view: BuildStageView) {
        var lastSaid = ""
        val look = object : Runnable {
            override fun run() {
                if (!ceremonyIsUp()) return
                val said = runCatching { Builder.nativeBuildProgress() }.getOrDefault("")
                if (said != lastSaid) {
                    lastSaid = said
                    val report = runCatching { JSONObject(said) }.getOrNull()
                    if (report != null) {
                        val state = report.optString("state")
                        view.observe(
                            percent = report.optInt("percent"),
                            stage = report.optInt("step", 1) - 1,
                            count = report.optInt("steps", 1),
                            finished = state == "built",
                            refused = state == "refused" || state == "stopped",
                        )
                        view.remaining = when (state) {
                            "built" -> getString(R.string.omni_stage_built)
                            "stopped" -> getString(R.string.omni_stage_stopped)
                            "refused" -> getString(R.string.omni_refused)
                            else -> left(report.optLong("leftMillis") / 1000L)
                        }
                    }
                }
                view.postDelayed(this, WATCH_MILLIS)
            }
        }
        view.post(look)
    }

    private fun settle(view: BuildStageView, then: () -> Unit) {
        view.postDelayed({
            view.rest()
            hideCeremony {
                results.removeAllViews()
                then()
            }
        }, Motion.of(Motion.SETTLE))
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

            if (outcome.findings.isNotEmpty()) {
                results.addView(
                    heading(getString(R.string.omni_build_said, outcome.findings.size))
                )
                val said = card()
                outcome.findings.forEachIndexed { index, one ->
                    if (index > 0) {
                        said.addView(rule(), MATCH_PARENT, 1)
                    }
                    said.addView(body(one))
                }
                results.addView(said)
            }

            outcome.path?.let { actionsFor(File(it), results) }
        }
    }

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
                if (isFinishing || isDestroyed) {
                    return@runOnUiThread
                }
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

    private fun View.readAs(className: String) {
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(
                host: View,
                info: AccessibilityNodeInfo,
            ) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.className = className
            }
        }
    }

    private fun said(vararg parts: String): String =
        parts.filter { it.isNotBlank() }.joinToString(", ")

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
        text = value.uppercase(Locale.getDefault())
        setTextColor(palette.foreground)
        typeface = Type.heading
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 21f)
        letterSpacing = 0.06f
        setPadding(0, gap(4), 0, gap(1))
    }

    private fun label(value: String) = TextView(this).apply {
        text = value.uppercase(Locale.getDefault())
        setTextColor(palette.muted)
        typeface = Type.label
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
        letterSpacing = Type.TRACKING
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
        typeface = Type.strong
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

    private fun control(): Button = Button(this).apply {
        isAllCaps = false
        stateListAnimator = null
        minWidth = 0
        minHeight = gap(TOUCH_UNITS)
        minimumWidth = 0
        minimumHeight = gap(TOUCH_UNITS)
        gravity = Gravity.CENTER
        isFocusable = true
        elevation = 0f
        typeface = Type.strong
    }

    private fun primary(text: String, onPress: () -> Unit) = control().apply {
        this.text = text
        setTextColor(palette.background)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        letterSpacing = 0.06f
        setPadding(gap(4), gap(4), gap(4), gap(4))
        background = touchable(pill(palette.accent, gap(3).toFloat()), palette.background)
        setOnClickListener { onPress() }
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(2)
            bottomMargin = gap(1)
        }
    }

    private fun subtle(text: String, colour: Int, onPress: () -> Unit) = control().apply {
        this.text = text
        setTextColor(colour)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        setPadding(gap(4), gap(3), gap(4), gap(3))
        background = touchable(pill(Color.TRANSPARENT, gap(3).toFloat()), colour)
        setOnClickListener { onPress() }
        layoutParams = LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply {
            topMargin = gap(1)
        }
    }

    private fun pictured(
        picture: String,
        title: String,
        detail: String,
        trailing: String,
        trailingColour: Int = palette.muted,
        onPress: () -> Unit,
    ) = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(0, gap(2), 0, gap(2))
        minimumHeight = gap(TOUCH_UNITS)
        background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
        isClickable = true
        isFocusable = true
        contentDescription = said(title, detail, trailing)
        isScreenReaderFocusable = true
        readAs(Button::class.java.name)
        setOnClickListener { onPress() }

        addView(
            thumbnail(picture, gap(11)),
            LinearLayout.LayoutParams(gap(11), gap(11)).apply { marginEnd = gap(3) },
        )
        addView(
            row(title, detail, trailing, trailingColour, onPress),
            LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f),
        )
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
        minimumHeight = gap(TOUCH_UNITS)
        background = touchable(pill(Color.TRANSPARENT, gap(2).toFloat()), palette.accent)
        isClickable = true
        isFocusable = true
        contentDescription = said(title, detail, trailing)
        isScreenReaderFocusable = true
        readAs(Button::class.java.name)
        setOnClickListener { onPress() }

        addView(
            LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                addView(
                    TextView(context).apply {
                        text = title
                        setTextColor(palette.foreground)
                        typeface = Type.strong
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
                    typeface = Type.label
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
                    letterSpacing = Type.TRACKING
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
        val views = mutableListOf<Button>()

        fun repaint() {
            views.forEachIndexed { index, view ->
                val on = selected(index)
                view.isSelected = on
                view.setTextColor(if (on) palette.background else palette.foreground)
                view.background = touchable(
                    pill(if (on) palette.accent else palette.raised, gap(4).toFloat()),
                    palette.accent,
                )
            }
        }

        labels.forEachIndexed { index, text ->
            val chip = control().apply {
                this.text = text
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                setPadding(gap(4), gap(2), gap(4), gap(2))
                maxLines = 1
                readAs(android.widget.ToggleButton::class.java.name)
                setOnClickListener {
                    onPick(index)
                    repaint()
                    press()
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

internal object Ink {

    const val FRAME_NANOS_60 = 16_666_667L

    fun ease(value: Float, from: Float, to: Float): Float {
        if (to <= from) return if (value >= to) 1f else 0f
        val held = ((value - from) / (to - from)).coerceIn(0f, 1f)
        return held * held * (3f - 2f * held)
    }

    fun mix(from: Float, to: Float, by: Float): Float = from + (to - from) * by

    fun fade(colour: Int, alpha: Float): Int = Color.argb(
        (Color.alpha(colour) * alpha.coerceIn(0f, 1f)).toInt(),
        Color.red(colour),
        Color.green(colour),
        Color.blue(colour),
    )

    fun hot(colour: Int, by: Float): Int {
        val held = by.coerceIn(0f, 1f)
        return Color.rgb(
            mix(Color.red(colour).toFloat(), 255f, held).toInt(),
            mix(Color.green(colour).toFloat(), 255f, held).toInt(),
            mix(Color.blue(colour).toFloat(), 255f, held).toInt(),
        )
    }

    fun tube(canvas: Canvas, paint: Paint, w: Float, h: Float, life: Float, strength: Float) {
        val spacing = max(3f, h / 300f)
        val drift = (life * 22f) % spacing
        paint.style = Paint.Style.FILL
        paint.color = fade(0xFF000000.toInt(), 0.26f * strength)
        paint.strokeWidth = spacing * 0.42f
        var y = -spacing + drift
        while (y < h) {
            canvas.drawLine(0f, y, w, y, paint)
            y += spacing
        }

        val band = ((life * 0.22f) % 1.4f) * h - h * 0.2f
        val tall = h * 0.10f
        paint.color = fade(0xFFFFFFFF.toInt(), 0.035f * strength)
        canvas.drawRect(0f, band, w, band + tall, paint)
    }

    fun vignette(canvas: Canvas, paint: Paint, w: Float, h: Float, strength: Float) {
        val radius = hypot(w, h) * 0.62f
        paint.shader = RadialGradient(
            w * 0.5f,
            h * 0.5f,
            radius,
            intArrayOf(0x00000000, fade(0xFF000000.toInt(), 0.55f * strength)),
            floatArrayOf(0.55f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
    }

    fun scatter(seed: Int, salt: Int): Float {
        var held = seed * 374_761_393 + salt * 668_265_263
        held = (held xor (held shr 13)) * 1_274_126_177
        return ((held xor (held shr 16)) and 0x00FF_FFFF) / 16_777_216f
    }
}

class BinaryVeil(context: Context) : View(context) {

    private companion object {
        const val COLUMNS = 26
        const val ROWS = 44
        const val SWEEP_MILLIS = 420f

        const val LATEST_MILLIS = 400L

        const val EDGE = 0.22f
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        textAlign = Paint.Align.CENTER
    }
    private val line = Paint()
    private var accent = Color.WHITE
    private var began = 0L
    private var running = false
    private var swapped = false
    private var whenDone: (() -> Unit)? = null
    private val glyph = CharArray(1)

    private val step = object : Runnable {
        override fun run() {
            if (!running) return
            invalidate()
            postOnAnimation(this)
        }
    }

    private val deadline = Runnable {
        running = false
        visibility = GONE
        swapNow()
    }

    fun sweep(colour: Int, swap: () -> Unit) {
        swapNow()
        accent = colour
        whenDone = swap
        began = SystemClock.uptimeMillis()
        running = true
        swapped = false
        visibility = VISIBLE
        alpha = 1f
        removeCallbacks(deadline)
        postDelayed(deadline, SWEEP_MILLIS.toLong() + LATEST_MILLIS)
        postOnAnimation(step)
    }

    private fun swapNow() {
        if (swapped) return
        swapped = true
        val work = whenDone
        whenDone = null
        work?.invoke()
    }

    override fun onDetachedFromWindow() {
        running = false
        removeCallbacks(step)
        removeCallbacks(deadline)
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        if (!running) return
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0f || h <= 0f) return

        val t = (SystemClock.uptimeMillis() - began) / SWEEP_MILLIS
        if (t >= 1f) {
            running = false
            visibility = GONE
            removeCallbacks(deadline)
            swapNow()
            return
        }
        if (t >= 0.5f) swapNow()

        val front = t * (1f + EDGE * 2f) - EDGE
        val columnWidth = w / COLUMNS
        val rowHeight = h / ROWS
        paint.textSize = min(columnWidth * 0.72f, rowHeight * 0.86f)

        val tracking = 1f - abs(t - 0.5f) * 2f
        val life = (SystemClock.uptimeMillis() - began) / 1000f

        for (row in 0 until ROWS) {
            val y = (row + 0.5f) / ROWS
            val distance = abs(y - front)
            if (distance > EDGE) continue
            val near = 1f - distance / EDGE
            val lit = near * near

            val slip = (Ink.scatter(row, (life * 12f).toInt()) - 0.5f) *
                columnWidth * 3.2f * tracking
            val bend = sin((y * 9f + life * 3.4f).toDouble()).toFloat() *
                columnWidth * 0.5f * tracking
            val shift = slip + bend

            val split = columnWidth * (0.10f + 0.30f * tracking) * (0.4f + lit)

            for (column in 0 until COLUMNS) {
                val roll = Ink.scatter(row * 131 + column, 7)
                if (roll > 0.55f + near * 0.4f) continue
                glyph[0] = if (Ink.scatter(row * 977 + column, 13) > 0.5f) '1' else '0'
                val x = (column + 0.5f) * columnWidth + shift
                val baseline = (row + 0.72f) * rowHeight

                paint.color = Ink.fade(0xFFFF3B30.toInt(), 0.22f + lit * 0.30f)
                canvas.drawText(glyph, 0, 1, x - split, baseline, paint)
                paint.color = Ink.fade(0xFF2BD6FF.toInt(), 0.22f + lit * 0.30f)
                canvas.drawText(glyph, 0, 1, x + split, baseline, paint)
                paint.color = Ink.fade(Ink.hot(accent, lit * 0.85f), 0.20f + lit * 0.80f)
                canvas.drawText(glyph, 0, 1, x, baseline, paint)
            }
        }

        drawScanlines(canvas, w, h, life)
        drawHeadSwitch(canvas, w, h, front, tracking)
    }

    private fun drawScanlines(canvas: Canvas, w: Float, h: Float, life: Float) {
        val spacing = max(3f, h / 260f)
        val drift = (life * 26f) % spacing
        line.color = Ink.fade(0xFF000000.toInt(), 0.30f)
        line.strokeWidth = spacing * 0.45f
        var y = -spacing + drift
        while (y < h) {
            canvas.drawLine(0f, y, w, y, line)
            y += spacing
        }
    }

    private fun drawHeadSwitch(canvas: Canvas, w: Float, h: Float, front: Float, tracking: Float) {
        if (tracking <= 0f) return
        val band = h * 0.018f
        val at = (front * h).coerceIn(-band, h + band)
        line.strokeWidth = 1f
        var y = at
        var index = 0
        while (y < at + band) {
            val torn = (Ink.scatter(index, (front * 400f).toInt()) - 0.5f) * w * 0.5f
            line.color = Ink.fade(Ink.hot(accent, 0.85f), 0.10f + 0.28f * tracking)
            canvas.drawLine(torn, y, w + torn, y, line)
            y += 1.5f
            index += 1
        }
        line.color = Ink.fade(Ink.hot(accent, 1f), 0.35f * tracking)
        line.strokeWidth = 1.4f
        canvas.drawLine(0f, at, w, at, line)
    }
}

class KeyForgeView(context: Context, private var palette: Palette) : View(context) {

    private companion object {
        const val DOTS = 520
        const val GLYPHS = 130
        const val LOOP_MILLIS = 4_200f
        const val SEAL_MILLIS = 1_150f

        const val GATHERED = 0.44f
    }

    private enum class Phase { FORGING, SEALING, SEALED, REFUSED }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        textAlign = Paint.Align.CENTER
    }
    private val glass = Paint()
    private val key = Path()
    private val trace = Path()
    private val measure = PathMeasure()
    private val bounds = RectF()
    private val position = FloatArray(2)
    private val glyph = CharArray(1)

    private val dotX = FloatArray(DOTS)
    private val dotY = FloatArray(DOTS)
    private val dotPoints = FloatArray(DOTS * 2)
    private val glyphTargetX = FloatArray(GLYPHS)
    private val glyphTargetY = FloatArray(GLYPHS)

    private var began = SystemClock.uptimeMillis()
    private var sealedAt = 0L
    private var phase = Phase.FORGING
    private var laidOut = 0
    private var running = false

    var caption: String = ""
    var detail: String = ""
    private var fingerprint: String = ""

    private val step = object : Runnable {
        override fun run() {
            if (!running) return
            invalidate()
            postOnAnimation(this)
        }
    }

    fun begin() {
        began = SystemClock.uptimeMillis()
        phase = Phase.FORGING
        running = true
        postOnAnimation(step)
    }

    fun seal(shown: String) {
        if (phase != Phase.FORGING) return
        fingerprint = shown
        sealedAt = SystemClock.uptimeMillis()
        phase = Phase.SEALING
    }

    fun refuse() {
        phase = Phase.REFUSED
        running = false
        removeCallbacks(step)
        invalidate()
    }

    fun sealedThrough(): Boolean =
        phase == Phase.SEALED ||
            (phase == Phase.SEALING &&
                SystemClock.uptimeMillis() - sealedAt >= SEAL_MILLIS)

    fun repaint(next: Palette) {
        palette = next
        invalidate()
    }

    override fun onDetachedFromWindow() {
        running = false
        removeCallbacks(step)
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        layOut(w.toFloat(), h.toFloat())
    }

    private fun layOut(w: Float, h: Float) {
        if (w <= 0f || h <= 0f) return
        buildKey(w, h)
        measure.setPath(key, false)
        var length = measure.length

        val lengths = ArrayList<Float>()
        do {
            lengths.add(measure.length)
        } while (measure.nextContour())
        length = lengths.sum()
        if (length <= 0f) return

        val everything = DOTS + GLYPHS
        for (index in 0 until everything) {

            val onOutline = index % 3 != 2
            var x: Float
            var y: Float
            if (onOutline) {
                val along = (Ink.scatter(index, 3) * length)
                var walked = 0f
                var contour = 0
                measure.setPath(key, false)
                while (contour < lengths.size - 1 && walked + lengths[contour] < along) {
                    walked += lengths[contour]
                    measure.nextContour()
                    contour++
                }
                measure.getPosTan(along - walked, position, null)
                x = position[0]
                y = position[1]
            } else {
                val turn = Ink.scatter(index, 5) * (PI * 2f).toFloat()
                val reach = bounds.width() * 0.5f * 0.62f * Ink.scatter(index, 9)
                x = bounds.centerX() + cos(turn) * reach
                y = bowCentreY + sin(turn) * reach
            }
            if (index < DOTS) {
                dotX[index] = x
                dotY[index] = y
            } else {
                glyphTargetX[index - DOTS] = x
                glyphTargetY[index - DOTS] = y
            }
        }
        laidOut = everything
    }

    private var bowCentreY = 0f
    private var bowRadius = 0f

    private fun buildKey(w: Float, h: Float) {
        key.reset()
        trace.reset()
        val span = min(w, h)
        val r = span * 0.155f
        val cx = w * 0.5f
        val cy = h * 0.34f
        bowCentreY = cy
        bowRadius = r
        bounds.set(cx - r, cy - r, cx + r, cy + r)

        key.addCircle(cx, cy, r, Path.Direction.CW)
        key.addCircle(cx, cy - r * 0.30f, r * 0.30f, Path.Direction.CCW)

        val half = span * 0.036f
        val top = cy + r * 0.92f
        val foot = h * 0.80f
        key.moveTo(cx - half, top)
        key.lineTo(cx - half, foot)

        val tooth = span * 0.052f
        key.lineTo(cx + half, foot)
        key.lineTo(cx + half, foot - tooth * 0.55f)
        key.lineTo(cx + half + tooth, foot - tooth * 0.55f)
        key.lineTo(cx + half + tooth, foot - tooth * 1.15f)
        key.lineTo(cx + half, foot - tooth * 1.15f)
        key.lineTo(cx + half, foot - tooth * 2.0f)
        key.lineTo(cx + half + tooth * 0.72f, foot - tooth * 2.0f)
        key.lineTo(cx + half + tooth * 0.72f, foot - tooth * 2.6f)
        key.lineTo(cx + half, foot - tooth * 2.6f)
        key.lineTo(cx + half, top)
        key.close()

        trace.moveTo(cx, foot - tooth * 3.4f)
        trace.lineTo(cx, cy + r * 0.55f)
        trace.moveTo(cx - r * 0.62f, cy + r * 0.10f)
        trace.lineTo(cx - r * 0.30f, cy + r * 0.10f)
        trace.lineTo(cx - r * 0.12f, cy + r * 0.38f)
        trace.moveTo(cx + r * 0.62f, cy - r * 0.02f)
        trace.lineTo(cx + r * 0.32f, cy - r * 0.02f)
        trace.lineTo(cx + r * 0.14f, cy + r * 0.28f)
        trace.moveTo(cx - r * 0.50f, cy - r * 0.52f)
        trace.lineTo(cx - r * 0.22f, cy - r * 0.52f)
        trace.moveTo(cx + r * 0.50f, cy + r * 0.56f)
        trace.lineTo(cx + r * 0.24f, cy + r * 0.56f)
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0f || h <= 0f) return
        if (laidOut == 0) layOut(w, h)

        val now = SystemClock.uptimeMillis()
        val life = (now - began) / 1000f

        val gather = Ink.ease(life, 0.30f, 2.10f)
        val reveal = Ink.ease(life, 1.05f, 2.35f)
        val sealing = if (phase == Phase.FORGING) 0f else
            ((now - sealedAt) / SEAL_MILLIS).coerceIn(0f, 1f)
        if (phase == Phase.SEALING && sealing >= 1f) phase = Phase.SEALED

        drawGround(canvas, w, h, life)
        drawParticles(canvas, w, h, life, gather, sealing)
        if (reveal > 0f) drawKey(canvas, life, reveal, sealing)
        drawWords(canvas, w, h, life, sealing)
        Ink.tube(canvas, glass, w, h, life, 1f)
        Ink.vignette(canvas, glass, w, h, 1f)
    }

    private fun drawGround(canvas: Canvas, w: Float, h: Float, life: Float) {
        canvas.drawColor(palette.background)
        paint.shader = RadialGradient(
            w * 0.5f, bowCentreY, max(w, h) * 0.75f,
            Ink.fade(palette.glowFirst, 0.55f),
            Ink.fade(palette.background, 0f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null

        paint.style = Paint.Style.STROKE
        paint.strokeWidth = 1f
        paint.color = Ink.fade(palette.accent, 0.07f)
        val spacing = h / 22f
        val drift = (life * 9f) % spacing
        var y = -spacing + drift
        while (y < h) {
            canvas.drawLine(0f, y, w, y, paint)
            y += spacing
        }
        var x = 0f
        val across = w / 9f
        while (x <= w) {
            canvas.drawLine(x, 0f, x, h, paint)
            x += across
        }
        paint.style = Paint.Style.FILL
    }

    private fun drawParticles(
        canvas: Canvas,
        w: Float,
        h: Float,
        life: Float,
        gather: Float,
        sealing: Float,
    ) {
        val cx = w * 0.5f
        val far = max(w, h) * 0.62f

        val thrown = 1f + sealing * sealing * 2.4f
        val faded = 1f - sealing

        var at = 0
        for (index in 0 until DOTS) {
            val turn = Ink.scatter(index, 1) * (PI * 2f).toFloat() +
                life * (0.35f + Ink.scatter(index, 2) * 0.9f)
            val reach = (far * (0.12f + Ink.scatter(index, 4) * 0.88f)) * (1f - gather * 0.78f)
            val fromX = cx + cos(turn) * reach
            val fromY = bowCentreY + sin(turn) * reach * 0.92f
            val toX = dotX[index]
            val toY = dotY[index]
            var x = Ink.mix(fromX, toX, gather)
            var y = Ink.mix(fromY, toY, gather)
            if (sealing > 0f) {
                x = cx + (x - cx) * thrown
                y = bowCentreY + (y - bowCentreY) * thrown
            }
            dotPoints[at++] = x
            dotPoints[at++] = y
        }
        paint.strokeCap = Paint.Cap.ROUND
        paint.strokeWidth = min(w, h) * 0.0055f
        paint.color = Ink.fade(Ink.hot(palette.accent, 0.25f), (0.30f + gather * 0.45f) * faded)
        canvas.drawPoints(dotPoints, 0, at, paint)
        paint.strokeCap = Paint.Cap.BUTT

        text.textSize = min(w, h) * 0.026f
        for (index in 0 until GLYPHS) {
            val seed = DOTS + index
            val turn = Ink.scatter(seed, 1) * (PI * 2f).toFloat() +
                life * (0.30f + Ink.scatter(seed, 2) * 0.8f)
            val reach = (far * (0.14f + Ink.scatter(seed, 4) * 0.86f)) * (1f - gather * 0.80f)
            var x = Ink.mix(cx + cos(turn) * reach, glyphTargetX[index], gather)
            var y = Ink.mix(bowCentreY + sin(turn) * reach * 0.92f, glyphTargetY[index], gather)
            if (sealing > 0f) {
                x = cx + (x - cx) * thrown
                y = bowCentreY + (y - bowCentreY) * thrown
            }
            glyph[0] = if (((life * 3f).toInt() + index) % 2 == 0) '1' else '0'
            val lit = 0.35f + 0.65f * Ink.scatter(seed, 6)
            text.color = Ink.fade(Ink.hot(palette.accent, gather * 0.5f), lit * faded)
            canvas.drawText(glyph, 0, 1, x, y, text)
        }
    }

    private fun split(canvas: Canvas, path: Path, width: Float, alpha: Float, by: Float) {
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = width
        paint.color = Ink.fade(0xFFFF3B30.toInt(), alpha * 0.55f)
        canvas.save()
        canvas.translate(-by, 0f)
        canvas.drawPath(path, paint)
        canvas.restore()
        paint.color = Ink.fade(0xFF2BD6FF.toInt(), alpha * 0.55f)
        canvas.save()
        canvas.translate(by, 0f)
        canvas.drawPath(path, paint)
        canvas.restore()
    }

    private fun drawKey(canvas: Canvas, life: Float, reveal: Float, sealing: Float) {
        val breath = 0.82f + 0.18f * sin(life * 2.1f)
        paint.style = Paint.Style.STROKE
        paint.strokeJoin = Paint.Join.ROUND

        paint.strokeWidth = bowRadius * 0.34f
        paint.color = Ink.fade(palette.accent, 0.13f * reveal * breath)
        canvas.drawPath(key, paint)

        paint.strokeWidth = bowRadius * 0.11f
        paint.color = Ink.fade(Ink.hot(palette.accent, 0.35f), 0.42f * reveal)
        canvas.drawPath(key, paint)

        split(canvas, key, max(1.4f, bowRadius * 0.028f), reveal, bowRadius * 0.030f)
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = max(1.4f, bowRadius * 0.028f)
        paint.color = Ink.fade(Ink.hot(palette.accent, 0.85f + sealing * 0.15f), reveal)
        canvas.drawPath(key, paint)

        val filled = Ink.ease(life, 1.7f, 3.0f)
        if (filled > 0f) {
            paint.strokeWidth = max(1f, bowRadius * 0.020f)
            paint.color = Ink.fade(Ink.hot(palette.accent, 0.55f), 0.75f * filled)
            canvas.drawPath(trace, paint)
        }

        val turning = Ink.ease(life, 1.9f, 2.8f)
        if (turning > 0f) {
            paint.strokeWidth = max(1f, bowRadius * 0.022f)
            paint.color = Ink.fade(palette.accent, 0.55f * turning)
            val inner = RectF(
                bounds.centerX() - bowRadius * 0.66f,
                bowCentreY - bowRadius * 0.66f,
                bounds.centerX() + bowRadius * 0.66f,
                bowCentreY + bowRadius * 0.66f,
            )
            val spin = (life * 62f) % 360f
            canvas.drawArc(inner, spin, 96f, false, paint)
            canvas.drawArc(inner, spin + 180f, 96f, false, paint)
            inner.inset(bowRadius * 0.14f, bowRadius * 0.14f)
            canvas.drawArc(inner, -spin * 1.4f, 62f, false, paint)
        }

        if (sealing > 0f) {
            val shut = Ink.ease(sealing, 0f, 0.55f)
            val body = bowRadius * 0.46f
            val cx = bounds.centerX()
            val cy = bowCentreY + bowRadius * 0.18f
            paint.strokeWidth = max(1.6f, bowRadius * 0.05f)
            paint.color = Ink.fade(Ink.hot(palette.ok, 0.4f), shut)
            val shackle = RectF(
                cx - body * 0.42f,
                cy - body * 1.02f,
                cx + body * 0.42f,
                cy - body * 0.18f,
            )
            canvas.drawArc(shackle, 180f, 180f * shut, false, paint)
            paint.style = Paint.Style.FILL
            canvas.drawRoundRect(
                cx - body * 0.62f, cy - body * 0.30f,
                cx + body * 0.62f, cy + body * 0.58f,
                body * 0.16f, body * 0.16f, paint,
            )
            paint.style = Paint.Style.STROKE

            val ring = Ink.ease(sealing, 0.10f, 1f)
            paint.strokeWidth = max(1f, bowRadius * 0.06f * (1f - ring))
            paint.color = Ink.fade(Ink.hot(palette.ok, 0.6f), (1f - ring) * 0.9f)
            canvas.drawCircle(cx, bowCentreY, bowRadius * (1f + ring * 3.2f), paint)
        }
        paint.style = Paint.Style.FILL
    }

    private fun drawWords(canvas: Canvas, w: Float, h: Float, life: Float, sealing: Float) {
        text.textSize = min(w, h) * 0.038f
        text.color = Ink.fade(palette.foreground, Ink.ease(life, 0.2f, 0.9f))
        canvas.drawText(caption, w * 0.5f, h * 0.90f, text)

        text.textSize = min(w, h) * 0.028f
        val said = if (sealing > 0f && fingerprint.isNotEmpty()) fingerprint else detail
        text.color = Ink.fade(
            if (sealing > 0f) palette.ok else palette.muted,
            Ink.ease(life, 0.5f, 1.2f),
        )
        canvas.drawText(said, w * 0.5f, h * 0.945f, text)
    }
}

class BuildStageView(context: Context, private var palette: Palette) : View(context) {

    private companion object {
        const val RAIN = 90

        const val CATCH_UP = 3.4f
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
    }
    private val glass = Paint()
    private val ring = RectF()
    private val glyph = CharArray(1)

    private var running = false
    private var lastFrame = 0L
    private var began = SystemClock.uptimeMillis()

    private var reported = 0f
    private var drawn = 0f
    private var stageAt = 0
    private var stageCount = 1
    private var refusedAt = -1
    private var done = false

    var title: String = ""
    var stages: List<String> = emptyList()
    var remaining: String = ""
    var note: String = ""

    private val step = object : Runnable {
        override fun run() {
            if (!running) return
            invalidate()
            postOnAnimation(this)
        }
    }

    fun begin() {
        began = SystemClock.uptimeMillis()
        lastFrame = began
        reported = 0f
        drawn = 0f
        stageAt = 0
        refusedAt = -1
        done = false
        running = true
        postOnAnimation(step)
    }

    fun observe(percent: Int, stage: Int, count: Int, finished: Boolean, refused: Boolean) {
        reported = percent.coerceIn(0, 100).toFloat()
        stageAt = stage.coerceAtLeast(0)
        stageCount = count.coerceAtLeast(1)
        done = finished
        if (refused) refusedAt = stageAt
    }

    fun rest() {
        running = false
        removeCallbacks(step)
    }

    fun repaint(next: Palette) {
        palette = next
        invalidate()
    }

    override fun onDetachedFromWindow() {
        rest()
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0f || h <= 0f) return

        val now = SystemClock.uptimeMillis()
        val since = ((now - lastFrame).coerceIn(0L, 100L)) / 1000f
        lastFrame = now
        drawn += (reported - drawn) * (since * CATCH_UP).coerceIn(0f, 1f)
        if (done && reported >= 100f && drawn > 99.4f) drawn = 100f
        val life = (now - began) / 1000f

        canvas.drawColor(palette.background)
        drawRain(canvas, w, h, life)
        val centre = h * 0.30f
        drawRing(canvas, w, centre, min(w, h) * 0.19f, life)
        drawStages(canvas, w, h)

        text.textAlign = Paint.Align.CENTER
        text.textSize = min(w, h) * 0.034f
        text.color = palette.foreground
        canvas.drawText(title, w * 0.5f, h * 0.075f, text)
        text.textSize = min(w, h) * 0.026f
        text.color = palette.muted
        canvas.drawText(remaining, w * 0.5f, h * 0.925f, text)
        canvas.drawText(note, w * 0.5f, h * 0.962f, text)

        Ink.tube(canvas, glass, w, h, life, 0.75f)
        Ink.vignette(canvas, glass, w, h, 0.8f)
    }

    private fun drawRain(canvas: Canvas, w: Float, h: Float, life: Float) {
        text.textAlign = Paint.Align.CENTER
        text.textSize = min(w, h) * 0.022f
        for (index in 0 until RAIN) {
            val column = Ink.scatter(index, 21) * w
            val speed = 26f + Ink.scatter(index, 22) * 70f
            val y = ((Ink.scatter(index, 23) * h + life * speed) % (h + 40f)) - 20f
            glyph[0] = if (((life * 2.4f).toInt() + index) % 2 == 0) '1' else '0'
            text.color = Ink.fade(palette.accent, 0.05f + 0.10f * Ink.scatter(index, 24))
            canvas.drawText(glyph, 0, 1, column, y, text)
        }
    }

    private fun drawRing(canvas: Canvas, w: Float, cy: Float, radius: Float, life: Float) {
        val cx = w * 0.5f
        ring.set(cx - radius, cy - radius, cx + radius, cy + radius)
        val width = radius * 0.13f

        paint.style = Paint.Style.STROKE
        paint.strokeWidth = width
        paint.strokeCap = Paint.Cap.ROUND
        paint.color = Ink.fade(palette.foreground, 0.10f)
        canvas.drawCircle(cx, cy, radius, paint)

        val colour = when {
            refusedAt >= 0 -> palette.error
            done -> palette.ok
            else -> palette.accent
        }
        val sweep = 360f * (drawn / 100f)
        paint.strokeWidth = width * 2.1f
        paint.color = Ink.fade(colour, 0.16f)
        canvas.drawArc(ring, -90f, sweep, false, paint)
        paint.strokeWidth = width
        paint.color = Ink.hot(colour, 0.2f)
        canvas.drawArc(ring, -90f, sweep, false, paint)

        if (!done && refusedAt < 0) {
            paint.strokeWidth = width * 0.55f
            paint.color = Ink.fade(Ink.hot(colour, 0.7f), 0.85f)
            canvas.drawArc(ring, -90f + (life * 220f) % 360f, 26f, false, paint)
        }
        paint.style = Paint.Style.FILL
        paint.strokeCap = Paint.Cap.BUTT

        text.textAlign = Paint.Align.CENTER
        text.textSize = radius * 0.62f
        text.color = palette.foreground
        canvas.drawText("${drawn.toInt()}", cx, cy + radius * 0.18f, text)
        text.textSize = radius * 0.22f
        text.color = palette.muted
        canvas.drawText("%", cx, cy + radius * 0.52f, text)
    }

    private fun drawStages(canvas: Canvas, w: Float, h: Float) {
        if (stages.isEmpty()) return
        val top = h * 0.545f
        val room = h * 0.33f
        val line = room / stages.size
        text.textAlign = Paint.Align.LEFT
        text.textSize = min(line * 0.52f, min(w, h) * 0.030f)
        val left = w * 0.16f

        for ((index, name) in stages.withIndex()) {
            val y = top + line * (index + 0.5f)
            val state = when {
                index == refusedAt -> 2
                done || index < stageAt -> 0
                index == stageAt -> 1
                else -> 3
            }
            val colour = when (state) {
                0 -> palette.ok
                1 -> palette.accent
                2 -> palette.error
                else -> palette.muted
            }
            paint.color = Ink.fade(colour, if (state == 3) 0.35f else 1f)
            val dot = line * 0.16f
            when (state) {
                1 -> {
                    paint.style = Paint.Style.STROKE
                    paint.strokeWidth = max(1.5f, dot * 0.45f)
                    canvas.drawCircle(left - dot * 2.4f, y - line * 0.16f, dot, paint)
                    paint.style = Paint.Style.FILL
                }
                else -> canvas.drawCircle(left - dot * 2.4f, y - line * 0.16f, dot, paint)
            }
            text.color = Ink.fade(
                if (state == 3) palette.muted else palette.foreground,
                if (state == 3) 0.55f else 1f,
            )
            canvas.drawText(name, left, y, text)
        }
    }
}

internal enum class Kind {
    COMMENT,
    TEXT,
    NUMBER,
    KEYWORD,
    TYPE,
    ANNOTATION,
    TAG,
    ATTRIBUTE,
}

internal class Token(val from: Int, val to: Int, val kind: Kind)

internal class Grammar(
    val keywords: Set<String>,
    val lineComment: String?,
    val blockComment: Pair<String, String>?,

    val characters: Boolean,

    val annotations: Boolean,

    val markup: Boolean,
) {
    companion object {

        private val JAVA = setOf(
            "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class",
            "const", "continue", "default", "do", "double", "else", "enum", "extends", "final",
            "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
            "interface", "long", "native", "new", "package", "private", "protected", "public",
            "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
            "throw", "throws", "transient", "try", "void", "volatile", "while",

            "exports", "module", "non-sealed", "open", "opens", "permits", "provides", "record",
            "requires", "sealed", "to", "transitive", "uses", "var", "when", "with", "yield",
            "true", "false", "null",
        )

        private val KOTLIN = setOf(
            "as", "break", "class", "continue", "do", "else", "false", "for", "fun", "if", "in",
            "interface", "is", "null", "object", "package", "return", "super", "this", "throw",
            "true", "try", "typealias", "typeof", "val", "var", "when", "while",
            "by", "catch", "constructor", "delegate", "dynamic", "field", "file", "finally",
            "get", "import", "init", "param", "property", "receiver", "set", "setparam", "value",
            "where", "actual", "abstract", "annotation", "companion", "const", "crossinline",
            "data", "enum", "expect", "external", "final", "infix", "inline", "inner", "internal",
            "lateinit", "noinline", "open", "operator", "out", "override", "private", "protected",
            "public", "reified", "sealed", "suspend", "tailrec", "vararg",
        )

        private val RUST = setOf(
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "union", "unsafe", "use", "where", "while",
        )

        private val CPP = setOf(
            "alignas", "alignof", "and", "asm", "auto", "bool", "break", "case", "catch", "char",
            "class", "const", "consteval", "constexpr", "constinit", "const_cast", "continue",
            "co_await", "co_return", "co_yield", "decltype", "default", "delete", "do", "double",
            "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "float",
            "for", "friend", "goto", "if", "inline", "int", "long", "mutable", "namespace", "new",
            "noexcept", "not", "nullptr", "operator", "or", "private", "protected", "public",
            "register", "reinterpret_cast", "requires", "return", "short", "signed", "sizeof",
            "static", "static_assert", "static_cast", "struct", "switch", "template", "this",
            "thread_local", "throw", "true", "try", "typedef", "typeid", "typename", "union",
            "unsigned", "using", "virtual", "void", "volatile", "wchar_t", "while", "xor",
        )

        val PLAIN = Grammar(emptySet(), null, null, false, false, false)

        fun of(name: String): Grammar {
            val lower = name.lowercase()
            return when {
                lower.endsWith(".java") ->
                    Grammar(JAVA, "//", "/*" to "*/", true, true, false)
                lower.endsWith(".kt") || lower.endsWith(".kts") ->
                    Grammar(KOTLIN, "//", "/*" to "*/", true, true, false)
                lower.endsWith(".rs") ->
                    Grammar(RUST, "//", "/*" to "*/", true, true, false)
                lower.endsWith(".c") || lower.endsWith(".h") ||
                    lower.endsWith(".cc") || lower.endsWith(".cpp") ||
                    lower.endsWith(".hpp") || lower.endsWith(".cxx") ->
                    Grammar(CPP, "//", "/*" to "*/", true, false, false)
                lower.endsWith(".xml") || lower.endsWith(".svg") || lower.endsWith(".html") ->
                    Grammar(emptySet(), null, "<!--" to "-->", false, false, true)
                lower.endsWith(".json") ->
                    Grammar(setOf("true", "false", "null"), null, null, false, false, false)
                lower.endsWith(".omni") || lower.endsWith(".toml") ||
                    lower.endsWith(".properties") || lower.endsWith(".gradle") ->
                    Grammar(emptySet(), "#", null, false, false, false)
                else -> PLAIN
            }
        }
    }
}

internal object Reader {

    const val LIMIT = 2_000_000

    fun read(text: CharSequence, grammar: Grammar): List<Token> {
        val out = ArrayList<Token>(min(text.length / 6 + 16, 200_000))
        if (grammar.markup) {
            readMarkup(text, grammar, out)
        } else {
            readCode(text, grammar, out)
        }
        return out
    }

    private fun readCode(text: CharSequence, grammar: Grammar, out: MutableList<Token>) {
        val end = min(text.length, LIMIT)
        val line = grammar.lineComment
        val block = grammar.blockComment
        var at = 0
        while (at < end) {
            val here = text[at]

            if (line != null && starts(text, at, line)) {
                var to = at
                while (to < end && text[to] != '\n') to++
                out.add(Token(at, to, Kind.COMMENT))
                at = to
                continue
            }

            if (block != null && starts(text, at, block.first)) {
                var to = at + block.first.length
                while (to < end && !starts(text, to, block.second)) to++
                to = min(end, to + block.second.length)
                out.add(Token(at, to, Kind.COMMENT))
                at = to
                continue
            }

            if (here == '"') {
                at = quoted(text, at, end, '"', out)
                continue
            }
            if (grammar.characters && here == '\'') {
                at = quoted(text, at, end, '\'', out)
                continue
            }

            if (grammar.annotations && here == '@' && at + 1 < end && isWordStart(text[at + 1])) {
                var to = at + 1
                while (to < end && isWord(text[to])) to++
                out.add(Token(at, to, Kind.ANNOTATION))
                at = to
                continue
            }

            if (here.isDigit()) {
                var to = at
                while (to < end && (text[to].isLetterOrDigit() || text[to] == '.' ||
                        text[to] == '_' || text[to] == 'x' || text[to] == 'X')
                ) {
                    to++
                }
                out.add(Token(at, to, Kind.NUMBER))
                at = to
                continue
            }

            if (isWordStart(here)) {
                var to = at
                while (to < end && isWord(text[to])) to++
                val word = text.subSequence(at, to).toString()
                when {
                    grammar.keywords.contains(word) -> out.add(Token(at, to, Kind.KEYWORD))

                    here.isUpperCase() -> out.add(Token(at, to, Kind.TYPE))
                }
                at = to
                continue
            }

            at++
        }
    }

    private fun quoted(
        text: CharSequence,
        from: Int,
        end: Int,
        quote: Char,
        out: MutableList<Token>,
    ): Int {
        var to = from + 1
        while (to < end) {
            val c = text[to]
            if (c == '\\') {
                to += 2
                continue
            }

            if (c == '\n') break
            to++
            if (c == quote) break
        }
        to = min(to, end)
        out.add(Token(from, to, Kind.TEXT))
        return max(to, from + 1)
    }

    private fun readMarkup(text: CharSequence, grammar: Grammar, out: MutableList<Token>) {
        val end = min(text.length, LIMIT)
        val block = grammar.blockComment
        var at = 0
        while (at < end) {
            if (block != null && starts(text, at, block.first)) {
                var to = at + block.first.length
                while (to < end && !starts(text, to, block.second)) to++
                to = min(end, to + block.second.length)
                out.add(Token(at, to, Kind.COMMENT))
                at = to
                continue
            }
            if (text[at] != '<') {
                at++
                continue
            }

            var to = at + 1
            if (to < end && (text[to] == '/' || text[to] == '?' || text[to] == '!')) to++
            val nameFrom = to
            while (to < end && (isWord(text[to]) || text[to] == ':' || text[to] == '-')) to++
            if (to > nameFrom) out.add(Token(at, to, Kind.TAG))

            while (to < end && text[to] != '>') {
                when {
                    text[to] == '"' || text[to] == '\'' -> {
                        to = quoted(text, to, end, text[to], out)
                    }
                    isWordStart(text[to]) -> {
                        val from = to
                        while (to < end && (isWord(text[to]) || text[to] == ':' ||
                                text[to] == '-' || text[to] == '.')
                        ) {
                            to++
                        }
                        out.add(Token(from, to, Kind.ATTRIBUTE))
                    }
                    else -> to++
                }
            }
            if (to < end) {
                out.add(Token(to, to + 1, Kind.TAG))
                to++
            }
            at = max(to, at + 1)
        }
    }

    private fun starts(text: CharSequence, at: Int, what: String): Boolean {
        if (at + what.length > text.length) return false
        for (index in what.indices) {
            if (text[at + index] != what[index]) return false
        }
        return true
    }

    private fun isWordStart(c: Char): Boolean = c.isLetter() || c == '_' || c == '$'

    private fun isWord(c: Char): Boolean = c.isLetterOrDigit() || c == '_' || c == '$'
}

internal class History(private val limit: Int = 400) {

    class Change(
        val at: Int,
        val removed: CharSequence,
        val added: CharSequence,
        val whenMade: Long,
    )

    private val done = ArrayList<Change>()
    private val undone = ArrayList<Change>()

    companion object {

        const val BREAK_MILLIS = 700L
    }

    fun canUndo(): Boolean = done.isNotEmpty()

    fun canRedo(): Boolean = undone.isNotEmpty()

    fun forget() {
        done.clear()
        undone.clear()
    }

    fun record(change: Change) {
        undone.clear()
        val last = done.lastOrNull()
        val merges = last != null &&
            last.removed.isEmpty() &&
            change.removed.isEmpty() &&
            change.added.length == 1 &&
            change.added[0] != '\n' &&
            change.at == last.at + last.added.length &&
            change.whenMade - last.whenMade < BREAK_MILLIS
        if (merges) {
            done[done.size - 1] = Change(
                last.at,
                last.removed,
                SpannableStringBuilder(last.added).append(change.added),
                change.whenMade,
            )
            return
        }
        done.add(change)
        while (done.size > limit) done.removeAt(0)
    }

    fun undo(into: Editable): Int? {
        val change = done.removeLastOrNull() ?: return null
        undone.add(change)
        into.replace(change.at, change.at + change.added.length, change.removed)
        return change.at + change.removed.length
    }

    fun redo(into: Editable): Int? {
        val change = undone.removeLastOrNull() ?: return null
        done.add(change)
        into.replace(change.at, change.at + change.removed.length, change.added)
        return change.at + change.added.length
    }
}

class CodeEditor(context: Context, private var palette: Palette) : EditText(context) {

    private companion object {

        const val MARGIN_LINES = 40

        const val REST_MILLIS = 140L
    }

    private val gutter = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        textAlign = Paint.Align.RIGHT
    }
    private val rule = Paint()

    private var grammar: Grammar = Grammar.PLAIN
    private var tokens: List<Token> = emptyList()
    private var painted = -1 to -1
    private var reading = false
    private var generation = 0

    private var replaying = false

    private val history = History()

    fun canUndo(): Boolean = history.canUndo()

    fun canRedo(): Boolean = history.canRedo()

    var onChanged: (() -> Unit)? = null

    private val readAgain = Runnable { read() }

    init {
        setTextColor(palette.foreground)
        setBackgroundColor(Color.TRANSPARENT)
        typeface = Typeface.MONOSPACE
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
        gravity = Gravity.TOP or Gravity.START
        setHorizontallyScrolling(true)
        isHorizontalScrollBarEnabled = true
        includeFontPadding = false
        setLineSpacing(0f, 1.15f)
        rule.strokeWidth = 1f

        addTextChangedListener(object : TextWatcher {
            private var removed: CharSequence = ""

            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {
                removed = s?.subSequence(start, start + count)?.toString().orEmpty()
            }

            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                if (replaying || s == null) return
                history.record(
                    History.Change(
                        start,
                        removed,
                        s.subSequence(start, start + count).toString(),
                        android.os.SystemClock.uptimeMillis(),
                    )
                )
            }

            override fun afterTextChanged(s: Editable?) {
                onChanged?.invoke()
                removeCallbacks(readAgain)
                postDelayed(readAgain, REST_MILLIS)
            }
        })
    }

    fun open(name: String, text: String) {
        grammar = Grammar.of(name)
        replaying = true
        setText(text)
        replaying = false
        history.forget()
        setSelection(0)
        read()
    }

    fun repaint(next: Palette) {
        palette = next
        setTextColor(palette.foreground)
        painted = -1 to -1
        colour()
        invalidate()
    }

    fun showLine(line: Int) {
        post {
            val held = text ?: return@post
            var at = 0
            var seen = 1
            while (seen < line && at < held.length) {
                if (held[at] == '\n') seen += 1
                at += 1
            }
            setSelection(at.coerceIn(0, held.length))
            val layout = layout ?: return@post
            val row = layout.getLineForOffset(at)

            scrollTo(0, (layout.getLineTop(row) - height / 3).coerceAtLeast(0))
            invalidate()
        }
    }

    fun undo() = move { history.undo(it) }

    fun redo() = move { history.redo(it) }

    private fun move(what: (Editable) -> Int?) {
        val editable = text ?: return
        replaying = true
        val caret = what(editable)
        replaying = false
        if (caret != null) {
            setSelection(caret.coerceIn(0, editable.length))
            onChanged?.invoke()
            read()
        }
    }

    private fun read() {
        if (grammar === Grammar.PLAIN) {
            tokens = emptyList()
            return
        }
        if (reading) return
        reading = true
        generation += 1
        val mine = generation
        val snapshot = text?.toString().orEmpty()
        Thread {
            val found = runCatching { Reader.read(snapshot, grammar) }.getOrDefault(emptyList())
            post {
                reading = false
                if (mine == generation) {
                    tokens = found
                    painted = -1 to -1
                    colour()
                    invalidate()
                }
            }
        }.start()
    }

    override fun onScrollChanged(l: Int, t: Int, oldl: Int, oldt: Int) {
        super.onScrollChanged(l, t, oldl, oldt)
        colour()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        colour()
    }

    private fun colour() {
        val layout = layout ?: return
        val editable = text ?: return
        if (tokens.isEmpty()) return

        val first = layout.getLineForVertical(scrollY)
        val last = layout.getLineForVertical(scrollY + height)
        val from = layout.getLineStart(max(0, first - MARGIN_LINES))
        val to = layout.getLineEnd(min(layout.lineCount - 1, last + MARGIN_LINES))
        if (from >= painted.first && to <= painted.second) return
        painted = from to to

        for (span in editable.getSpans(0, editable.length, ForegroundColorSpan::class.java)) {
            editable.removeSpan(span)
        }
        for (span in editable.getSpans(0, editable.length, StyleSpan::class.java)) {
            editable.removeSpan(span)
        }

        for (token in tokens) {
            if (token.to <= from) continue
            if (token.from >= to) break
            val start = max(token.from, 0)
            val end = min(token.to, editable.length)
            if (start >= end) continue
            editable.setSpan(
                ForegroundColorSpan(colourOf(token.kind)),
                start,
                end,
                Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
            if (token.kind == Kind.KEYWORD || token.kind == Kind.TAG) {
                editable.setSpan(
                    StyleSpan(Typeface.BOLD),
                    start,
                    end,
                    Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
                )
            }
        }
    }

    private fun colourOf(kind: Kind): Int = when (kind) {
        Kind.COMMENT -> Ink.fade(palette.muted, 0.85f)
        Kind.TEXT -> palette.ok
        Kind.NUMBER -> palette.warning
        Kind.KEYWORD -> palette.accent
        Kind.TYPE -> Ink.hot(palette.accent, 0.45f)
        Kind.ANNOTATION -> palette.glowThird.let { Ink.hot(it, 0.55f) }
        Kind.TAG -> palette.accent
        Kind.ATTRIBUTE -> Ink.hot(palette.warning, 0.25f)
    }

    override fun onDraw(canvas: Canvas) {
        val layout = layout
        if (layout != null && paddingLeft > 0) {
            gutter.textSize = textSize * 0.78f
            val caretLine = layout.getLineForOffset(selectionStart.coerceIn(0, text?.length ?: 0))
            val first = layout.getLineForVertical(scrollY)
            val last = layout.getLineForVertical(scrollY + height)
            val edge = (paddingLeft - gutter.textSize * 0.9f) + scrollX

            rule.color = Ink.fade(palette.accent, 0.06f)
            canvas.drawRect(
                scrollX.toFloat(),
                layout.getLineTop(caretLine).toFloat(),
                (scrollX + width).toFloat(),
                layout.getLineBottom(caretLine).toFloat(),
                rule,
            )

            for (line in first..min(last, layout.lineCount - 1)) {
                gutter.color = if (line == caretLine) {
                    palette.accent
                } else {
                    Ink.fade(palette.muted, 0.55f)
                }
                canvas.drawText(
                    (line + 1).toString(),
                    edge,
                    layout.getLineBaseline(line).toFloat(),
                    gutter,
                )
            }

            rule.color = Ink.fade(palette.divider, 0.9f)
            val at = (paddingLeft - gutter.textSize * 0.35f) + scrollX
            canvas.drawLine(
                at,
                scrollY.toFloat(),
                at,
                (scrollY + height).toFloat(),
                rule,
            )
        }
        super.onDraw(canvas)
    }

    override fun onSelectionChanged(start: Int, end: Int) {
        super.onSelectionChanged(start, end)
        invalidate()
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(readAgain)
        super.onDetachedFromWindow()
    }
}

internal object Drafts {

    private const val FOLDER = "Drafts"

    const val REST_MILLIS = 1_500L

    private fun folder(context: Context): File =
        File(context.filesDir, FOLDER).also { it.mkdirs() }

    private fun named(root: String, path: String): String {
        var held = 0xcbf2_9ce4_8422_2325uL
        for (character in "$root $path") {
            held = held xor character.code.toULong()
            held *= 0x100_0000_01b3uL
        }
        val stem = path.substringAfterLast('/').filter { it.isLetterOrDigit() || it == '.' }
        return "${stem.takeLast(40)}.${held.toString(16)}.draft"
    }

    fun read(context: Context, root: String, path: String): String? {
        val file = File(folder(context), named(root, path))
        if (!file.isFile) return null
        return runCatching { file.readText() }.getOrNull()
    }

    fun write(context: Context, root: String, path: String, text: String) {
        runCatching { File(folder(context), named(root, path)).writeText(text) }
    }

    fun forget(context: Context, root: String, path: String) {
        runCatching { File(folder(context), named(root, path)).delete() }
    }
}
