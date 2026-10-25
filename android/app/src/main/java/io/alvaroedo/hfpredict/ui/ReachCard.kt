package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalViewConfiguration
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
import io.alvaroedo.hfpredict.core.greenwich
import io.alvaroedo.hfpredict.core.localClock
import io.alvaroedo.hfpredict.core.qualityOf
import io.alvaroedo.hfpredict.core.viewPanned
import io.alvaroedo.hfpredict.core.viewZoomed
import io.alvaroedo.hfpredict.label
import io.alvaroedo.hfpredict.tr
import kotlin.math.roundToInt

@Composable
fun ReachCard(
    state: UiState,
    vm: AppViewModel,
    fmt: Format,
    modifier: Modifier = Modifier,
    onFullscreen: () -> Unit = {},
    onPoint: ((Double, String) -> Unit)? = null,
) {
    val ctx = LocalResources.current
    val band = bandLabel(state.band)
    SectionCard(modifier) {
        // The headline: the chance to the destination, or the share of
        // directions reached.
        val cell = state.forecast?.cells?.firstOrNull { it.band == state.band && it.hour.toInt() == state.hour }
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            val headline = when {
                cell == null -> tr(R.string.status_loading)
                state.forecast.to != null -> tr(
                    R.string.reach_answer, "band" to band, "place" to state.forecast.to!!.name,
                    "hour" to fmt.hour(state.hour), "percent" to fmt.percent(cell.reliability),
                )
                else -> tr(R.string.reach_answerAnywhere, "band" to band, "hour" to fmt.hour(state.hour), "percent" to fmt.percent(cell.reliability))
            }
            Text(headline, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            if (cell != null) QualityPill(qualityOf(cell.reliability))
        }

        MapBox(state, vm, fmt, Modifier.fillMaxWidth().widthIn(max = 720.dp), onFullscreen = onFullscreen, onPoint = onPoint)

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

/** Largest square the full-screen map is rendered at; the screen scales it. */
private const val FULL_MAX_PX = 1600

/**
 * The map itself at any shape: the rendered square (the disc's frame) is
 * centred, as wide as the shorter side in the card or the longer side full
 * screen, so the map always fills what it is given. Gestures and taps are
 * read in that square. In the card one finger scrolls the page and two move
 * the map; [full] screen, one finger moves it too.
 */
@Composable
fun MapSurface(state: UiState, vm: AppViewModel, fmt: Format, modifier: Modifier, full: Boolean) {
    val colors = LocalMapColors.current
    val density = LocalDensity.current.density
    val latest by rememberUpdatedState(state)
    var area by remember { mutableStateOf(IntSize.Zero) }
    val side = if (full) maxOf(area.width, area.height) else minOf(area.width, area.height)
    LaunchedEffect(side, colors) {
        if (side > 0) {
            val px = if (full) minOf(side, FULL_MAX_PX) else side
            vm.setMapSurface(px, density * px / side, colors.palette, full)
        }
    }
    if (full) {
        DisposableEffect(Unit) { onDispose { vm.setMapSurface(0, density, colors.palette, full = true) } }
    }
    val fingers = if (full) 1 else 2
    val slop = LocalViewConfiguration.current.touchSlop
    val mapLabel = latestReachLabel(state, fmt)
    // The square's corner on screen.
    val x0 = (area.width - side) / 2f
    val y0 = (area.height - side) / 2f

    Canvas(
        modifier
            .onSizeChanged { area = it }
            .semantics { contentDescription = mapLabel }
            .pointerInput(full) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    var view = latest.view
                    var moved = false
                    var travel = 0f
                    do {
                        val event = awaitPointerEvent()
                        val down = event.changes.count { it.pressed }
                        if (down == 1) travel += event.calculatePan().getDistance()
                        // One finger pans only past the touch slop, so a tap still probes.
                        if (down >= 2 || (down >= fingers && travel > slop)) {
                            val sq = (if (full) maxOf(size.width, size.height) else minOf(size.width, size.height)).toDouble()
                            val ox = (size.width - sq) / 2
                            val oy = (size.height - sq) / 2
                            val c = event.calculateCentroid()
                            val pan = event.calculatePan()
                            view = viewZoomed(view, event.calculateZoom().toDouble(), c.x - ox, c.y - oy, sq)
                            view = viewPanned(view, pan.x.toDouble(), pan.y.toDouble(), sq)
                            vm.setView(view, settled = false)
                            event.changes.forEach { it.consume() }
                            moved = true
                        }
                    } while (event.changes.any { it.pressed })
                    if (moved) vm.setView(view, settled = true)
                }
            }
            .pointerInput(full) {
                fun square(): Triple<Double, Double, Double> {
                    val sq = (if (full) maxOf(size.width, size.height) else minOf(size.width, size.height)).toDouble()
                    return Triple(sq, (size.width - sq) / 2, (size.height - sq) / 2)
                }
                detectTapGestures(
                    onTap = {
                        val (sq, ox, oy) = square()
                        vm.probe(it.x - ox, it.y - oy, sq)
                    },
                    onDoubleTap = {
                        val (sq, ox, oy) = square()
                        vm.setView(viewZoomed(latest.view, 2.0, it.x - ox, it.y - oy, sq), settled = true)
                    },
                )
            },
    ) {
        val rendered = state.rendered ?: return@Canvas
        if (side <= 0) return@Canvas
        val sq = side.toFloat()
        val at = rendered.view
        val now = state.view
        // Move and scale the last image from the view it was drawn for
        // to the view on screen, so gestures follow the fingers at once.
        val k = (now.scale / at.scale).toFloat()
        fun t(cAt: Double, cNow: Double) = ((cAt - cNow - 0.5 / at.scale + 0.5 / now.scale) * now.scale).toFloat() * sq
        translate(x0 + t(at.cx, now.cx), y0 + t(at.cy, now.cy)) {
            withTransform({ scale(k, k, pivot = Offset.Zero) }) {
                drawImage(rendered.image, dstSize = IntSize(side, side))
            }
        }
    }
}

/** Zoom, framing and the full-screen switch, stacked. */
@Composable
private fun MapButtons(vm: AppViewModel, state: UiState, full: Boolean, onFullscreen: () -> Unit, modifier: Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        val zoom = { k: Double -> vm.setView(viewZoomed(state.view, k, 0.5, 0.5, 1.0), settled = true) }
        MapButton(R.drawable.ic_add, tr(R.string.map_zoomIn)) { zoom(1.6) }
        MapButton(R.drawable.ic_remove, tr(R.string.map_zoomOut)) { zoom(1 / 1.6) }
        MapButton(R.drawable.ic_fit_screen, tr(R.string.map_fitReach)) { vm.fitReach() }
        MapButton(R.drawable.ic_globe, tr(R.string.map_wholeGlobe)) { vm.setView(WHOLE_GLOBE, settled = true) }
        if (full) MapButton(R.drawable.ic_fullscreen_exit, tr(R.string.map_exitFullscreen), onFullscreen)
        else MapButton(R.drawable.ic_fullscreen, tr(R.string.map_fullscreen), onFullscreen)
    }
}

/** What is under the finger: locator, distance, bearing, quality, and a way to point there. */
@Composable
private fun ProbeBubble(state: UiState, fmt: Format, onPoint: ((Double, String) -> Unit)?, modifier: Modifier) {
    val p = state.probe ?: return
    val ctx = LocalResources.current
    val text = buildString {
        append(p.grid).append(" · ").append(fmt.distance(ctx, p.distanceKm)).append(" · ").append(fmt.degrees(p.bearingDeg)).append("°")
        p.quality?.let { append(" · ").append(ctx.getString(it.label())) }
    }
    Surface(
        modifier,
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.94f),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(text, Modifier.padding(horizontal = 12.dp, vertical = 7.dp), style = MaterialTheme.typography.labelMedium)
            if (onPoint != null) {
                IconButton(onClick = { onPoint(p.bearingDeg, p.grid) }, Modifier.size(36.dp)) {
                    Icon(painterResource(R.drawable.ic_navigation), contentDescription = tr(R.string.compass_pointHere), Modifier.size(18.dp))
                }
            }
        }
    }
}

/** The map in the card: a rounded square with its buttons. */
@Composable
fun MapBox(
    state: UiState,
    vm: AppViewModel,
    fmt: Format,
    modifier: Modifier = Modifier,
    onFullscreen: () -> Unit,
    onPoint: ((Double, String) -> Unit)? = null,
) {
    Box(modifier.aspectRatio(1f).clip(RoundedCornerShape(20.dp))) {
        MapSurface(state, vm, fmt, Modifier.matchParentSize(), full = false)
        MapButtons(vm, state, full = false, onFullscreen, Modifier.align(Alignment.TopEnd).padding(8.dp))
        if (state.mapWorking) {
            CircularProgressIndicator(Modifier.align(Alignment.BottomEnd).padding(14.dp).size(22.dp), strokeWidth = 2.5.dp)
        }
        ProbeBubble(state, fmt, onPoint, Modifier.align(Alignment.BottomStart).padding(10.dp))
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
 * station above (legal time, from its time zone) and UTC below.
 */
@Composable
fun HourTrack(state: UiState, vm: AppViewModel, fmt: Format) {
    val start = state.trackStart
    val from = state.saved.from
    // Legal local time at the station (summer time and half-hour zones
    // included) for the moment each mark stands for; UTC below.
    val first = state.now - Math.floorMod(state.now, 3600L) - PAST_HOURS * 3600L
    val here = from ?: greenwich()
    val local = remember(here.lat, here.lon, first) {
        List(24) { i -> localClock(here.lat, here.lon, first + i * 3600L) }
    }
    val position = Math.floorMod(state.hour - start, 24)
    var dragging by remember { mutableFloatStateOf(-1f) }
    val shown = if (dragging >= 0) dragging else position.toFloat()

    val hourLabel = tr(R.string.time_hourSlider)
    val haptics = LocalHapticFeedback.current
    Column(Modifier.padding(top = 4.dp)) {
        Kicker(tr(R.string.time_localAt, "place" to (from?.name ?: "Greenwich")))
        Labels(top = true) { i -> local[i] }
        Slider(
            value = shown,
            onValueChange = {
                dragging = it
                val hour = (start + it.roundToInt()) % 24
                if (hour != state.hour) {
                    // A tick per hour crossed, so the hand feels the steps.
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    vm.select(state.band, hour)
                }
            },
            onValueChangeFinished = { dragging = -1f },
            valueRange = 0f..23f,
            steps = 22,
            modifier = Modifier.semantics {
                contentDescription = hourLabel
                stateDescription = fmt.hour(state.hour) + " UTC"
            },
        )
        Labels(top = false) { i -> fmt.hour((start + i) % 24) }
        Kicker(tr(R.string.time_utcAt))
    }
}

@Composable
private fun Labels(top: Boolean, label: (Int) -> String) {
    val now = MaterialTheme.colorScheme.tertiary
    BoxWithConstraints(Modifier.fillMaxWidth().height(18.dp)) {
        // The slider's track runs inside a thumb's width of each edge.
        val inset = 10.dp
        val track = maxWidth - inset * 2
        for (i in 0 until 24) {
            val labelled = i % 4 == PAST_HOURS % 4
            if (!labelled) continue
            val text = if (top && i == PAST_HOURS) tr(R.string.time_now) else label(i)
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

/**
 * The map over the whole screen, edge to edge under the system bars, with
 * everything else floating on it as rounded surfaces: the band and hour on
 * top, the bands below them, the buttons on the side and the hour slider
 * at the bottom.
 */
@Composable
fun FullScreenMap(
    state: UiState,
    vm: AppViewModel,
    fmt: Format,
    onPoint: ((Double, String) -> Unit)?,
    onDismiss: () -> Unit,
) {
    androidx.compose.ui.window.Dialog(
        onDismissRequest = onDismiss,
        properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        val colors = LocalMapColors.current
        val floating = MaterialTheme.colorScheme.surfaceContainer.copy(alpha = 0.92f)
        // Its own window: the system bar icons follow the theme.
        val view = androidx.compose.ui.platform.LocalView.current
        val light = MaterialTheme.colorScheme.surface.luminance() > 0.5f
        androidx.compose.runtime.SideEffect {
            (view.parent as? androidx.compose.ui.window.DialogWindowProvider)?.window?.let { window ->
                androidx.core.view.WindowCompat.getInsetsController(window, view).apply {
                    isAppearanceLightStatusBars = light
                    isAppearanceLightNavigationBars = light
                }
            }
        }
        Box(Modifier.fillMaxSize().background(Color(colors.palette.disc.toInt()))) {
            MapSurface(state, vm, fmt, Modifier.fillMaxSize(), full = true)
            Column(
                Modifier.fillMaxSize().safeDrawingPadding().padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Surface(shape = RoundedCornerShape(28.dp), color = floating) {
                    Row(Modifier.padding(4.dp), verticalAlignment = Alignment.CenterVertically) {
                        IconButton(onClick = onDismiss) {
                            Icon(painterResource(R.drawable.ic_close), contentDescription = tr(R.string.map_exitFullscreen))
                        }
                        Text(
                            bandLabel(state.band) + " · " + fmt.hour(state.hour) + " UTC",
                            Modifier.weight(1f),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        val cell = state.forecast?.cells?.firstOrNull { it.band == state.band && it.hour.toInt() == state.hour }
                        if (cell != null) Box(Modifier.padding(end = 8.dp)) { QualityPill(qualityOf(cell.reliability)) }
                    }
                }
                Surface(shape = RoundedCornerShape(28.dp), color = floating) { BandChips(state, vm) }
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    MapButtons(vm, state, full = true, onDismiss, Modifier.align(Alignment.TopEnd))
                    if (state.mapWorking) {
                        Surface(Modifier.align(Alignment.BottomEnd), shape = RoundedCornerShape(50), color = floating) {
                            CircularProgressIndicator(Modifier.padding(10.dp).size(22.dp), strokeWidth = 2.5.dp)
                        }
                    }
                    ProbeBubble(state, fmt, onPoint, Modifier.align(Alignment.BottomStart))
                }
                Surface(shape = RoundedCornerShape(28.dp), color = floating) {
                    Box(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) { HourTrack(state, vm, fmt) }
                }
            }
        }
    }
}
