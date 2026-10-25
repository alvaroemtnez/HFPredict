package io.alvaroedo.hfpredict.ui

import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.alvaroedo.hfpredict.AppViewModel
import io.alvaroedo.hfpredict.BuildConfig
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.Languages
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.Model
import io.alvaroedo.hfpredict.core.Theme
import io.alvaroedo.hfpredict.core.Units
import io.alvaroedo.hfpredict.core.qualityThresholds
import io.alvaroedo.hfpredict.core.ssnTableRange
import io.alvaroedo.hfpredict.tr

@Composable
private fun Group(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, top = 20.dp, bottom = 4.dp),
    )
}

@Composable
private fun <T> Choice(options: List<Pair<T, String>>, selected: T, hint: (T) -> String? = { null }, onPick: (T) -> Unit) {
    options.forEach { (value, label) ->
        ListItem(
            headlineContent = { Text(label) },
            supportingContent = hint(value)?.let { { Text(it) } },
            leadingContent = { RadioButton(selected = value == selected, onClick = null) },
            modifier = Modifier.selectable(value == selected, role = Role.RadioButton) { onPick(value) },
            colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        )
    }
}

@Composable
fun PreferencesScreen(state: UiState, vm: AppViewModel, compact: Boolean, onDismiss: () -> Unit) {
    val ctx = LocalResources.current
    val context = LocalContext.current
    val s = state.saved.settings
    var stored by remember { mutableLongStateOf(vm.core.storedMapsBytes().toLong()) }
    FullScreenDialog(tr(R.string.preferences_title), onDismiss, compact) { mod ->
        Column(mod.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            Group(tr(R.string.preferences_theme))
            Choice(
                listOf(
                    Theme.SYSTEM to tr(R.string.preferences_theme_system),
                    Theme.LIGHT to tr(R.string.preferences_theme_light),
                    Theme.DARK to tr(R.string.preferences_theme_dark),
                    Theme.LOW_LIGHT to tr(R.string.preferences_theme_lowLight),
                ),
                s.theme,
                hint = { if (it == Theme.LOW_LIGHT) ctx.getString(R.string.preferences_theme_lowLightHint) else null },
            ) { vm.setSettings(s.copy(theme = it)) }

            Group(tr(R.string.preferences_language))
            val current = Languages.current(context)
            Choice(
                listOf(null to tr(R.string.preferences_language_system)) + Languages.ALL.map { it.first to it.second },
                current,
            ) { vm.setSettings(s.copy(language = it)); Languages.set(context, it) }

            Group(tr(R.string.preferences_units))
            Choice(
                listOf(
                    Units.AUTO to tr(R.string.preferences_units_auto),
                    Units.METRIC to tr(R.string.preferences_units_metric),
                    Units.IMPERIAL to tr(R.string.preferences_units_imperial),
                ),
                s.units,
            ) { vm.setSettings(s.copy(units = it)) }

            Group(tr(R.string.preferences_model))
            Choice(
                listOf(Model.TRUECAST to tr(R.string.preferences_model_truecast), Model.VOACAP to tr(R.string.preferences_model_voacap)),
                s.model,
                hint = {
                    ctx.getString(if (it == Model.TRUECAST) R.string.preferences_model_truecastHint else R.string.preferences_model_voacapHint)
                },
            ) { vm.setSettings(s.copy(model = it)) }

            Group(tr(R.string.preferences_privacy))
            SwitchRow(tr(R.string.preferences_network), tr(R.string.preferences_networkHint), s.network) {
                vm.setSettings(s.copy(network = it))
            }

            Group(tr(R.string.preferences_storage))
            SwitchRow(tr(R.string.preferences_keepMaps), tr(R.string.preferences_keepMapsHint), s.keepMaps) {
                vm.setSettings(s.copy(keepMaps = it))
            }
            Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    tr(R.string.preferences_stored, "size" to "%.1f MB".format(stored / 1_048_576.0)),
                    Modifier.weight(1f),
                )
                OutlinedButton(onClick = { vm.core.clearStoredMaps(); stored = vm.core.storedMapsBytes().toLong() }) {
                    Text(tr(R.string.preferences_forget))
                }
            }
        }
    }
}

@Composable
private fun SwitchRow(title: String, hint: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = { Text(hint) },
        trailingContent = { Switch(checked = checked, onCheckedChange = null) },
        modifier = Modifier.selectable(checked, role = Role.Switch) { onChange(!checked) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
    )
}

@Composable
fun HelpScreen(fmt: Format, compact: Boolean, onDismiss: () -> Unit) {
    val (first, last) = ssnTableRange().let { it[0] to it[1] }
    val (reliable, patchy, weak) = qualityThresholds().map { fmt.int(it * 100) }
    val sections = listOf(
        R.string.help_engine_heading to tr(R.string.help_engine_body),
        R.string.help_truecast_heading to tr(R.string.help_truecast_body),
        R.string.help_ssn_heading to tr(R.string.help_ssn_body, "first" to first, "last" to last, "hours" to 24),
        R.string.help_colours_heading to tr(R.string.help_colours_body, "reliable" to reliable, "patchy" to patchy, "weak" to weak),
        R.string.help_map_heading to tr(R.string.help_map_body, "points" to fmt.int(34560.0)),
        R.string.help_nvis_heading to tr(R.string.help_nvis_body, "degrees" to 60),
        R.string.help_invertedV_heading to tr(R.string.help_invertedV_body, "percent" to 80),
        R.string.help_privacy_heading to tr(R.string.help_privacy_body),
    )
    FullScreenDialog(tr(R.string.help_title), onDismiss, compact, maxWidth = 720.dp) { mod ->
        Column(
            mod.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            sections.forEach { (heading, body) ->
                Text(tr(heading), style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 12.dp))
                Text(body, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}

private data class Credit(val who: String, val what: Int, val url: String, val terms: String? = null)

private val BUILT_ON = listOf(
    Credit("Jonathan McSweet", R.string.about_credit_engine, "https://github.com/jonathanmcsweet/hfcast-engine", "Apache-2.0"),
    Credit("NTIA/ITS · Greg Hand", R.string.about_credit_voacap, "https://its.ntia.gov/"),
    Credit("J.A. Watson", R.string.about_credit_voacapl, "https://github.com/jawatson/voacapl", "CC0"),
    Credit("ITU-R Study Group 3", R.string.about_credit_coefficients, "https://github.com/ITU-R-Study-Group-3/ITU-R-HF"),
)

private val DATA = listOf(
    Credit("NTIA/ITS", R.string.about_credit_cities, "https://its.ntia.gov/"),
    Credit("Natural Earth", R.string.about_credit_naturalEarth, "https://www.naturalearthdata.com/"),
    Credit("timezone-boundary-builder (© OpenStreetMap, ODbL)", R.string.about_credit_timeZones, "https://github.com/evansiroky/timezone-boundary-builder"),
    Credit("NOAA SWPC", R.string.about_credit_noaa, "https://www.swpc.noaa.gov/"),
    Credit("GIRO, UMass Lowell", R.string.about_credit_giro, "https://giro.uml.edu/", "CC BY-NC-SA 4.0"),
    Credit("Open-Meteo", R.string.about_credit_openMeteo, "https://open-meteo.com/", "CC BY 4.0"),
)

private val LIBRARIES = listOf(
    "Android Jetpack, Jetpack Compose, Material 3" to "Apache-2.0",
    "Kotlin, kotlinx.coroutines" to "Apache-2.0",
    "JNA" to "Apache-2.0",
    "UniFFI" to "MPL-2.0",
    "tiny-skia" to "BSD-3-Clause",
    "serde, serde_json, ureq, rustls, ring, webpki-roots" to "MIT / Apache-2.0 / ISC / MPL-2.0",
    "Material Symbols" to "Apache-2.0",
)

const val SOURCE_URL = "https://code.alvaroedo.io/alvaroemtnez/HFPredict"

@Composable
fun AboutScreen(compact: Boolean, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    val open = { url: String -> ctx.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
    FullScreenDialog(tr(R.string.about_title), onDismiss, compact) { mod ->
        Column(mod.fillMaxSize().verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            Column(Modifier.fillMaxWidth().padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(
                    painterResource(R.drawable.ic_launcher_foreground), null,
                    tint = androidx.compose.ui.graphics.Color.Unspecified, modifier = Modifier.size(112.dp),
                )
                Text(tr(R.string.app_name), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                Text(tr(R.string.about_version, "version" to BuildConfig.VERSION_NAME), color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("© 2026 Álvaro Edo Martínez · GPL-3.0-or-later", style = MaterialTheme.typography.bodySmall)
                Text(tr(R.string.about_description), Modifier.padding(top = 12.dp), style = MaterialTheme.typography.bodyMedium)
            }
            ListItem(
                headlineContent = { Text(tr(R.string.about_source)) },
                supportingContent = { Text(SOURCE_URL) },
                modifier = Modifier.clickable { open(SOURCE_URL) },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
            )
            ListItem(
                headlineContent = { Text(tr(R.string.about_issues)) },
                modifier = Modifier.clickable { open("$SOURCE_URL/issues") },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
            )
            Group(tr(R.string.about_builtOn))
            BUILT_ON.forEach { CreditRow(it, open) }
            Group(tr(R.string.about_dataAndServices))
            DATA.forEach { CreditRow(it, open) }
            Text(
                tr(R.string.about_endorsement),
                Modifier.padding(16.dp),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            HorizontalDivider()
            Group(tr(R.string.about_licences))
            LIBRARIES.forEach { (what, terms) ->
                ListItem(
                    headlineContent = { Text(what) },
                    supportingContent = { Text(terms) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
                )
            }
        }
    }
}

@Composable
private fun CreditRow(c: Credit, open: (String) -> Unit) {
    ListItem(
        headlineContent = { Text(c.who) },
        supportingContent = { Text(tr(c.what) + (c.terms?.let { " · $it" } ?: "")) },
        modifier = Modifier.clickable { open(c.url) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
    )
}
