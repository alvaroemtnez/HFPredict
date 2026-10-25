package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.AntennaKind
import io.alvaroedo.hfpredict.core.Station
import io.alvaroedo.hfpredict.core.antennaAskedAsWire
import io.alvaroedo.hfpredict.core.antennaIsBidirectional
import io.alvaroedo.hfpredict.core.antennaLobes
import io.alvaroedo.hfpredict.core.antennaUsesBeam
import io.alvaroedo.hfpredict.core.antennaUsesGain
import io.alvaroedo.hfpredict.core.antennaUsesHeight
import io.alvaroedo.hfpredict.core.antennasInOrder
import io.alvaroedo.hfpredict.core.bearingBetween
import io.alvaroedo.hfpredict.core.beamFromWire
import io.alvaroedo.hfpredict.core.effectiveHeightM
import io.alvaroedo.hfpredict.core.modesInOrder
import io.alvaroedo.hfpredict.core.nearestLobe
import io.alvaroedo.hfpredict.core.requiredSnrDb
import io.alvaroedo.hfpredict.core.separationDeg
import io.alvaroedo.hfpredict.core.wireFromBeam
import io.alvaroedo.hfpredict.label
import io.alvaroedo.hfpredict.tr
import kotlin.math.cos
import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin

private const val FOOT = 0.3048

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun StationScreen(state: UiState, vm: AppViewModel, fmt: Format, compact: Boolean, onDismiss: () -> Unit) {
    val ctx = LocalResources.current
    val saved = state.saved
    val active = saved.stations.firstOrNull { it.id == saved.activeStation } ?: saved.stations.first()
    var original by remember(active.id) { mutableStateOf(active) }
    var draft by remember(active.id) { mutableStateOf(active) }
    var confirm by remember { mutableStateOf(false) }
    val dirty = draft != original
    val close = { if (dirty) confirm = true else onDismiss() }

    FullScreenDialog(
        tr(R.string.station_title), close, compact,
        actions = {
            TextButton(onClick = { if (dirty) vm.putStation(draft); onDismiss() }) { Text(tr(R.string.common_save)) }
        },
    ) { mod ->
        Column(
            mod.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            // Which station, and its name.
            var picking by remember { mutableStateOf(false) }
            ExposedDropdownMenuBox(expanded = picking, onExpandedChange = { picking = it }) {
                OutlinedTextField(
                    value = draft.name.ifEmpty { tr(R.string.station_unnamed) },
                    onValueChange = {},
                    readOnly = true,
                    label = { Text(tr(R.string.station_pickSection)) },
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
                    modifier = Modifier.menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable).fillMaxWidth(),
                )
                ExposedDropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                    saved.stations.forEach { s ->
                        DropdownMenuItem(
                            text = { Text(s.name.ifEmpty { ctx.getString(R.string.station_unnamed) }) },
                            onClick = {
                                picking = false
                                if (dirty) vm.putStation(draft)
                                vm.selectStation(s.id)
                            },
                        )
                    }
                }
            }
            OutlinedTextField(
                value = draft.name,
                onValueChange = { draft = draft.copy(name = it.take(24)) },
                label = { Text(tr(R.string.station_nameSection)) },
                placeholder = { Text(tr(R.string.station_namePlaceholder)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton(onClick = {
                    if (dirty) vm.putStation(draft)
                    vm.putStation(vm.core.newStation())
                }) {
                    Icon(painterResource(R.drawable.ic_add), null, Modifier.size(18.dp))
                    Text(" " + tr(R.string.station_add))
                }
                if (saved.stations.size > 1) {
                    OutlinedButton(onClick = { vm.deleteStation(draft.id) }) {
                        Icon(painterResource(R.drawable.ic_delete), null, Modifier.size(18.dp))
                        Text(" " + tr(R.string.common_delete))
                    }
                }
            }
            HorizontalDivider()

            Section(tr(R.string.station_modeSection), tr(R.string.station_modeHint))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                modesInOrder().forEach { m ->
                    FilterChip(selected = draft.mode == m, onClick = { draft = draft.copy(mode = m) }, label = { Text(tr(m.label())) })
                }
            }
            Text(
                tr(R.string.station_modeNeeds, "mode" to ctx.getString(draft.mode.label()), "db" to requiredSnrDb(draft.mode)),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            HorizontalDivider()

            Section(tr(R.string.station_powerSection), tr(R.string.station_powerRange, "min" to fmt.dec(0.1, 1), "max" to fmt.int(1500.0)))
            NumberField(fmt.watts(ctx, draft.watts), draft.watts, decimals = true) {
                draft = draft.copy(watts = it.coerceIn(0.1, 1500.0))
            }
            // Log scale: QRP to legal limit on one slider.
            Slider(
                value = log10(draft.watts).toFloat(),
                onValueChange = { draft = draft.copy(watts = roundWatts(10.0.pow(it.toDouble()))) },
                valueRange = -1f..log10(1500f),
            )
            HorizontalDivider()

            Section(tr(R.string.station_antennaSection), null)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                antennasInOrder().forEach { k ->
                    FilterChip(
                        selected = draft.antenna.kind == k,
                        onClick = { draft = draft.copy(antenna = draft.antenna.copy(kind = k)) },
                        label = { Text(tr(k.label())) },
                    )
                }
            }
            val kind = draft.antenna.kind
            Text(
                when (kind) {
                    AntennaKind.ISOTROPIC -> tr(R.string.station_isotropicNote)
                    AntennaKind.INVERTED_V -> tr(
                        R.string.station_invertedVNote,
                        "height" to fmt.height(ctx, effectiveHeightM(draft.antenna)), "percent" to 80,
                    )
                    AntennaKind.VERTICAL -> tr(R.string.station_verticalNote)
                    else -> tr(R.string.station_heightNote)
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (antennaUsesHeight(kind)) {
                val unit = if (fmt.imperial) FOOT else 1.0
                Text(
                    tr(if (kind == AntennaKind.INVERTED_V) R.string.station_apexHeight else R.string.station_height),
                    style = MaterialTheme.typography.titleSmall,
                )
                NumberField(fmt.height(ctx, draft.antenna.heightM), draft.antenna.heightM / unit, decimals = false) {
                    draft = draft.copy(antenna = draft.antenna.copy(heightM = (it * unit).coerceIn(1.0, 100.0)))
                }
                Slider(
                    value = draft.antenna.heightM.toFloat(),
                    onValueChange = { draft = draft.copy(antenna = draft.antenna.copy(heightM = (it / unit).roundToInt() * unit)) },
                    valueRange = 1f..100f,
                )
            }
            if (antennaUsesGain(kind)) {
                Text(tr(R.string.station_gain) + " · " + tr(R.string.units_decibelsDbd, "value" to fmt.dec(draft.antenna.gainDbd, 1)), style = MaterialTheme.typography.titleSmall)
                Slider(
                    value = draft.antenna.gainDbd.toFloat(),
                    onValueChange = { draft = draft.copy(antenna = draft.antenna.copy(gainDbd = (it * 2).roundToInt() / 2.0)) },
                    valueRange = 0f..20f,
                )
            }
            if (antennaUsesBeam(kind)) {
                HorizontalDivider()
                val wire = antennaAskedAsWire(kind)
                val shown = if (wire) wireFromBeam(draft.antenna.beamDeg) else draft.antenna.beamDeg
                Section(
                    tr(if (wire) R.string.station_wireRuns else R.string.station_beam) + " · " + fmt.degrees(shown) + "°",
                    null,
                )
                Slider(
                    value = shown.toFloat(),
                    onValueChange = {
                        val v = (it / 5).roundToInt() * 5.0
                        draft = draft.copy(antenna = draft.antenna.copy(beamDeg = if (wire) beamFromWire(v) else v))
                    },
                    valueRange = 0f..355f,
                )
                val context = LocalContext.current
                if (remember { compassSensor(context) != null }) {
                    var measuring by remember { mutableStateOf(false) }
                    OutlinedButton(onClick = { measuring = true }) {
                        Icon(painterResource(R.drawable.ic_explore), null, Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text(tr(R.string.compass_measure))
                    }
                    if (measuring) {
                        val here = saved.from ?: io.alvaroedo.hfpredict.core.greenwich()
                        CompassScreen(
                            CompassGoal.Measure(draft) { v ->
                                draft = draft.copy(antenna = draft.antenna.copy(beamDeg = if (wire) beamFromWire(v) else v))
                                measuring = false
                            },
                            draft, here.lat, here.lon, fmt, compact,
                        ) { measuring = false }
                    }
                }
                val from = saved.from
                val to = saved.to
                val target = if (from != null && to != null) bearingBetween(from.lat, from.lon, to.lat, to.lon) else null
                Compass(draft, target, Modifier.align(Alignment.CenterHorizontally))
                val lobes = antennaLobes(draft.antenna.beamDeg, kind)
                Text(
                    if (lobes.size == 2) tr(R.string.station_favoursTwo, "first" to fmt.degrees(lobes[0]), "second" to fmt.degrees(lobes[1]))
                    else tr(R.string.station_favoursOne, "first" to fmt.degrees(lobes[0])),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (target != null && to != null) {
                    val lobe = nearestLobe(draft.antenna.beamDeg, kind, target)
                    val off = separationDeg(lobe, target)
                    val key = when {
                        off <= 30 -> R.string.station_aim_best
                        off >= 60 && antennaIsBidirectional(kind) -> R.string.station_aim_nearNull
                        else -> R.string.station_aim_offToOneSide
                    }
                    Text(
                        tr(key, "place" to to.name, "degrees" to fmt.degrees(target), "lobe" to fmt.degrees(lobe), "offset" to fmt.degrees(off)),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    FilledTonalButton(onClick = {
                        draft = draft.copy(antenna = draft.antenna.copy(beamDeg = (target.roundToInt() % 360).toDouble()))
                    }) {
                        Text(tr(R.string.station_alignFor, "place" to to.name, "degrees" to fmt.degrees(target)))
                    }
                }
            }
        }
    }

    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text(tr(R.string.station_discardTitle)) },
            text = { Text(tr(R.string.station_discardBody)) },
            confirmButton = { TextButton(onClick = { confirm = false; onDismiss() }) { Text(tr(R.string.station_discard)) } },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text(tr(R.string.station_keepEditing)) } },
        )
    }
}

/** Steps that make sense on a log slider: tenths below 1 W, then whole watts. */
private fun roundWatts(w: Double): Double = when {
    w < 1 -> (w * 10).roundToInt() / 10.0
    w < 20 -> w.roundToInt().toDouble()
    w < 200 -> (w / 5).roundToInt() * 5.0
    else -> (w / 50).roundToInt() * 50.0
}

@Composable
private fun Section(title: String, hint: String?) {
    Column {
        Text(title, style = MaterialTheme.typography.titleMedium)
        if (hint != null) Text(hint, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** A number typed in, committed when it parses. */
@Composable
private fun NumberField(label: String, value: Double, decimals: Boolean, onValue: (Double) -> Unit) {
    var text by remember(value) { mutableStateOf(if (decimals && value % 1 != 0.0) "%.1f".format(value) else value.roundToInt().toString()) }
    OutlinedTextField(
        value = text,
        onValueChange = { t ->
            text = t
            t.replace(',', '.').toDoubleOrNull()?.let(onValue)
        },
        suffix = { Text(label.replace(Regex("[0-9.,\\s]"), "")) },
        keyboardOptions = KeyboardOptions(keyboardType = if (decimals) KeyboardType.Decimal else KeyboardType.Number),
        singleLine = true,
        modifier = Modifier.widthIn(max = 240.dp).fillMaxWidth(),
    )
}

/** The lobes as wedges round a compass; the wire drawn through the middle. */
@Composable
private fun Compass(station: Station, target: Double?, modifier: Modifier = Modifier) {
    val colors = LocalMapColors.current
    val scheme = MaterialTheme.colorScheme
    val measurer = rememberTextMeasurer()
    val letters = listOf(tr(R.string.station_compass_n), tr(R.string.station_compass_e), tr(R.string.station_compass_s), tr(R.string.station_compass_w))
    val kind = station.antenna.kind
    val lobes = antennaLobes(station.antenna.beamDeg, kind)
    Canvas(modifier.size(220.dp)) {
        val c = Offset(size.width / 2, size.height / 2)
        val r = size.minDimension / 2 - 22.dp.toPx()
        fun at(deg: Double, rad: Float) = Offset(
            c.x + rad * sin(Math.toRadians(deg)).toFloat(),
            c.y - rad * cos(Math.toRadians(deg)).toFloat(),
        )
        drawCircle(scheme.outlineVariant, r, c, style = Stroke(1.5.dp.toPx()))
        letters.forEachIndexed { i, l ->
            val t = measurer.measure(l, TextStyle(fontSize = 13.sp, color = scheme.onSurfaceVariant))
            val p = at(i * 90.0, r + 13.dp.toPx())
            drawText(t, topLeft = Offset(p.x - t.size.width / 2, p.y - t.size.height / 2))
        }
        for (lobe in lobes) {
            for (k in -12..12) {
                val spread = k * 2.5
                val reach = r * (1 - (kotlin.math.abs(spread) / 40.0).pow(2)).toFloat() * 0.92f
                drawLine(colors.cells[2].copy(alpha = 0.35f), c, at(lobe + spread, reach), 2.dp.toPx())
            }
            drawLine(colors.cells[3], c, at(lobe, r * 0.92f), 3.dp.toPx())
        }
        if (antennaAskedAsWire(kind)) {
            val run = wireFromBeam(station.antenna.beamDeg)
            drawLine(scheme.onSurface, at(run, r * 0.75f), at(run + 180, r * 0.75f), 4.dp.toPx())
        }
        if (target != null) drawCircle(colors.now, 6.dp.toPx(), at(target, r))
        drawCircle(scheme.onSurface, 4.dp.toPx(), c)
    }
}
