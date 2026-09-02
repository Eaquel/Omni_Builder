package com.omni.builder

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.text.Editable
import android.text.Spannable
import android.text.SpannableStringBuilder
import android.text.TextWatcher
import android.text.style.ForegroundColorSpan
import android.text.style.StyleSpan
import java.io.File
import android.util.TypedValue
import android.view.Gravity
import android.widget.EditText
import kotlin.math.max
import kotlin.math.min

/**
 * What one run of characters in a file is.
 *
 * Only the kinds worth a different colour are here. Punctuation and plain
 * identifiers are left alone: a file where every character is coloured is a
 * file where nothing stands out, and the point of colouring is that the
 * comment, the string and the keyword are the three things the eye needs to
 * find without reading.
 */
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

/**
 * The words one language spells differently from another.
 *
 * These are the real reserved word lists, not a sample of them: a keyword
 * missing from the list is a word that stays the colour of an identifier and
 * reads as a variable name, which is worse than not colouring at all.
 */
internal class Grammar(
    val keywords: Set<String>,
    val lineComment: String?,
    val blockComment: Pair<String, String>?,
    /** Whether `'` opens a character literal rather than being punctuation. */
    val characters: Boolean,
    /** Whether a word after `@` is an annotation. */
    val annotations: Boolean,
    /** Markup is lexed by a different reader: it has tags, not keywords. */
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
            // Contextual, but they read as keywords wherever they appear.
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

        /** The grammar a file's name says it is written in, if this knows one. */
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

/**
 * Reading a file into the runs that get a colour.
 *
 * One pass, left to right, no backtracking and no allocation per character.
 * It is not a parser and does not try to be: it knows where a comment starts
 * and ends, where a string starts and ends, and what a word is, which is
 * everything colouring needs and nothing it does not.
 */
internal object Reader {

    /** How much of a file is read at all. Past this, colouring stops. */
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
                    // A word starting with a capital is a type nearly always,
                    // and the times it is a constant it is still not a
                    // variable, so the colour is telling the truth either way.
                    here.isUpperCase() -> out.add(Token(at, to, Kind.TYPE))
                }
                at = to
                continue
            }

            at++
        }
    }

    /** A string, run to its closing quote, honouring backslash escapes. */
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
            // An unterminated string ends at the line, not at the file: a
            // missing quote should colour one line, not everything after it.
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

            // The tag's name, then its attributes, then the close.
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

/**
 * What was typed, so it can be untyped.
 *
 * Every edit is kept as the span it replaced and the span it put there, which
 * is enough to move in either direction. Runs of ordinary typing are merged
 * into one entry -- undo after typing a word takes the word, not the letter,
 * which is what everybody means by undo -- and a run is broken by a pause, by
 * a newline, by a deletion, or by the caret being moved somewhere else.
 */
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
        /** A pause this long starts a new entry. */
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

    /** Puts the last change back, and says where the caret belongs. */
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

/**
 * The editor a file is opened in.
 *
 * It is one `EditText` rather than a text view beside a gutter, because two
 * views scrolling together are two views that can disagree by a pixel, and a
 * gutter that drifts against its own lines is worse than none. The numbers,
 * the line under the caret and the right margin are drawn into the padding
 * this view already leaves on its left.
 *
 * Colouring is not done as the person types. The file is read into runs on a
 * worker, and only the runs the screen is actually showing become spans --
 * which is why the same editor opens a fifty line manifest and a fifteen
 * thousand line source file at the same speed.
 */
class CodeEditor(context: Context, private var palette: Palette) : EditText(context) {

    private companion object {
        /** Lines above and below the screen that are coloured anyway. */
        const val MARGIN_LINES = 40
        /** How long after the last keystroke the file is read again. */
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

    /** Set while this view is the one changing the text, so it is not recorded. */
    private var replaying = false

    private val history = History()

    fun canUndo(): Boolean = history.canUndo()

    fun canRedo(): Boolean = history.canRedo()

    /** Told whenever the text changes, so a screen can show it is unsaved. */
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

    /**
     * Opens a file: its text, and the language its name says it is in.
     *
     * The history is dropped here and nowhere else. Undo reaching back past
     * the file that is open would put another file's characters into this one.
     */
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

    /**
     * Puts the caret on a line and scrolls to it, counting from one.
     *
     * This is how a refusal becomes somewhere to look: the build says line
     * ninety, and the file opens with the caret sitting on line ninety rather
     * than at the top with a number beside it. It is posted rather than done
     * now, because a view that has not been laid out has no lines to land on.
     */
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
            // A third of the way down rather than at the very top, so what is
            // above the line is visible too.
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

    /**
     * Reads the whole file into runs, off this thread.
     *
     * A generation number is carried through, so a read that finishes after a
     * newer one started is thrown away rather than colouring the file as it
     * was two keystrokes ago.
     */
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

    /**
     * Puts spans on the part of the file the screen is showing.
     *
     * Off-screen spans cost layout work and buy nothing, so what is off screen
     * carries none. The window that was last painted is remembered and a
     * scroll that stays inside it does nothing at all.
     */
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

            // The line the caret is on, behind everything.
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

/**
 * What was typed but not saved, kept where it survives the application dying.
 *
 * A phone closes an application whenever it wants to and says nothing first,
 * so an editor that only holds what was typed in memory loses it to a phone
 * call. Every pause in typing writes the text to this application's own
 * storage, under a name derived from the file it belongs to; opening that
 * file again finds the draft, compares it against what is on disk, and offers
 * it back rather than replacing anything.
 *
 * The file the person is editing is never written to by any of this. A draft
 * is a copy beside it, and only pressing save puts anything into the project.
 */
internal object Drafts {

    private const val FOLDER = "Drafts"

    /** How long after the last keystroke a draft is written. */
    const val REST_MILLIS = 1_500L

    private fun folder(context: Context): File =
        File(context.filesDir, FOLDER).also { it.mkdirs() }

    /**
     * A file name for a path, which is a name and a number.
     *
     * The name is there so a person looking in the folder can tell what it is,
     * and the number is what makes it unambiguous: two projects can hold a
     * `Main.java`, and a draft of one must never open in the other.
     */
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
