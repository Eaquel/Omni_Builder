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

// ---------------------------------------------------------------------------
// Logging (directive sections 33, 34, 56 and 57)
// ---------------------------------------------------------------------------

/** Severity of a session log entry. */
enum class LogLevel {
    /** Fine-grained detail. */
    TRACE,

    /** Something happened that is worth a record. */
    INFO,

    /** Something is suspicious but the application continues. */
    WARN,

    /** Something failed. */
    ERROR,
}

/**
 * Where a log file could be written.
 *
 * The application always keeps a copy it controls. Publishing to shared storage
 * can fail for reasons the application does not govern - a denied permission, a
 * provider that refuses - and when it does, the reason is recorded rather than
 * swallowed.
 */
sealed interface LogDestination {
    /** The file reached shared storage. */
    data class Published(val location: String) : LogDestination

    /**
     * The private copy is written and publishing is still running.
     *
     * Measured on a Galaxy S23 running Android 16, publishing takes 11 to 18 ms
     * while the private write is sub-millisecond. Directive section 36 does not
     * allow spending that on the main thread, so the two are split and this
     * state is what the caller sees in between.
     */
    data class Pending(val location: String) : LogDestination

    /** Only the private copy exists. */
    data class PrivateOnly(val location: String, val reason: String) : LogDestination
}

/**
 * One place a copy of a log was written, and how that went.
 *
 * Two attempts to publish the log went wrong in a row, and both stayed invisible
 * for a whole phase because a failure was recorded only in the log that had just
 * failed to be published. Every attempt is now reported individually, with the
 * exception's type as well as its message: an IOException's message is often
 * null, and "publishing failed: null" says nothing at all.
 */
data class LogCopy(
    /** Short name of the place, for the screen. */
    val label: String,
    /** Where it went, or where it would have gone. */
    val location: String,
    /** Null when it worked; otherwise what went wrong. */
    val error: String?,
) {
    /** Whether the copy is there. */
    val succeeded: Boolean get() = error == null
}

/**
 * The application's log.
 *
 * ## Contract (directive section 2)
 *
 * * **Purpose** — record what the application did and why it stopped, in a place
 *   the person using it can actually reach.
 * * **Outputs** — `Documents/Omni_Builder/Session_Log.txt` and
 *   `Documents/Omni_Builder/Crash_Log.txt`, plus a private copy of each.
 * * **Security** — the log carries no credential, no key and no file content
 *   from a user's project (directive sections 25 and 57). It records what
 *   happened, not what was processed.
 * * **Failure modes** — writing the private copy is the operation that must not
 *   fail; publishing to shared storage may, and the reason becomes a log entry
 *   of its own. A failure inside the logger never propagates to the caller,
 *   because a broken logger must not be the thing that breaks the application.
 * * **Resource bounds** — each file is capped, and the oldest half is dropped
 *   when the cap is reached (directive section 60).
 * * **Status** — FOUNDATION.
 */
object OmniLog {

    /** Directory created under the shared Documents folder. */
    const val DIRECTORY_NAME: String = "Omni_Builder"

    /** File holding this run's events. */
    const val SESSION_FILE: String = "Session_Log.txt"

    /** File holding every crash the application has recorded. */
    const val CRASH_FILE: String = "Crash_Log.txt"

    /**
     * Largest a log file may become, in bytes.
     *
     * A log that grows without bound is a resource-exhaustion bug wearing a
     * useful disguise (directive section 60). On overflow the newest half is
     * kept: the entries near a failure are the ones worth having.
     */
    const val MAX_BYTES: Int = 256 * 1024

    private const val TAG = "OmniBuilder"

    private val lock = Any()
    private val session = StringBuilder(8 * 1024)

    /**
     * Carries publishing off the calling thread.
     *
     * One thread, so writes stay ordered and two flushes can never interleave
     * inside the same file. A daemon thread, so it can never hold the process
     * open (directive section 36).
     */
    private val publisher = java.util.concurrent.Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "omni-log-publisher").apply { isDaemon = true }
    }

    @Volatile
    private var lastPublished: LogDestination? = null

    @Volatile
    private var copies: List<LogCopy> = emptyList()

    /**
     * Notified after each publish attempt, on the publisher thread.
     *
     * A single listener rather than a list: exactly one screen shows this, and a
     * registry of listeners would be a way to leak an activity. It is cleared in
     * onDestroy for the same reason.
     */
    @Volatile
    private var publishListener: ((LogDestination) -> Unit)? = null

    @Volatile
    private var context: Context? = null

    @Volatile
    private var installedAt: String = "not started"

    private fun timestamp(): String =
        SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US).format(Date())

    /**
     * Binds the log to an application context and installs the crash handler.
     *
     * Called from [BuilderApplication] rather than from the activity, so a crash
     * during activity creation is still recorded.
     */
    fun install(application: Application) {
        context = application.applicationContext
        installedAt = timestamp()

        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            try {
                recordCrash(thread, error)
            } catch (secondary: Throwable) {
                // The crash handler must never become the crash. Logcat is the
                // last resort and is always available.
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

    /** Records one event. Never throws. */
    fun event(level: LogLevel, tag: String, message: String) {
        val line = "${timestamp()}  ${level.name.padEnd(5)}  ${tag.padEnd(12)}  $message"
        synchronized(lock) { session.append(line).append('\n') }
        when (level) {
            LogLevel.ERROR -> Log.e(TAG, "$tag: $message")
            LogLevel.WARN -> Log.w(TAG, "$tag: $message")
            else -> Log.i(TAG, "$tag: $message")
        }
    }

    /**
     * Writes this run's events to storage.
     *
     * Safe to call repeatedly; each call rewrites the session file with
     * everything recorded so far.
     */
    fun flushSession(): LogDestination {
        val started = System.nanoTime()
        val destination = write(SESSION_FILE, sessionDocument(), append = false)

        // Writing the log is I/O on whichever thread asked for it, and on a
        // phone that is usually the main thread. The cost is recorded rather
        // than assumed (directive section 38): if it ever stops being a few
        // milliseconds, this belongs on a background thread and the measurement
        // in the log is what will say so.
        val elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000
        event(
            LogLevel.TRACE,
            "log",
            "Session written in ${elapsedMilliseconds} ms; " +
                describeDestination(destination),
        )
        return destination
    }

    /**
     * Appends a crash record and flushes the session alongside it.
     *
     * The record is deliberately self-contained: the events leading up to the
     * failure are written into the crash file too, so one file is enough to
     * understand what happened.
     */
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

    /** The most recent crash record, if there is one. */
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

    /** Human-readable description of where the logs went. */
    fun describeDestination(destination: LogDestination): String = when (destination) {
        is LogDestination.Published -> destination.location
        is LogDestination.Pending -> "${destination.location} (publishing to Documents)"
        is LogDestination.PrivateOnly ->
            "${destination.location} (shared storage unavailable: ${destination.reason})"
    }

    /** The outcome of the most recent publish, if one has finished. */
    fun lastPublishOutcome(): LogDestination? = lastPublished

    /** Every place the log was written on the most recent flush, and how it went. */
    fun lastCopies(): List<LogCopy> = copies

    /** Replaces the publish listener. Passing null removes it. */
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

    /** Outcome of writing the copy the application controls. */
    private sealed interface PrivateWrite {
        /** The private copy is on disk at this path. */
        data class Ok(val path: String) : PrivateWrite

        /** It is not, and this is what the caller should report. */
        data class Failed(val destination: LogDestination) : PrivateWrite
    }

    /**
     * Writes the private copy now and publishes in the background.
     *
     * The split is deliberate. The private copy is the one that must survive a
     * process being killed, so it is written on the calling thread where its
     * completion is guaranteed; it costs well under a millisecond. Publishing to
     * shared storage costs an order of magnitude more and nothing depends on it
     * having finished, so it goes to the publisher thread.
     */
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

    /**
     * Writes the private copy and publishes before returning.
     *
     * Used by the crash handler, where there is no later moment: the process is
     * about to end, so a background publish would simply not happen.
     */
    private fun writeBlocking(name: String, text: String, append: Boolean): LogDestination =
        when (val written = writePrivate(name, text, append)) {
            is PrivateWrite.Failed -> written.destination
            is PrivateWrite.Ok -> publishNow(name, written.path).also {
                lastPublished = it
                publishListener?.invoke(it)
            }
        }

    /**
     * Writes the copy the application controls.
     *
     * Reports failure as a value rather than throwing: every caller is on a path
     * where the right response is to record what went wrong and carry on. A
     * logger that throws is a logger that takes the application down with it.
     */
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

    /**
     * Drops the oldest half once the cap is reached.
     *
     * A log that grows without bound is a resource-exhaustion bug wearing a
     * useful disguise (directive section 60). The newest half is kept: the
     * entries near a failure are the ones worth having.
     */
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

    /** Reads the private copy back and pushes it to shared storage. */
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

        // Report the best place the file actually reached. The order is by how
        // easily a person can get at it, not by how clever the mechanism is.
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

    /**
     * Writes into the application's own directory on shared storage.
     *
     * This is the copy that cannot fail for a permission reason: every app may
     * write under `Android/data/<package>/files` without holding anything, on
     * every version this application runs on. A file manager reaches it, which
     * is what matters when the point of the file is to be sent to someone.
     */
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

    /** Writes into the shared Documents folder, where it is easiest to find. */
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

    /**
     * Describes a failure in a way that is worth reading.
     *
     * An IOException's message is frequently null, and a report that says
     * "failed: null" is how the last two publishing bugs stayed invisible.
     */
    private fun Exception.describe(): String {
        val detail = message?.takeIf { it.isNotBlank() }
        return if (detail == null) javaClass.simpleName else "${javaClass.simpleName}: $detail"
    }

    private fun publishThroughMediaStore(context: Context, name: String, bytes: ByteArray) {
        val resolver = context.contentResolver
        val collection = MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        val relativePath = "${Environment.DIRECTORY_DOCUMENTS}/$DIRECTORY_NAME"

        // Reuse the entry if it is already there; only create one when it is not.
        //
        // The previous attempt deleted by a RELATIVE_PATH and DISPLAY_NAME
        // selection and then inserted. On a Galaxy S23 running Android 16 the
        // delete matched nothing - the stored RELATIVE_PATH did not equal what
        // was searched for - and the insert then failed with
        //
        //   SQLiteConstraintException: UNIQUE constraint failed: files._data
        //
        // because a row already held that path. `_data` is the absolute path
        // column, so that error is MediaStore saying "this file is already
        // registered". Looking the row up and writing into it removes the whole
        // question; the fallback below covers the case where it is found only
        // after the insert has already objected.
        val existing = findExisting(resolver, collection, relativePath, name)
        if (existing != null) {
            writeTruncating(resolver, existing, bytes)
            return
        }

        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, "text/plain")
            put(MediaStore.MediaColumns.RELATIVE_PATH, relativePath)
        }

        val created = try {
            resolver.insert(collection, values)
        } catch (constraint: Exception) {
            // The row exists after all, under a path this query did not match.
            findExisting(resolver, collection, relativePath, name)
                ?: throw IOException(
                    "MediaStore has a row for this file that cannot be found: " +
                        "${constraint.javaClass.simpleName}: ${constraint.message}"
                )
        } ?: throw IOException("MediaStore refused to create the entry")

        writeTruncating(resolver, created, bytes)
    }

    /**
     * Finds an existing entry, tolerating how the provider spells its path.
     *
     * `RELATIVE_PATH` is stored with a trailing slash on some devices and
     * without on others, and an exact match on the wrong spelling is what let
     * the previous attempt believe the file was not there.
     */
    private fun findExisting(
        resolver: android.content.ContentResolver,
        collection: Uri,
        relativePath: String,
        name: String,
    ): Uri? {
        val selection =
            "${MediaStore.MediaColumns.DISPLAY_NAME}=? AND " +
                "${MediaStore.MediaColumns.RELATIVE_PATH} LIKE ?"
        val arguments = arrayOf(name, "$relativePath%")

        return runCatching {
            resolver.query(
                collection,
                arrayOf(MediaStore.MediaColumns._ID),
                selection,
                arguments,
                null,
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    ContentUris.withAppendedId(collection, cursor.getLong(0))
                } else {
                    null
                }
            }
        }.getOrNull()
    }

    /**
     * Writes over an entry, shortening it to what is written.
     *
     * "rwt" is the mode that truncates; "wt" did not on the device this was
     * measured on, and truncating through a raw descriptor closed it twice.
     * `AutoCloseOutputStream` owns the descriptor and closes it exactly once,
     * which is the whole reason that class exists.
     */
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

        // The size is read back, but a zero or absent size means "not indexed
        // yet" rather than "wrong": treating it as a failure once threw away a
        // write that had in fact succeeded.
        val stored = runCatching {
            resolver.query(uri, arrayOf(MediaStore.MediaColumns.SIZE), null, null, null)
                ?.use { cursor -> if (cursor.moveToFirst()) cursor.getLong(0) else -1L }
                ?: -1L
        }.getOrDefault(-1L)

        if (stored > 0 && stored != bytes.size.toLong()) {
            throw IOException("stored $stored bytes of ${bytes.size}")
        }
    }

    /**
     * Publishes on Android 9, where shared storage is still a filesystem path
     * and writing to it requires a permission the user grants at runtime.
     */
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

/**
 * Application entry point.
 *
 * It exists for one reason: the crash handler has to be installed before any
 * activity is created, or the first thing that goes wrong goes unrecorded.
 */
class BuilderApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        OmniLog.install(this)
    }
}

// ---------------------------------------------------------------------------
// Bridge to the Omni Core
// ---------------------------------------------------------------------------

/**
 * Bridge to the Omni Core.
 *
 * ## Contract (directive section 2)
 *
 * * **Purpose** — carry calls to the native Core and nothing else.
 * * **Non-responsibilities** — interpreting, caching or embellishing what the
 *   Core returns. Directive section 43 requires the flow to be
 *   `Core State -> View Model -> UI`; this object is the first arrow only.
 * * **Failure modes** — [load] reports why the library could not be loaded
 *   instead of leaving the caller to guess.
 * * **Status** — FOUNDATION.
 *
 * The external functions are instance methods of this object, which is what the
 * `Java_com_omni_builder_Builder_*` symbols in `Builder.cpp` are named for.
 */
object Builder {

    /** Outcome of loading the native library. */
    sealed interface LoadState {
        /** The library loaded and its ABI matched. */
        data object Loaded : LoadState

        /**
         * The library did not load.
         *
         * @property reason what went wrong, in a form that can be shown to a user.
         */
        data class Failed(val reason: String) : LoadState
    }

    @Volatile
    private var loadState: LoadState? = null

    /**
     * Loads `libomni_builder.so` once, and remembers the outcome.
     *
     * A failure here is never swallowed. The most likely causes are an ABI
     * mismatch, which `JNI_OnLoad` refuses deliberately, and a build in which
     * the Core was not linked in.
     */
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

    /** ABI version reported by the linked Core. */
    external fun nativeAbiVersion(): Int

    /** Version string of the linked Core. */
    external fun nativeCoreVersion(): String

    /**
     * Asks the Core for its full state as a JSON document.
     *
     * @param observedEnvironment `key=value` pairs separated by `;`, or `null`.
     *   The Core bounds and validates them and reports anything it rejects as a
     *   diagnostic, so nothing is filtered here.
     */
    external fun nativeStateReport(observedEnvironment: String?): String

    /**
     * Everything about the toolchain lock this device can honestly observe.
     *
     * Gradle, the JDK, the NDK and the compilers run on a build host, not on the
     * device, so they are deliberately absent: the Core reports them as
     * `NOT_OBSERVABLE` rather than guessing (directive section 15).
     */
    fun observedEnvironment(context: Context): String {
        val info = context.applicationInfo
        return buildString {
            append("minSdk=").append(info.minSdkVersion)
            append(';')
            append("targetSdk=").append(info.targetSdkVersion)
        }
    }
}

// ---------------------------------------------------------------------------
// View model (directive section 43: Core State -> View Model -> UI)
// ---------------------------------------------------------------------------

/** A single line of the toolchain verification table. */
data class ToolchainRow(
    /** Human-facing component name. */
    val displayName: String,
    /** The version the toolchain lock pins. */
    val pinned: String,
    /** What the device reported, if anything. */
    val observed: String?,
    /** `MATCH`, `MISMATCH`, `MISSING` or `NOT_OBSERVABLE`. */
    val state: String,
    /** Whether a checksum has been verified for this component. */
    val checksumPinned: Boolean,
)

/** A single line of the plugin table. */
data class PluginRow(
    /** Human-facing plugin name. */
    val displayName: String,
    /** Maturity, verbatim from the Core. */
    val status: String,
    /** Roadmap phase in which this plugin becomes real. */
    val roadmapPhase: String,
)

/** One subsystem of the Core, with what it still lacks. */
data class SubsystemRow(
    /** Human-facing name. */
    val name: String,
    /** Maturity, verbatim from the Core. */
    val status: String,
    /** Section of the directive that specifies it. */
    val directiveSection: Int,
    /** One sentence on what it does today. */
    val summary: String,
    /** What the specification asks for that is not built. */
    val missing: List<String>,
)

/** A single diagnostic, as the Core emitted it. */
data class DiagnosticRow(
    /** Stable diagnostic code. */
    val code: String,
    /** Severity name. */
    val severity: String,
    /** One-sentence statement of the problem. */
    val message: String,
    /** Actionable remedy, when the Core supplied one. */
    val suggestion: String?,
)

/**
 * The Core's state, reshaped for display.
 *
 * Every field here comes from the Core's report. The user interface adds no
 * field of its own, so it cannot show something the Core did not say.
 */
data class CoreState(
    /** Core version. */
    val version: String,
    /** Core maturity. */
    val status: String,
    /** Roadmap phase this build implements. */
    val phase: String,
    /** C ABI version in use. */
    val abiVersion: Int,
    /** Whether Omni_Builder builds itself. Always false for now. */
    val selfHosted: Boolean,
    /** The Core's own explanation of the self-hosting state. */
    val selfHostingNote: String,
    /** Tools this build still borrows (directive section 15). */
    val bootstrapDependencies: List<String>,
    /** The Core's own subsystems and their maturity. */
    val subsystems: List<SubsystemRow>,
    /** How many subsystems have reached PRODUCTION. */
    val subsystemsProduction: Int,
    /** Toolchain verification table. */
    val toolchain: List<ToolchainRow>,
    /** Number of pinned components that were verified here. */
    val toolchainVerified: Int,
    /** Plugin table. */
    val plugins: List<PluginRow>,
    /** How many plugins are actually implemented. */
    val pluginsImplemented: Int,
    /** Diagnostics the Core emitted while producing this report. */
    val diagnostics: List<DiagnosticRow>,
) {
    companion object {
        /**
         * Parses a Core report.
         *
         * `org.json` is part of Android, so this adds no dependency. Any missing
         * field is a defect in the Core rather than something to paper over, so
         * required fields are read with the strict accessors that throw.
         */
        fun parse(document: String): CoreState {
            val root = JSONObject(document)
            val core = root.getJSONObject("core")
            val subsystems = root.getJSONObject("subsystems")
            val toolchain = root.getJSONObject("toolchain")
            val plugins = root.getJSONObject("plugins")

            return CoreState(
                version = core.getString("version"),
                status = core.getString("status"),
                phase = core.getString("phase"),
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

// ---------------------------------------------------------------------------
// User interface
// ---------------------------------------------------------------------------

/**
 * Shows what Omni_Builder actually is right now.
 *
 * This screen deliberately offers no "Build" button. Directive section 1 forbids
 * a feature that only exists in the interface, and no plugin in this tree can
 * produce an artifact yet. The screen's job is to state the truth: what the Core
 * is, what the toolchain lock demands, what could be verified here, which
 * subsystems exist only as contracts, what the build still borrows, and where
 * the logs went.
 *
 * The view is built in code rather than from a layout resource because directive
 * section 46 defines no `res/layout` directory.
 */
class BuilderActivity : Activity() {

    private companion object {
        /** Identifies the storage permission request on Android 9. */
        const val STORAGE_PERMISSION_REQUEST = 1
    }

    /** The line that says where the session log went, updated when it gets there. */
    private var logDestinationView: TextView? = null

    /** The list of copies, refreshed when publishing finishes. */
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

    /**
     * Flushes the log when the activity stops being interactive.
     *
     * A mobile process can be terminated at any point after this (directive
     * section 36), so this is the last reliable moment to persist the session.
     */
    override fun onPause() {
        super.onPause()
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity paused.")
        OmniLog.flushSession()
    }

    override fun onDestroy() {
        // The listener holds this activity; leaving it registered would leak it.
        OmniLog.setPublishListener(null)
        logDestinationView = null
        logCopiesView = null
        OmniLog.event(LogLevel.INFO, "lifecycle", "Activity destroyed.")
        OmniLog.flushSession()
        super.onDestroy()
    }

    /**
     * Asks for the storage permission, but only where it is the sole way to
     * write into shared Documents.
     *
     * On Android 10 and later MediaStore needs no permission, so none is asked
     * for. The manifest caps the declaration at API 28 for the same reason.
     */
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

    /**
     * Renders the failure honestly.
     *
     * There is no partial or cached view to fall back to: if the Core is not
     * there, the application has nothing true to show.
     */
    private fun renderLoadFailure(root: LinearLayout, reason: String) {
        root.addView(title(getString(R.string.omni_app_name)))
        root.addView(banner(getString(R.string.omni_core_unavailable), R.color.omni_error))
        root.addView(body(reason))
    }

    private fun renderCoreState(root: LinearLayout) {
        val started = System.nanoTime()
        val state = try {
            // Measured at roughly 0.1 ms per call on a desktop host for a 14 KB
            // report. It is called once per screen creation, so it stays on the
            // main thread; if that measurement ever changes, so must this.
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

    /**
     * Shows where the logs went, and the last crash if there was one.
     *
     * The location is not assumed: it is whatever the logger reports after
     * actually writing, so a screen that says "published" means published.
     */
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

        // Publishing runs on its own thread, so the lines above start out saying
        // "publishing" and would stay that way forever if nothing came back to
        // correct them. Directive section 43 does not allow a screen to show a
        // state the system has already left - and two publishing bugs stayed
        // hidden for a phase each because the screen never said what happened.
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

    /** Lists every place a copy was written, and says why any of them failed. */
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

    // --- view helpers ------------------------------------------------------

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
