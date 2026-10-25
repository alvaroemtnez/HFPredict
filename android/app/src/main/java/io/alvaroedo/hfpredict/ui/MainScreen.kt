package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.AntennaKind
import io.alvaroedo.hfpredict.core.antennaLobes
import io.alvaroedo.hfpredict.core.antennaUsesBeam
import io.alvaroedo.hfpredict.core.bandLabel
import io.alvaroedo.hfpredict.core.bandsDisplayOrder
import io.alvaroedo.hfpredict.core.qualityOf
import io.alvaroedo.hfpredict.core.requiredSnrDb
import io.alvaroedo.hfpredict.label
import io.alvaroedo.hfpredict.tr

/** Screens and dialogs over the main one. */
sealed interface Overlay {
    data object None : Overlay
    data class Path(val to: Boolean) : Overlay
    data object Station : Overlay
    data object Preferences : Overlay
    data object Help : Overlay
    data object About : Overlay
    data object Map : Overlay
    /** Point the antenna at a bearing (true, short path), or just read the heading. */
    data class Compass(val bearing: Double?, val name: String?) : Overlay
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(state: UiState, vm: AppViewModel, fmt: Format, open: (Overlay) -> Unit) {
    val scroll = TopAppBarDefaults.pinnedScrollBehavior()
    val context = LocalContext.current
    val hasCompass = remember { compassSensor(context) != null }
    val point: ((Double, String) -> Unit)? = if (hasCompass) { b, name -> open(Overlay.Compass(b, name)) } else null
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            Column {
                TopAppBar(
                    title = { PathTitle(state, fmt) { open(Overlay.Path(to = false)) } },
                    actions = {
                        if (hasCompass) {
                            val to = state.saved.to
                            IconButton(onClick = { open(Overlay.Compass(state.forecast?.takeIf { it.to != null }?.bearingDeg, to?.name)) }) {
                                Icon(painterResource(R.drawable.ic_explore), contentDescription = tr(R.string.compass_open))
                            }
                        }
                        StatusChip(state, fmt)
                        Menu(open, refresh = { vm.refreshReadings(force = true) })
                    },
                    scrollBehavior = scroll,
                    colors = TopAppBarDefaults.topAppBarColors(scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
                )
                StationStrip(state, fmt) { open(Overlay.Station) }
                BandChips(state, vm)
            }
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = state.refreshing,
            onRefresh = { vm.refreshReadings() },
            modifier = Modifier.padding(padding).fillMaxSize(),
        ) {
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val wide = maxWidth >= 840.dp
                Column(
                    Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 12.dp, vertical = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    val content = Modifier.widthIn(max = 1500.dp).fillMaxWidth()
                    if (wide) {
                        Row(content, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            ReachCard(state, vm, fmt, Modifier.weight(1f), onFullscreen = { open(Overlay.Map) }, onPoint = point)
                            BandsCard(state, vm, fmt, Modifier.weight(1f))
                        }
                    } else {
                        ReachCard(state, vm, fmt, content, onFullscreen = { open(Overlay.Map) }, onPoint = point)
                        BandsCard(state, vm, fmt, content)
                    }
                    SunCard(state, fmt, content)
                    BasisCard(state, fmt, content)
                }
            }
        }
    }
}

@Composable
private fun PathTitle(state: UiState, fmt: Format, onClick: () -> Unit) {
    val ctx = LocalResources.current
    val from = state.saved.from?.name ?: "Greenwich"
    val to = state.saved.to
    val forecast = state.forecast?.takeIf { it.to != null }
    val label = tr(R.string.a11y_changeLocation, "place" to from)
    Column(
        Modifier
            .clickable(onClick = onClick)
            .semantics { contentDescription = label }
            .padding(vertical = 4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (to != null) "$from → ${to.name}" else from,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Icon(painterResource(R.drawable.ic_expand_more), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        }
        Text(
            if (forecast != null) {
                val hops = forecast.hops?.toInt() ?: 1
                val args = arrayOf(
                    "distance" to fmt.distance(ctx, forecast.distanceKm ?: 0.0),
                    "bearing" to fmt.degrees(forecast.bearingDeg ?: 0.0),
                    "hops" to hops,
                )
                tr(if (hops == 1) R.string.path_summaryOneHop else R.string.path_summary, *args)
            } else tr(R.string.path_everyDirection),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Live or offline; a tap says what that means. */
@Composable
private fun StatusChip(state: UiState, fmt: Format) {
    val colors = LocalMapColors.current
    val color = if (state.live) colors.live else colors.offline
    val hint = state.saved.spaceWeather?.takeIf { state.live }
        ?.let { tr(R.string.status_liveHint, "time" to fmt.hm(it.observedAt)) }
        ?: tr(R.string.status_offlineHint)
    var open by remember { mutableStateOf(false) }
    val label = tr(if (state.live) R.string.status_live else R.string.status_offline)
    Surface(
        onClick = { open = true },
        shape = MaterialTheme.shapes.small,
        color = color.copy(alpha = 0.14f),
        contentColor = color,
        border = BorderStroke(1.dp, color.copy(alpha = 0.6f)),
        modifier = Modifier.semantics { contentDescription = hint },
    ) {
        Text(
            label.uppercase(),
            Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
        )
    }
    if (open) {
        AlertDialog(
            onDismissRequest = { open = false },
            confirmButton = { TextButton(onClick = { open = false }) { Text(tr(R.string.common_close)) } },
            title = { Text(label) },
            text = { Text(hint + "\n\n" + tr(R.string.status_liveExplain)) },
        )
    }
}

@Composable
private fun Menu(open: (Overlay) -> Unit, refresh: () -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = tr(R.string.menu_open))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text(tr(R.string.menu_refresh)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_refresh), null) },
                onClick = { expanded = false; refresh() },
            )
            DropdownMenuItem(
                text = { Text(tr(R.string.menu_preferences)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_settings), null) },
                onClick = { expanded = false; open(Overlay.Preferences) },
            )
            DropdownMenuItem(
                text = { Text(tr(R.string.menu_help)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_help), null) },
                onClick = { expanded = false; open(Overlay.Help) },
            )
            DropdownMenuItem(
                text = { Text(tr(R.string.menu_about)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_info), null) },
                onClick = { expanded = false; open(Overlay.About) },
            )
        }
    }
}

/** The station as chips; each opens the editor. */
@Composable
private fun StationStrip(state: UiState, fmt: Format, onClick: () -> Unit) {
    val ctx = LocalResources.current
    val saved = state.saved
    val st = saved.stations.firstOrNull { it.id == saved.activeStation } ?: saved.stations.first()
    val chips = buildList {
        if (saved.stations.size > 1 || st.name.isNotEmpty()) {
            add(R.drawable.ic_radio to st.name.ifEmpty { ctx.getString(R.string.station_unnamed) })
        }
        add(R.drawable.ic_cell_tower to ctx.getString(st.mode.label()))
        add(R.drawable.ic_bolt to fmt.watts(ctx, st.watts))
        val antenna = ctx.getString(st.antenna.kind.label())
        add(
            R.drawable.ic_settings_input_antenna to
                if (st.antenna.kind == AntennaKind.ISOTROPIC) antenna else "$antenna · ${fmt.height(ctx, st.antenna.heightM)}",
        )
        if (antennaUsesBeam(st.antenna.kind)) {
            add(R.drawable.ic_explore to antennaLobes(st.antenna.beamDeg, st.antenna.kind).joinToString("/") { fmt.degrees(it) } + "°")
        }
    }
    val hint = tr(R.string.station_strip_mode, "mode" to ctx.getString(st.mode.label()), "db" to requiredSnrDb(st.mode))
    LazyRow(
        contentPadding = PaddingValues(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.semantics { contentDescription = hint },
    ) {
        items(chips) { (icon, text) ->
            AssistChip(
                onClick = onClick,
                label = { Text(text) },
                leadingIcon = { Icon(painterResource(icon), null, Modifier.size(AssistChipDefaults.IconSize)) },
            )
        }
    }
}

/** The bands, highest first; each carries a dot in this hour's colour. */
@Composable
fun BandChips(state: UiState, vm: AppViewModel) {
    val colors = LocalMapColors.current
    val outline = MaterialTheme.colorScheme.outline
    LazyRow(
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(bandsDisplayOrder()) { band ->
            val r = state.forecast?.cells?.firstOrNull { it.band == band && it.hour.toInt() == state.hour }?.reliability
            FilterChip(
                selected = band == state.band,
                onClick = { vm.select(band, state.hour) },
                label = { Text(bandLabel(band), fontWeight = FontWeight.Bold) },
                leadingIcon = {
                    Canvas(Modifier.size(8.dp)) {
                        if (r != null) drawCircle(colors.cell(qualityOf(r)))
                        drawCircle(outline, style = Stroke(1.dp.toPx()))
                    }
                },
            )
        }
    }
}
