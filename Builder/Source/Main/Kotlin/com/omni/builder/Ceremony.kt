package com.omni.builder

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import android.graphics.RadialGradient
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import android.os.SystemClock
import android.view.View
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/**
 * The pieces every ceremony in this application draws with.
 *
 * Three things are worth saying about the drawing here. It is all on a canvas
 * rather than assembled out of views, because what is being drawn -- a few
 * hundred particles finding their way onto the outline of a key -- is not a
 * layout and pretending it is one costs a frame. It allocates nothing while it
 * runs: every array, path and paint is made once, because a garbage collection
 * in the middle of an animation is a visible stutter and nothing else. And it
 * is driven by the clock rather than by a frame count, so the same ceremony
 * takes the same time on a phone that draws it at 120 frames a second and on
 * one that manages 45.
 */
internal object Ink {

    const val FRAME_NANOS_60 = 16_666_667L

    /** Smoothstep: 0 below the start, 1 above the end, eased in between. */
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

    /** Towards white, which is what a light looks like when it is bright. */
    fun hot(colour: Int, by: Float): Int {
        val held = by.coerceIn(0f, 1f)
        return Color.rgb(
            mix(Color.red(colour).toFloat(), 255f, held).toInt(),
            mix(Color.green(colour).toFloat(), 255f, held).toInt(),
            mix(Color.blue(colour).toFloat(), 255f, held).toInt(),
        )
    }

    /**
     * A number the same every time for the same seed.
     *
     * The particles are laid out from this rather than from `Math.random`, so
     * that the ceremony looks the same on every run and a screenshot of it
     * means something.
     */
    fun scatter(seed: Int, salt: Int): Float {
        var held = seed * 374_761_393 + salt * 668_265_263
        held = (held xor (held shr 13)) * 1_274_126_177
        return ((held xor (held shr 16)) and 0x00FF_FFFF) / 16_777_216f
    }
}

/**
 * The field of 0s and 1s a screen passes through on its way to another.
 *
 * Every screen change in this application dissolves through this rather than
 * cutting: a wipe of binary digits crosses the content, brightest at its
 * leading edge, and the content it is covering fades out behind it and the new
 * one fades in. It lasts less than half a second, which is long enough to see
 * and short enough that nobody waits for it.
 *
 * It is a view of its own laid over the content, so nothing about the screens
 * themselves has to know it exists.
 */
class BinaryVeil(context: Context) : View(context) {

    private companion object {
        const val COLUMNS = 26
        const val ROWS = 44
        const val SWEEP_MILLIS = 420f
        /** How much of the height the bright edge covers. */
        const val EDGE = 0.22f
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        textAlign = Paint.Align.CENTER
    }
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

    /**
     * Runs the wipe, changing what is underneath while it is covered.
     *
     * `swap` is called once, at the moment the bright edge is across the
     * middle of the screen -- which is the only moment during the sweep when
     * the change it makes cannot be seen happening. Everything before that is
     * the old screen going out and everything after is the new one arriving.
     */
    fun sweep(colour: Int, swap: () -> Unit) {
        accent = colour
        whenDone = swap
        began = SystemClock.uptimeMillis()
        running = true
        swapped = false
        visibility = VISIBLE
        alpha = 1f
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
            swapNow()
            return
        }
        if (t >= 0.5f) swapNow()

        // The wipe runs top to bottom and back out, so the field is never on
        // screen at full strength for more than an instant.
        val front = t * (1f + EDGE * 2f) - EDGE
        val columnWidth = w / COLUMNS
        val rowHeight = h / ROWS
        paint.textSize = min(columnWidth * 0.72f, rowHeight * 0.86f)

        for (row in 0 until ROWS) {
            val y = (row + 0.5f) / ROWS
            val distance = abs(y - front)
            if (distance > EDGE) continue
            val near = 1f - distance / EDGE
            for (column in 0 until COLUMNS) {
                // Each cell keeps its digit for the whole sweep, and which
                // cells are lit at all is fixed too: a field that reshuffles
                // every frame reads as noise rather than as a transition.
                val roll = Ink.scatter(row * 131 + column, 7)
                if (roll > 0.55f + near * 0.4f) continue
                glyph[0] = if (Ink.scatter(row * 977 + column, 13) > 0.5f) '1' else '0'
                val lit = near * near
                paint.color = Ink.fade(Ink.hot(accent, lit * 0.7f), 0.15f + lit * 0.85f)
                canvas.drawText(
                    glyph, 0, 1,
                    (column + 0.5f) * columnWidth,
                    (row + 0.72f) * rowHeight,
                    paint,
                )
            }
        }
    }
}

/**
 * Making a signing key, drawn while the key is really being made.
 *
 * Nothing here is on a timer waiting to declare success. The forging loop runs
 * for exactly as long as the Core takes to generate the key -- which on a
 * phone is anywhere between a second and most of a minute for 4096 bits -- and
 * the seal at the end is played when the key exists and its fingerprint is in
 * hand. If the Core refuses, the ceremony is told so and stops cold rather
 * than finishing an animation that would say something untrue.
 *
 * What is drawn: a field of bits swirling in the dark, drawn towards the
 * outline of a key until they settle on it; the key lighting up along that
 * outline; circuit traces filling from the shaft into the bow; and, when the
 * key is real, a lock closing inside the bow and a ring going out.
 */
class KeyForgeView(context: Context, private var palette: Palette) : View(context) {

    private companion object {
        const val DOTS = 520
        const val GLYPHS = 130
        const val LOOP_MILLIS = 4_200f
        const val SEAL_MILLIS = 1_150f
        /** Where the outline is reached and the key starts to light. */
        const val GATHERED = 0.44f
    }

    private enum class Phase { FORGING, SEALING, SEALED, REFUSED }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
        textAlign = Paint.Align.CENTER
    }
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

    /** What the ceremony says it is doing, in the language the app is in. */
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

    /** The key exists. This is the only thing that starts the finale. */
    fun seal(shown: String) {
        if (phase != Phase.FORGING) return
        fingerprint = shown
        sealedAt = SystemClock.uptimeMillis()
        phase = Phase.SEALING
    }

    /** The Core refused. The ceremony stops where it is. */
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

    /**
     * Where every particle is going.
     *
     * The targets are points on the key's own outline, walked with a
     * `PathMeasure`, so the shape the particles settle into is the shape that
     * is about to be drawn rather than a second description of it that could
     * drift.
     */
    private fun layOut(w: Float, h: Float) {
        if (w <= 0f || h <= 0f) return
        buildKey(w, h)
        measure.setPath(key, false)
        var length = measure.length
        // A path of several contours: `PathMeasure` walks them one at a time.
        val lengths = ArrayList<Float>()
        do {
            lengths.add(measure.length)
        } while (measure.nextContour())
        length = lengths.sum()
        if (length <= 0f) return

        val everything = DOTS + GLYPHS
        for (index in 0 until everything) {
            // Two out of three settle on the outline; the rest fill the bow,
            // which is what makes it read as a solid head rather than a ring.
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

    /**
     * The key itself: a round bow with a hole through it, a shaft, and teeth.
     *
     * Written in fractions of the space it is given, so it is the same key on
     * a small phone and on a tablet.
     */
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

        // The bow, and the hole through it.
        key.addCircle(cx, cy, r, Path.Direction.CW)
        key.addCircle(cx, cy - r * 0.30f, r * 0.30f, Path.Direction.CCW)

        // The shaft, down from the bow.
        val half = span * 0.036f
        val top = cy + r * 0.92f
        val foot = h * 0.80f
        key.moveTo(cx - half, top)
        key.lineTo(cx - half, foot)

        // The teeth, cut into the right-hand edge on the way back up.
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

        // The traces that fill from the shaft up into the bow, drawn as one
        // path so the whole circuit lights along its length at once.
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
        // The gather runs once and stays gathered: a ceremony that scattered
        // the key every four seconds would look like it had failed and
        // started again.
        val gather = Ink.ease(life, 0.30f, 2.10f)
        val reveal = Ink.ease(life, 1.05f, 2.35f)
        val sealing = if (phase == Phase.FORGING) 0f else
            ((now - sealedAt) / SEAL_MILLIS).coerceIn(0f, 1f)
        if (phase == Phase.SEALING && sealing >= 1f) phase = Phase.SEALED

        drawGround(canvas, w, h, life)
        drawParticles(canvas, w, h, life, gather, sealing)
        if (reveal > 0f) drawKey(canvas, life, reveal, sealing)
        drawWords(canvas, w, h, life, sealing)
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

        // A grid that drifts upwards, which is what gives the dark a depth to
        // sit in rather than being flat black.
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
        // The seal throws everything outward again, so the key is left alone
        // on the screen at the end.
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

        // And the ones that are digits rather than dots, which is what says
        // what the key is made of.
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

    private fun drawKey(canvas: Canvas, life: Float, reveal: Float, sealing: Float) {
        val breath = 0.82f + 0.18f * sin(life * 2.1f)
        paint.style = Paint.Style.STROKE
        paint.strokeJoin = Paint.Join.ROUND

        // Three passes: a wide soft halo, a mid glow, and a hairline. Layering
        // them is what a glow is; one wide translucent stroke is a smudge.
        paint.strokeWidth = bowRadius * 0.34f
        paint.color = Ink.fade(palette.accent, 0.13f * reveal * breath)
        canvas.drawPath(key, paint)

        paint.strokeWidth = bowRadius * 0.11f
        paint.color = Ink.fade(Ink.hot(palette.accent, 0.35f), 0.42f * reveal)
        canvas.drawPath(key, paint)

        paint.strokeWidth = max(1.4f, bowRadius * 0.028f)
        paint.color = Ink.fade(Ink.hot(palette.accent, 0.85f + sealing * 0.15f), reveal)
        canvas.drawPath(key, paint)

        // The traces, filling along their own length.
        val filled = Ink.ease(life, 1.7f, 3.0f)
        if (filled > 0f) {
            paint.strokeWidth = max(1f, bowRadius * 0.020f)
            paint.color = Ink.fade(Ink.hot(palette.accent, 0.55f), 0.75f * filled)
            canvas.drawPath(trace, paint)
        }

        // Two arcs turning inside the bow, one each way.
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

        // The lock, which closes only once the key is real.
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

            // And the ring going out, which is the moment it is sealed.
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

/**
 * A build, drawn stage by stage while the Core does the work.
 *
 * Everything on this screen is read from the Core rather than acted out: the
 * stage is the one the build is in, the percentage is how far through it is
 * against what this device took last time, and the time remaining is what is
 * left of that. Before a device has built anything the estimate says so, and
 * from the second build onwards it is that device's own measurement.
 *
 * The arc eases towards the figure rather than jumping to it, which is the one
 * liberty taken here: the number is the Core's, and only the way it arrives on
 * screen is this view's.
 */
class BuildStageView(context: Context, private var palette: Palette) : View(context) {

    private companion object {
        const val RAIN = 90
        /** How fast the drawn arc closes on the reported one, per second. */
        const val CATCH_UP = 3.4f
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Typeface.MONOSPACE
    }
    private val ring = RectF()
    private val glyph = CharArray(1)

    private var running = false
    private var lastFrame = 0L
    private var began = SystemClock.uptimeMillis()

    /** What the Core last said, and how much of it is on screen so far. */
    private var reported = 0f
    private var drawn = 0f
    private var stageAt = 0
    private var stageCount = 1
    private var refusedAt = -1
    private var done = false

    /** The words come from the activity, which is where the language lives. */
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

    /** What the Core says, taken as it is. */
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

        // A mark running around the rim while there is work happening, so a
        // long stage never looks like a hung one.
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
