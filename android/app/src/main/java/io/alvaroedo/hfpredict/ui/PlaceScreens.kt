package io.alvaroedo.hfpredict.ui

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.DeviceLocation
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.Place
import io.alvaroedo.hfpredict.core.greenwich
import io.alvaroedo.hfpredict.tr
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Search as you type: built-in places and typed coordinates at once,
 *  online after a pause when those find little. */
@Composable
private fun PlaceSearch(vm: AppViewModel, network: Boolean, query: String, onPick: (Place) -> Unit) {
    val language = LocalContext.current.resources.configuration.locales[0].language
    var results by remember { mutableStateOf<List<Place>>(emptyList()) }
    var status by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(query) {
        status = null
        if (query.isBlank()) {
            results = emptyList()
            return@LaunchedEffect
        }
        val local = withContext(Dispatchers.Default) { runCatching { vm.core.searchPlaces(query, language, false) }.getOrDefault(emptyList()) }
        results = local
        if (local.size >= 3 || !network || query.trim().length < 3) {
            if (local.isEmpty()) status = R.string.location_noResults
            return@LaunchedEffect
        }
        delay(600)
        status = R.string.location_searching
        val remote = withContext(Dispatchers.IO) { runCatching { vm.core.searchPlaces(query, language, true) } }
        remote.fold(
            { results = it; status = if (it.isEmpty()) R.string.location_noResults else null },
            { status = R.string.location_searchFailed },
        )
    }
    status?.let { Text(tr(it), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 8.dp)) }
    results.forEach { PlaceRow(it, onPick) }
}

@Composable
private fun PlaceRow(place: Place, onPick: (Place) -> Unit) {
    ListItem(
        headlineContent = { Text(place.name) },
        supportingContent = { Text(if (place.detail.isEmpty()) place.grid else "${place.detail} · ${place.grid}") },
        modifier = Modifier.fillMaxWidth().clickableRow { onPick(place) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    )
}

private fun Modifier.clickableRow(onClick: () -> Unit) = clickable(onClick = onClick)

/** Asks for coarse location when needed, then reads the device's position. */
@Composable
private fun rememberLocate(onFound: (Double, Double) -> Unit, onFail: (Int) -> Unit): () -> Unit {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val run = {
        scope.launch {
            when (val r = DeviceLocation.locate(ctx)) {
                is DeviceLocation.Result.Found -> onFound(r.lat, r.lon)
                DeviceLocation.Result.NoPermission -> onFail(R.string.location_permissionDenied)
                DeviceLocation.Result.Disabled -> onFail(R.string.location_noProvider)
                DeviceLocation.Result.Unavailable -> onFail(R.string.location_unavailable)
            }
        }
        Unit
    }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) run() else onFail(R.string.location_permissionDenied)
    }
    return {
        if (DeviceLocation.hasPermission(ctx)) run() else launcher.launch(Manifest.permission.ACCESS_COARSE_LOCATION)
    }
}

@Composable
fun PathScreen(state: UiState, vm: AppViewModel, startOnTo: Boolean, compact: Boolean, onDismiss: () -> Unit) {
    var to by remember { mutableStateOf(startOnTo) }
    var query by remember { mutableStateOf("") }
    var locating by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<Int?>(null) }
    val pick = { p: Place ->
        if (to) vm.setTo(p) else vm.setFrom(p)
        onDismiss()
    }
    val locate = rememberLocate(
        onFound = { lat, lon ->
            locating = false
            pick(vm.core.placeAt(lat, lon))
        },
        onFail = { locating = false; problem = it },
    )
    FullScreenDialog(tr(R.string.location_title), onDismiss, compact) { mod ->
        Column(
            mod.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                SingleChoiceSegmentedButtonRow(Modifier.weight(1f)) {
                    SegmentedButton(!to, { to = false }, SegmentedButtonDefaults.itemShape(0, 2)) { Text(tr(R.string.location_from)) }
                    SegmentedButton(to, { to = true }, SegmentedButtonDefaults.itemShape(1, 2)) { Text(tr(R.string.location_to)) }
                }
                IconButton(onClick = { vm.swapEnds(); onDismiss() }, enabled = state.saved.to != null) {
                    Icon(painterResource(R.drawable.ic_swap_vert), contentDescription = tr(R.string.location_swap))
                }
            }
            val current = if (to) state.saved.to else state.saved.from
            Text(
                current?.let { "${it.name} · ${it.grid}" } ?: tr(R.string.path_everyDirection),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                placeholder = { Text(tr(R.string.location_searchPlaceholder)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            if (!to) {
                TextButton(onClick = { locating = true; problem = null; locate() }, enabled = !locating) {
                    if (locating) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    else Icon(painterResource(R.drawable.ic_my_location), null)
                    Spacer(Modifier.size(8.dp))
                    Text(tr(if (locating) R.string.location_locating else R.string.location_useDevice))
                }
            } else if (state.saved.to != null) {
                ListItem(
                    headlineContent = { Text(tr(R.string.location_everyDirection)) },
                    supportingContent = { Text(tr(R.string.location_everyDirectionHint)) },
                    leadingContent = { Icon(painterResource(R.drawable.ic_travel_explore), null) },
                    modifier = Modifier.clickableRow { vm.setTo(null); onDismiss() },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                )
            }
            problem?.let { Text(tr(it), color = MaterialTheme.colorScheme.error) }
            PlaceSearch(vm, state.saved.settings.network, query, pick)
            if (query.isBlank() && state.saved.recent.isNotEmpty()) {
                Kicker(tr(R.string.location_recent))
                state.saved.recent.forEach { PlaceRow(it, pick) }
            }
        }
    }
}

@Composable
fun FirstRunScreen(state: UiState, vm: AppViewModel) {
    var query by remember { mutableStateOf("") }
    var locating by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<Int?>(null) }
    val finish = { p: Place ->
        vm.setFrom(p)
        vm.refreshReadings(force = true)
    }
    val locate = rememberLocate(
        onFound = { lat, lon -> locating = false; finish(vm.core.placeAt(lat, lon)) },
        onFail = { locating = false; problem = it },
    )
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .imePadding()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Spacer(Modifier.height(32.dp))
        Icon(
            painterResource(R.drawable.ic_launcher_foreground),
            contentDescription = null,
            tint = androidx.compose.ui.graphics.Color.Unspecified,
            modifier = Modifier.size(120.dp),
        )
        Kicker(tr(R.string.firstRun_kicker), color = MaterialTheme.colorScheme.primary)
        Text(tr(R.string.firstRun_title), style = MaterialTheme.typography.headlineMedium, textAlign = TextAlign.Center)
        Text(tr(R.string.firstRun_subtitle), textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(
            onClick = { locating = true; problem = null; locate() },
            enabled = !locating,
            contentPadding = ButtonDefaults.ButtonWithIconContentPadding,
        ) {
            Icon(painterResource(R.drawable.ic_my_location), null, Modifier.size(ButtonDefaults.IconSize))
            Spacer(Modifier.size(ButtonDefaults.IconSpacing))
            Text(tr(if (locating) R.string.location_locating else R.string.firstRun_useGps))
        }
        problem?.let { Text(tr(it), color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center) }
        Kicker(tr(R.string.firstRun_orType), Modifier.padding(top = 12.dp))
        Column(Modifier.widthIn(max = 560.dp).fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                placeholder = { Text(tr(R.string.location_searchPlaceholder)) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                listOf("València", "39.47, -0.38", "IM99sl").forEach { ex ->
                    SuggestionChip(onClick = { query = ex }, label = { Text(ex) })
                }
            }
            Text(
                tr(R.string.firstRun_formats) + "\n" + tr(R.string.firstRun_formatsHint),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
            PlaceSearch(vm, state.saved.settings.network, query, finish)
        }
        Text(
            tr(R.string.firstRun_footnote),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 12.dp),
        )
        TextButton(onClick = { finish(greenwich()) }) {
            Text(tr(R.string.firstRun_skip), fontWeight = FontWeight.Bold)
        }
    }
}
