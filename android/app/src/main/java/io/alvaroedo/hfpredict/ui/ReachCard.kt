package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.PAST_HOURS
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.WHOLE_GLOBE
import io.alvaroedo.hfpredict.core.MapDetail
import io.alvaroedo.hfpredict.core.MapView
import io.alvaroedo.hfpredict.core.bandLabel
import io.alvaroedo.hfpredict.core.qualityOf
import io.alvaroedo.hfpredict.core.viewPanned
import io.alvaroedo.hfpredict.core.viewZoomed
import io.alvaroedo.hfpredict.label
import io.alvaroedo.hfpredict.tr
import kotlin.math.roundToInt

@Composable
fun ReachCard(state: UiState, vm: AppViewModel, fmt: Format, modifier: Modifier = Modifier) {
    val ctx = LocalResources.current
    val band = bandLabel(state.band)
    SectionCard(modifier) {
        // The headline: the chance to the destination, or the share of
        // directions reached.
        val cell = state.forecast?.cells?.firstOrNull { it.band == state.band && it.hour.toInt() == state.hour }
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            val headline = when {
                cell == null -> tr(R.string.status_loading)
                state.forecast?.to != null -> tr(
                    R.string.reach_answer, "band" to band, "place" to state.forecast.to!!.name,
                    "hour" to fmt.hour(state.hour), "percent" to fmt.percent(cell.reliability),
                )
                else -> tr(R.string.reach_answerAnywhere, "band" to band, "hour" to fmt.hour(state.hour), "percent" to fmt.percent(cell.reliability))
            }
            Text(headline, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            if (cell != null) QualityPill(qualityOf(cell.reliability))
        }

        MapBox(state, vm, fmt)

        val detail = when {
            state.mapError != null -> tr(R.string.reach_mapUnavailable)
            state.detail == MapDetail.NONE -> tr(R.string.reach_mapLoading)
            state.detail == MapDetail.COARSE -> tr(R.string.reach_detailCoarse)
            state.detail == MapDetail.FINE -> tr(R.string.reach_detailWorld)
            else -> tr(R.string.reach_detailNear)
        }
        Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)

        val colors = LocalMapColors.current
        val reach = state.status?.reach ?: state.reach.firstOrNull { it.band == state.band }?.reach
        val nvis = state.status?.nvisKm
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Swatches(colors.cells.drop(1), tr(R.string.reach_legendReach))
            Swatches(listOf(androidx.compose.ui.graphics.Color.Black.copy(alpha = 0.45f)), tr(R.string.reach_legendNight))
            if (nvis != null) Swatches(listOf(MaterialTheme.colorScheme.onSurface), tr(R.string.reach_legendNvis), round = true)
        }
        if (reach != null) {
            Text(
                if (nvis != null) tr(R.string.reach_reachAndNvis, "band" to band, "percent" to fmt.percent(reach), "distance" to fmt.distance(ctx, nvis))
                else tr(R.string.reach_reachLine, "band" to band, "percent" to fmt.percent(reach)),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        HourTrack(state, vm, fmt)
    }
}

@Composable
private fun MapBox(state: UiState, vm: AppViewModel, fmt: Format) {
    val colors = LocalMapColors.current
    val density = LocalDensity.current.density
    val latest by rememberUpdatedState(state)
    var sidePx by remember { mutableIntStateOf(0) }
    val mapLabel = latestReachLabel(state, fmt)
    LaunchedEffect(sidePx, colors) { if (sidePx > 0) vm.setMapSurface(sidePx, density, colors.palette) }

    Box(
        Modifier
            .fillMaxWidth()
            .widthIn(max = 720.dp)
            .aspectRatio(1f)
            .clip(RoundedCornerShape(20.dp)),
    ) {
        Canvas(
            Modifier
                .matchParentSize()
                .onSizeChanged { sidePx = minOf(it.width, it.height) }
                .semantics { contentDescription = mapLabel }
                // One finger scrolls the page; two move and zoom the map.
                .pointerInput(Unit) {
                    awaitEachGesture {
                        awaitFirstDown(requireUnconsumed = false)
                        var view = latest.view
                        var moved = false
                        do {
                            val event = awaitPointerEvent()
                            if (event.changes.count { it.pressed } >= 2) {
                                val s = size.width.toDouble()
                                val c = event.calculateCentroid()
                                val pan = event.calculatePan()
                                view = viewZoomed(view, event.calculateZoom().toDouble(), c.x.toDouble(), c.y.toDouble(), s)
                                view = viewPanned(view, pan.x.toDouble(), pan.y.toDouble(), s)
                                vm.setView(view, settled = false)
                                event.changes.forEach { it.consume() }
                                moved = true
                            }
                        } while (event.changes.any { it.pressed })
                        if (moved) vm.setView(view, settled = true)
                    }
                }
                .pointerInput(Unit) {
                    detectTapGestures(
                        onTap = { vm.probe(it.x.toDouble(), it.y.toDouble(), size.width.toDouble()) },
                        onDoubleTap = {
                            val s = size.width.toDouble()
                            vm.setView(viewZoomed(latest.view, 2.0, it.x.toDouble(), it.y.toDouble(), s), settled = true)
                        },
                    )
                },
        ) {
            val rendered = state.rendered ?: return@Canvas
            val side = size.minDimension
            val at = rendered.view
            val now = state.view
            // Move and scale the last image from the view it was drawn for
            // to the view on screen, so gestures follow the fingers at once.
            val k = (now.scale / at.scale).toFloat()
            fun t(cAt: Double, cNow: Double) = ((cAt - cNow - 0.5 / at.scale + 0.5 / now.scale) * now.scale).toFloat() * side
            translate(t(at.cx, now.cx), t(at.cy, now.cy)) {
                withTransform({ scale(k, k, pivot = Offset.Zero) }) {
                    drawImage(rendered.image, dstSize = IntSize(side.roundToInt(), side.roundToInt()))
                }
            }
        }

        Column(
            Modifier.align(Alignment.TopEnd).padding(8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            val zoom = { k: Double ->
                val s = sidePx.toDouble()
                vm.setView(viewZoomed(state.view, k, s / 2, s / 2, s), settled = true)
            }
            MapButton(R.drawable.ic_add, tr(R.string.map_zoomIn)) { zoom(1.6) }
            MapButton(R.drawable.ic_remove, tr(R.string.map_zoomOut)) { zoom(1 / 1.6) }
            MapButton(R.drawable.ic_fit_screen, tr(R.string.map_fitReach)) { vm.fitReach() }
            MapButton(R.drawable.ic_globe, tr(R.string.map_wholeGlobe)) { vm.setView(WHOLE_GLOBE, settled = true) }
        }
        if (state.mapWorking) {
            CircularProgressIndicator(Modifier.align(Alignment.BottomEnd).padding(14.dp).size(22.dp), strokeWidth = 2.5.dp)
        }
        state.probe?.let { p ->
            val ctx = LocalResources.current
            val text = buildString {
                append(p.grid).append(" · ").append(fmt.distance(ctx, p.distanceKm)).append(" · ").append(fmt.degrees(p.bearingDeg)).append("°")
                p.quality?.let { append(" · ").append(ctx.getString(it.label())) }
            }
            Surface(
                Modifier.align(Alignment.BottomStart).padding(10.dp),
                shape = RoundedCornerShape(10.dp),
                color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.92f),
            ) { Text(text, Modifier.padding(horizontal = 10.dp, vertical = 5.dp), style = MaterialTheme.typography.labelMedium) }
        }
    }
}

@Composable
private fun MapButton(icon: Int, label: String, onClick: () -> Unit) {
    FilledTonalIconButton(onClick = onClick, modifier = Modifier.size(40.dp)) {
        Icon(painterResource(icon), contentDescription = label)
    }
}

/**
 * The hour slider: six hours back to eighteen ahead, local time at the
 * station above (solar, an hour per 15 degrees of longitude) and UTC below.
 */
@Composable
private fun HourTrack(state: UiState, vm: AppViewModel, fmt: Format) {
    val start = state.trackStart
    val from = state.saved.from
    val offset = from?.let { (it.lon / 15).roundToInt() } ?: 0
    val position = Math.floorMod(state.hour - start, 24)
    var dragging by remember { mutableFloatStateOf(-1f) }
    val shown = if (dragging >= 0) dragging else position.toFloat()

    val hourLabel = tr(R.string.time_hourSlider)
    Column(Modifier.padding(top = 4.dp)) {
        Kicker(tr(R.string.time_localAt, "place" to (from?.name ?: "Greenwich")))
        Labels(start, offset, top = true, fmt = fmt)
        Slider(
            value = shown,
            onValueChange = {
                dragging = it
                val hour = (start + it.roundToInt()) % 24
                if (hour != state.hour) vm.select(state.band, hour)
            },
            onValueChangeFinished = { dragging = -1f },
            valueRange = 0f..23f,
            steps = 22,
            modifier = Modifier.semantics {
                contentDescription = hourLabel
                stateDescription = fmt.hour(state.hour) + " UTC"
            },
        )
        Labels(start, 0, top = false, fmt = fmt)
        Kicker(tr(R.string.time_utcAt))
    }
}

@Composable
private fun Labels(start: Int, offset: Int, top: Boolean, fmt: Format) {
    val now = MaterialTheme.colorScheme.tertiary
    BoxWithConstraints(Modifier.fillMaxWidth().height(18.dp)) {
        // The slider's track runs inside a thumb's width of each edge.
        val inset = 10.dp
        val track = maxWidth - inset * 2
        for (i in 0 until 24) {
            val labelled = i % 4 == PAST_HOURS % 4
            if (!labelled) continue
            val text = if (top && i == PAST_HOURS) tr(R.string.time_now)
            else fmt.hour((start + i + offset) % 24)
            val x = inset + track * (i / 23f)
            Text(
                text,
                Modifier.offset { IntOffset((x - 18.dp).roundToPx(), 0) }.widthIn(min = 36.dp),
                style = MaterialTheme.typography.labelSmall,
                color = if (top && i == PAST_HOURS) now else MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
private fun latestReachLabel(state: UiState, fmt: Format): String {
    val reach = state.status?.reach ?: state.reach.firstOrNull { it.band == state.band }?.reach
    return if (reach == null) tr(R.string.reach_mapLoading)
    else tr(R.string.map_description, "band" to bandLabel(state.band), "percent" to fmt.percent(reach))
}
