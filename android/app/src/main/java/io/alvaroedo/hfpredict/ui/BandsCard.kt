package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.PAST_HOURS
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.Band
import io.alvaroedo.hfpredict.core.Quality
import io.alvaroedo.hfpredict.core.bandLabel
import io.alvaroedo.hfpredict.core.bandMhz
import io.alvaroedo.hfpredict.core.bandsDisplayOrder
import io.alvaroedo.hfpredict.core.qualityOf
import io.alvaroedo.hfpredict.tr
import kotlin.math.ln
import kotlin.math.roundToInt

private val DISPLAY = bandsDisplayOrder()

@Composable
fun BandsCard(state: UiState, vm: AppViewModel, fmt: Format, modifier: Modifier = Modifier) {
    val ctx = LocalResources.current
    val forecast = state.forecast
    val to = forecast?.to
    SectionCard(modifier) {
        Text(
            if (to != null) tr(R.string.sections_allBandsTo, "place" to to.name) else tr(R.string.sections_allBandsAnywhere),
            style = MaterialTheme.typography.titleMedium,
        )
        val hour = fmt.hour(state.hour)
        Kicker(
            if (state.hour == state.nowHour) tr(R.string.grid_readoutNow, "hour" to hour, "band" to bandLabel(state.band))
            else tr(R.string.grid_readoutAt, "hour" to hour, "band" to bandLabel(state.band)),
            color = MaterialTheme.colorScheme.primary,
        )

        forecast?.window?.let { w ->
            val ceiling = w.hpfByHour.getOrNull(state.hour)
            val floor = w.lufByHour.getOrNull(state.hour)
            Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainer) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Kicker(tr(R.string.window_label), Modifier.weight(1f))
                        Text(
                            when {
                                floor != null && ceiling != null -> tr(R.string.window_floorCeiling, "floor" to fmt.mhz(ctx, floor), "ceiling" to fmt.mhz(ctx, ceiling))
                                ceiling != null -> tr(R.string.window_ceilingOnly, "ceiling" to fmt.mhz(ctx, ceiling))
                                else -> tr(R.string.window_none)
                            },
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                    WindowRail(floor, ceiling ?: forecast.mufByHour.getOrNull(state.hour)?.takeIf { it > 0 }, state.band)
                }
            }
        }

        Row(verticalAlignment = Alignment.CenterVertically) {
            Kicker(
                if (to != null) tr(R.string.grid_unitChance) else tr(R.string.grid_unitDirections),
                Modifier.weight(1f),
            )
            TextButton(onClick = vm::toggleTable) {
                Text(if (state.showTable) tr(R.string.grid_showAsGrid) else tr(R.string.grid_showAsTable))
            }
        }
        if (state.showTable) BandTable(state, fmt) else Heatmap(state, vm)
        QualityLegend()
        if (forecast?.cells?.all { qualityOf(it.reliability) == Quality.CLOSED } == true) {
            Text(
                if (to != null) tr(R.string.sections_noneReach, "place" to to.name) else tr(R.string.sections_noneReachAnywhere),
                style = MaterialTheme.typography.titleSmall,
            )
        }
        Text(
            tr(R.string.grid_footnoteHours, "place" to (state.saved.from?.name ?: "Greenwich")),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** 2 to 30 MHz on a log scale, the window shaded and the bands ticked. */
@Composable
private fun WindowRail(floor: Double?, ceiling: Double?, band: Band) {
    val colors = LocalMapColors.current
    val dim = MaterialTheme.colorScheme.onSurfaceVariant
    val measurer = rememberTextMeasurer()
    val small = MaterialTheme.typography.labelSmall.copy(color = dim)
    Canvas(Modifier.fillMaxWidth().height(40.dp)) {
        val lo = ln(2.0)
        val hi = ln(30.0)
        fun x(mhz: Double) = ((ln(mhz.coerceIn(2.0, 30.0)) - lo) / (hi - lo)).toFloat() * size.width
        val y = 12.dp.toPx()
        val h = 6.dp.toPx()
        drawRoundRect(colors.cells[0], Offset(0f, y - h / 2), Size(size.width, h), CornerRadius(h / 2))
        if (ceiling != null) {
            val x0 = floor?.let { x(it) } ?: 0f
            val x1 = x(ceiling)
            if (x1 > x0) drawRoundRect(colors.cells[2], Offset(x0, y - h / 2), Size(x1 - x0, h), CornerRadius(h / 2))
        }
        for (b in DISPLAY) {
            val active = b == band
            val tick = if (active) 16.dp.toPx() else 10.dp.toPx()
            drawRoundRect(
                if (active) colors.now else dim.copy(alpha = 0.7f),
                Offset(x(bandMhz(b)) - 1.dp.toPx(), y - tick / 2),
                Size(2.dp.toPx(), tick),
                CornerRadius(1.dp.toPx()),
            )
        }
        drawText(measurer, "2 MHz", Offset(0f, size.height - 14.dp.toPx()), small)
        val r = measurer.measure("30 MHz", small)
        drawText(r, topLeft = Offset(size.width - r.size.width, size.height - 14.dp.toPx()))
    }
}

/** Every band at every hour; tap a cell to show that band and hour. */
@Composable
private fun Heatmap(state: UiState, vm: AppViewModel) {
    val colors = LocalMapColors.current
    val scheme = MaterialTheme.colorScheme
    // 34 labels (bands and hours): the default cache of 8 missed on every draw.
    val measurer = rememberTextMeasurer(cacheSize = 64)
    val start = state.trackStart
    val cells = remember(state.forecast) {
        state.forecast?.cells?.associate { (it.band to it.hour.toInt()) to it.reliability } ?: emptyMap()
    }
    val rowH = 26.dp
    val labelW = 46.dp
    val axisH = 22.dp
    val description = tr(R.string.grid_unitChance)
    val haptics = LocalHapticFeedback.current
    val latest by rememberUpdatedState(state)
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(rowH * DISPLAY.size + axisH)
            .semantics { contentDescription = description }
            .pointerInput(start) {
                detectTapGestures { p ->
                    val colW = (size.width - labelW.toPx()) / 24f
                    val col = ((p.x - labelW.toPx()) / colW).toInt()
                    val row = (p.y / rowH.toPx()).toInt()
                    if (col in 0..23 && row in DISPLAY.indices) {
                        haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                        vm.select(DISPLAY[row], (start + col) % 24)
                    }
                }
            }
            // A sideways drag slides the hour: the cursor stays under the
            // finger and each hour crossed ticks.
            .pointerInput(start) {
                detectHorizontalDragGestures { change, _ ->
                    val colW = (size.width - labelW.toPx()) / 24f
                    val col = ((change.position.x - labelW.toPx()) / colW).toInt().coerceIn(0, 23)
                    val hour = (start + col) % 24
                    val s = latest
                    if (hour != s.hour) {
                        haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                        vm.select(s.band, hour)
                    }
                    change.consume()
                }
            },
    ) {
        val colW = (size.width - labelW.toPx()) / 24f
        val rh = rowH.toPx()
        val gap = 3.dp.toPx()
        DISPLAY.forEachIndexed { row, band ->
            val y = row * rh
            val selected = band == state.band
            val style = TextStyle(
                fontSize = 13.sp,
                fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                color = if (selected) scheme.onSurface else scheme.onSurfaceVariant,
            )
            val t = measurer.measure(bandLabel(band), style)
            drawText(t, topLeft = Offset(0f, y + (rh - t.size.height) / 2))
            for (i in 0 until 24) {
                val hour = (start + i) % 24
                val r = cells[band to hour]
                val c = if (r == null) colors.cells[0] else colors.cell(qualityOf(r))
                drawRoundRect(
                    c,
                    Offset(labelW.toPx() + i * colW + gap / 2, y + gap / 2),
                    Size(colW - gap, rh - gap),
                    CornerRadius(4.dp.toPx()),
                )
            }
        }
        val sel = Math.floorMod(state.hour - start, 24)
        val x = labelW.toPx() + sel * colW
        drawRoundRect(colors.now, Offset(x, 0f), Size(colW, rh * DISPLAY.size), CornerRadius(5.dp.toPx()), style = Stroke(2.dp.toPx()))
        val row = DISPLAY.indexOf(state.band)
        drawRoundRect(
            scheme.onSurface,
            Offset(x + 1.5.dp.toPx(), row * rh + 1.5.dp.toPx()),
            Size(colW - 3.dp.toPx(), rh - 3.dp.toPx()),
            CornerRadius(4.dp.toPx()),
            style = Stroke(2.dp.toPx()),
        )
        drawCircle(colors.now, 2.5.dp.toPx(), Offset(labelW.toPx() + (PAST_HOURS + 0.5f) * colW, rh * DISPLAY.size + 3.dp.toPx()))
        val step = if (colW < 18.dp.toPx()) 4 else 2
        for (i in 0 until 24 step step) {
            val t = measurer.measure(
                "%02d".format((start + i) % 24),
                TextStyle(fontSize = 11.sp, color = if (i == sel) colors.now else scheme.onSurfaceVariant, textAlign = TextAlign.Center),
            )
            drawText(t, topLeft = Offset(labelW.toPx() + (i + 0.5f) * colW - t.size.width / 2, rh * DISPLAY.size + 6.dp.toPx()))
        }
    }
}

@Composable
private fun BandTable(state: UiState, fmt: Format) {
    val start = state.trackStart
    val forecast = state.forecast ?: return
    Row(Modifier.horizontalScroll(rememberScrollState())) {
        Column {
            Text(tr(R.string.grid_bandColumn), style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
            DISPLAY.forEach { b ->
                Text(
                    bandLabel(b),
                    Modifier.height(24.dp).padding(end = 8.dp),
                    fontWeight = if (b == state.band) FontWeight.Bold else FontWeight.Normal,
                )
            }
        }
        for (i in 0 until 24) {
            val hour = (start + i) % 24
            Column(Modifier.width(36.dp), horizontalAlignment = Alignment.End) {
                Text(
                    "%02d".format(hour),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = if (hour == state.hour) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
                DISPLAY.forEach { b ->
                    val r = forecast.cells.firstOrNull { it.band == b && it.hour.toInt() == hour }?.reliability ?: 0.0
                    Text(
                        fmt.int(r * 100),
                        Modifier.height(24.dp),
                        style = MaterialTheme.typography.bodyMedium,
                        // Traffic-light numbers, green to red.
                        color = LocalMapColors.current.semaphore(qualityOf(r)),
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
        Spacer(Modifier.width(8.dp))
    }
}
