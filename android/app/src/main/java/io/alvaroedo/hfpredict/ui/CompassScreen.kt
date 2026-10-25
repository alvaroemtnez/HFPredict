package io.alvaroedo.hfpredict.ui

import android.content.Context
import android.hardware.GeomagneticField
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Build
import android.view.Surface as DisplaySurface
import android.view.WindowManager
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.core.Station
import io.alvaroedo.hfpredict.core.antennaAskedAsWire
import io.alvaroedo.hfpredict.core.antennaLobes
import io.alvaroedo.hfpredict.core.antennaUsesBeam
import io.alvaroedo.hfpredict.core.longPathDeg
import io.alvaroedo.hfpredict.core.turnDeg
import io.alvaroedo.hfpredict.tr
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.roundToInt
import kotlin.math.sin

/*
 * Pointing an antenna with the phone's compass. The heading is the
 * direction of the phone's top edge (or, held upright, of its back), from
 * the fused rotation-vector sensor: no permission needed. Android gives it
 * against magnetic north; the forecast's bearings are true north, so the
 * magnetic declination at the station (the platform's World Magnetic Model,
 * offline) is added.
 */

/** What the compass reads. `heading` is true north, degrees; null until the first reading. */
data class Heading(val heading: Double?, val unsure: Boolean, val declination: Double)

/** The fused orientation sensor, or the magnetometer-only one; null on devices with neither. */
fun compassSensor(context: Context): Sensor? {
    val sm = context.getSystemService(SensorManager::class.java) ?: return null
    return sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
        ?: sm.getDefaultSensor(Sensor.TYPE_GEOMAGNETIC_ROTATION_VECTOR)
}

@Composable
fun rememberHeading(lat: Double, lon: Double): Heading {
    val context = LocalContext.current
    val declination = remember(lat, lon) {
        GeomagneticField(lat.toFloat(), lon.toFloat(), 0f, System.currentTimeMillis()).declination.toDouble()
    }
    var heading by remember { mutableStateOf<Double?>(null) }
    var unsure by remember { mutableStateOf(false) }
    DisposableEffect(context) {
        val sm = context.getSystemService(SensorManager::class.java)
        val sensor = compassSensor(context)
        val magnetometer = sm?.getDefaultSensor(Sensor.TYPE_MAGNETIC_FIELD)
        val r = FloatArray(9)
        // Smoothed as a unit vector, so the average of 359° and 1° is 0°.
        var sx = 0.0
        var sy = 0.0
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                if (event.sensor.type == Sensor.TYPE_MAGNETIC_FIELD) return
                SensorManager.getRotationMatrixFromVector(r, event.values)
                // World axes: x east, y (magnetic) north, z up. The phone's
                // top edge is its y axis, its back is -z; whichever lies
                // flatter gives the heading.
                val (ex, ny) = if (hypot(r[1].toDouble(), r[4].toDouble()) >= hypot(r[2].toDouble(), r[5].toDouble())) {
                    r[1].toDouble() to r[4].toDouble()
                } else {
                    -r[2].toDouble() to -r[5].toDouble()
                }
                val a = atan2(ex, ny)
                val k = if (sx == 0.0 && sy == 0.0) 1.0 else 0.18
                sx += (sin(a) - sx) * k
                sy += (cos(a) - sy) * k
                val magnetic = Math.toDegrees(atan2(sx, sy))
                heading = Math.floorMod((magnetic + declination + displayTurn(context)).roundToInt() * 10, 3600) / 10.0
            }

            override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {
                if (sensor.type == Sensor.TYPE_MAGNETIC_FIELD || sensor == compassSensor(context)) {
                    unsure = accuracy == SensorManager.SENSOR_STATUS_UNRELIABLE ||
                        accuracy == SensorManager.SENSOR_STATUS_ACCURACY_LOW
                }
            }
        }
        if (sm != null && sensor != null) {
            sm.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_UI)
            magnetometer?.let { sm.registerListener(listener, it, SensorManager.SENSOR_DELAY_NORMAL) }
        }
        onDispose { sm?.unregisterListener(listener) }
    }
    return Heading(heading, unsure, declination)
}

/** A tablet turned on its side: the screen's top is no longer the device's. */
private fun displayTurn(context: Context): Int {
    val rotation = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        context.display.rotation
    } else {
        @Suppress("DEPRECATION")
        context.getSystemService(WindowManager::class.java).defaultDisplay.rotation
    }
    return when (rotation) {
        DisplaySurface.ROTATION_90 -> 90
        DisplaySurface.ROTATION_180 -> 180
        DisplaySurface.ROTATION_270 -> 270
        else -> 0
    }
}

/** What the compass is for: turning towards a bearing, or reading one off. */
sealed interface CompassGoal {
    /** Turn to `bearing` (true, short path), named `name`; null just shows the heading. */
    data class Point(val bearing: Double?, val name: String?) : CompassGoal
    /** Read the antenna's own direction for the station editor. */
    data class Measure(val station: Station, val use: (Double) -> Unit) : CompassGoal
}

/** Within this many degrees counts as on target; leaving takes a little more. */
private const val ON_TARGET = 3.0
private const val OFF_TARGET = 5.0

@Composable
fun CompassScreen(
    goal: CompassGoal,
    station: Station,
    lat: Double,
    lon: Double,
    fmt: Format,
    compact: Boolean,
    onDismiss: () -> Unit,
) {
    val reading = rememberHeading(lat, lon)
    val haptics = LocalHapticFeedback.current
    val view = LocalView.current
    // The screen stays on while aiming.
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
    var longPath by remember { mutableStateOf(false) }
    val point = goal as? CompassGoal.Point
    val target = point?.bearing?.let { if (longPath) longPathDeg(it) else it }
    val heading = reading.heading
    val turn = if (heading != null && target != null) turnDeg(heading, target) else null

    // A firm tap on arriving on target; leaving takes a little more, so
    // a trembling hand does not buzz.
    var onTarget by remember { mutableStateOf(false) }
    LaunchedEffect(turn) {
        if (turn == null) return@LaunchedEffect
        if (!onTarget && abs(turn) <= ON_TARGET) {
            onTarget = true
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
        } else if (onTarget && abs(turn) > OFF_TARGET) {
            onTarget = false
        }
    }

    val title = tr(if (goal is CompassGoal.Measure) R.string.compass_measureTitle else R.string.compass_title)
    FullScreenDialog(title, onDismiss, compact) { mod ->
        Column(
            mod.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (point?.bearing != null) {
                SingleChoiceSegmentedButtonRow(Modifier.widthIn(max = 420.dp).fillMaxWidth()) {
                    SegmentedButton(!longPath, { longPath = false }, SegmentedButtonDefaults.itemShape(0, 2)) { Text(tr(R.string.compass_shortPath)) }
                    SegmentedButton(longPath, { longPath = true }, SegmentedButtonDefaults.itemShape(1, 2)) { Text(tr(R.string.compass_longPath)) }
                }
            }

            val lobes = if (goal is CompassGoal.Point && antennaUsesBeam(station.antenna.kind)) {
                antennaLobes(station.antenna.beamDeg, station.antenna.kind)
            } else emptyList()
            Dial(heading, target, lobes, onTarget, Modifier.widthIn(max = 360.dp).fillMaxWidth())

            val headingText = heading?.let { fmt.degrees(it) + "°" } ?: "–"
            val headingLabel = tr(R.string.compass_heading) + " " + headingText
            Text(
                headingText,
                style = MaterialTheme.typography.displayMedium,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.semantics { contentDescription = headingLabel },
            )
            if (target != null) {
                val name = point.name?.let { "$it · " } ?: ""
                Text(
                    tr(R.string.compass_target) + ": " + name + fmt.degrees(target) + "°",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (turn != null) {
                    val instruction = when {
                        onTarget -> tr(R.string.compass_onTarget)
                        turn < 0 -> tr(R.string.compass_turnLeft, "degrees" to fmt.degrees(abs(turn)))
                        else -> tr(R.string.compass_turnRight, "degrees" to fmt.degrees(turn))
                    }
                    Text(
                        instruction,
                        style = MaterialTheme.typography.headlineSmall,
                        fontWeight = FontWeight.Bold,
                        color = if (onTarget) LocalMapColors.current.live else MaterialTheme.colorScheme.primary,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
            if (goal is CompassGoal.Measure && heading != null) {
                Button(onClick = { goal.use(heading.roundToInt().toDouble() % 360) }) {
                    Text(tr(R.string.compass_use, "degrees" to fmt.degrees(heading)))
                }
            }

            if (reading.unsure) {
                Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.errorContainer) {
                    Text(
                        tr(R.string.compass_calibrate),
                        Modifier.padding(12.dp),
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
            val wire = goal is CompassGoal.Measure && antennaAskedAsWire(goal.station.antenna.kind)
            Text(
                tr(if (wire) R.string.compass_holdWire else R.string.compass_holdBoom),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (lobes.isNotEmpty()) {
                Text(
                    tr(R.string.compass_lobes) + ": " + lobes.joinToString(" / ") { fmt.degrees(it) + "°" },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(
                tr(R.string.compass_declination, "degrees" to fmt.dec(reading.declination, 1)),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * The card turns under a fixed index at the top (where the phone points):
 * north, the degrees, the target as a line in the highlight colour and the
 * antenna's lobes as short marks.
 */
@Composable
private fun Dial(heading: Double?, target: Double?, lobes: List<Double>, onTarget: Boolean, modifier: Modifier) {
    val scheme = MaterialTheme.colorScheme
    val colors = LocalMapColors.current
    val highlight = Color(colors.palette.path.toInt())
    val measurer = rememberTextMeasurer()
    val label = TextStyle(fontSize = 13.sp, color = scheme.onSurfaceVariant)
    Canvas(modifier.padding(8.dp).aspectRatio(1f)) {
        val c = Offset(size.width / 2, size.height / 2)
        val r = size.minDimension / 2 - 6.dp.toPx()
        drawCircle(scheme.surfaceContainerHigh, r, c)
        drawCircle(scheme.outlineVariant, r, c, style = Stroke(1.dp.toPx()))
        val turnBy = -(heading ?: 0.0).toFloat()
        rotate(turnBy, c) {
            for (d in 0 until 360 step 5) {
                val long = d % 30 == 0
                val len = (if (long) 14 else 7).dp.toPx()
                val a = Math.toRadians(d.toDouble())
                val o = Offset(c.x + (r * sin(a)).toFloat(), c.y - (r * cos(a)).toFloat())
                val i = Offset(c.x + ((r - len) * sin(a)).toFloat(), c.y - ((r - len) * cos(a)).toFloat())
                drawLine(if (long) scheme.onSurface else scheme.outline, i, o, (if (long) 2 else 1).dp.toPx())
            }
            for (d in listOf(0, 90, 180, 270)) {
                val t = measurer.measure(if (d == 0) "N" else d.toString(), label.copy(fontWeight = if (d == 0) FontWeight.Bold else FontWeight.Normal))
                val a = Math.toRadians(d.toDouble())
                val rr = r - 30.dp.toPx()
                val p = Offset(c.x + (rr * sin(a)).toFloat(), c.y - (rr * cos(a)).toFloat())
                rotate(-turnBy, p) {
                    drawText(t, topLeft = Offset(p.x - t.size.width / 2, p.y - t.size.height / 2))
                }
            }
            // North: a filled triangle at the rim.
            val n = Path().apply {
                moveTo(c.x, c.y - r + 2.dp.toPx())
                lineTo(c.x - 7.dp.toPx(), c.y - r + 16.dp.toPx())
                lineTo(c.x + 7.dp.toPx(), c.y - r + 16.dp.toPx())
                close()
            }
            drawPath(n, scheme.error)
            for (l in lobes) {
                val a = Math.toRadians(l)
                val o = Offset(c.x + ((r - 18.dp.toPx()) * sin(a)).toFloat(), c.y - ((r - 18.dp.toPx()) * cos(a)).toFloat())
                drawLine(colors.cells[2], c, o, 4.dp.toPx(), StrokeCap.Round)
            }
            if (target != null) {
                val a = Math.toRadians(target)
                val o = Offset(c.x + (r * sin(a)).toFloat(), c.y - (r * cos(a)).toFloat())
                drawLine(Color.Black.copy(alpha = 0.6f), c, o, 6.dp.toPx(), StrokeCap.Round)
                drawLine(highlight, c, o, 3.5.dp.toPx(), StrokeCap.Round)
                drawCircle(highlight, 7.dp.toPx(), o)
            }
        }
        // The index: where the phone points.
        val index = Path().apply {
            moveTo(c.x, c.y - r + 16.dp.toPx())
            lineTo(c.x - 10.dp.toPx(), c.y - r - 5.dp.toPx())
            lineTo(c.x + 10.dp.toPx(), c.y - r - 5.dp.toPx())
            close()
        }
        drawPath(index, if (onTarget) colors.live else scheme.primary)
        drawCircle(scheme.onSurface, 4.dp.toPx(), c)
    }
}
