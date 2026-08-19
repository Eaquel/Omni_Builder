package com.omni.builder

import android.app.Activity
import android.graphics.Typeface
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject

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
            LoadState.Loaded
        } catch (error: UnsatisfiedLinkError) {
            LoadState.Failed(
                "The native library could not be loaded: ${error.message ?: "no detail"}. " +
                    "This usually means the Omni Core was not linked into this build, " +
                    "or the bridge and the Core disagree on the ABI version."
            )
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
    fun observedEnvironment(context: Activity): String {
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
 * subsystems exist only as contracts, and what the build still borrows.
 *
 * The view is built in code rather than from a layout resource because directive
 * section 46 defines no `res/layout` directory.
 */
class BuilderActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(color(R.color.omni_background))
            setPadding(dp(R.dimen.omni_screen_padding))
        }

        when (val load = Builder.load()) {
            is Builder.LoadState.Failed -> renderLoadFailure(root, load.reason)
            is Builder.LoadState.Loaded -> renderCoreState(root)
        }

        setContentView(
            ScrollView(this).apply {
                setBackgroundColor(color(R.color.omni_background))
                isFillViewport = true
                addView(root, MATCH_PARENT, WRAP_CONTENT)
            }
        )
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
        val state = try {
            // Measured at roughly 0.1 ms per call on a desktop host for a 14 KB
            // report. It is called once per screen creation, so it stays on the
            // main thread; if that measurement ever changes, so must this.
            CoreState.parse(Builder.nativeStateReport(Builder.observedEnvironment(this)))
        } catch (error: RuntimeException) {
            renderLoadFailure(
                root,
                "The Core produced a report this build cannot read: ${error.message}. " +
                    "The interface and the Core are out of step."
            )
            return
        }

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
