package io.alvaroedo.hfpredict.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.alvaroedo.hfpredict.Format
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.UiState
import io.alvaroedo.hfpredict.core.Basis
import io.alvaroedo.hfpredict.core.Model
import io.alvaroedo.hfpredict.tr

/** The sun today: live readings and the nearest ionosonde, collapsible. */
@Composable
fun SunCard(state: UiState, fmt: Format, modifier: Modifier = Modifier) {
    val ctx = LocalContext.current
    var open by rememberSaveable { mutableStateOf(false) }
    val sw = state.saved.spaceWeather
    val fresh = sw != null && state.now - sw.observedAt <= 24 * 3600
    SectionCard(modifier) {
        Row(
            Modifier.fillMaxWidth().clickable { open = !open },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(tr(R.string.sections_sun), style = MaterialTheme.typography.titleMedium)
                if (sw != null) {
                    Text(
                        tr(if (fresh) R.string.spaceWeather_updated else R.string.spaceWeather_asOf, "time" to fmt.hm(sw.fetchedAt)),
                        style = MaterialTheme.typography.bodySmall,
                        color = if (fresh) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error,
                    )
                }
            }
            Icon(painterResource(if (open) R.drawable.ic_expand_less else R.drawable.ic_expand_more), contentDescription = null)
        }
        AnimatedVisibility(open) {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (sw == null) {
                    Text(
                        tr(if (state.saved.settings.network) R.string.spaceWeather_unavailable else R.string.spaceWeather_networkOff),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                } else {
                    Reading(tr(R.string.spaceWeather_flux), tr(R.string.spaceWeather_fluxHint), fmt.int(sw.f107))
                    HorizontalDivider()
                    val kpHint = when {
                        !fresh -> tr(R.string.spaceWeather_stale)
                        sw.kpMax24h >= 5 -> tr(R.string.spaceWeather_kpStormy)
                        sw.kpMax24h >= 4 -> tr(R.string.spaceWeather_kpUnsettled)
                        else -> tr(R.string.spaceWeather_kpQuiet)
                    }
                    Reading(
                        tr(R.string.spaceWeather_kp),
                        kpHint + "\n" + tr(R.string.spaceWeather_kpMax, "kp" to fmt.dec(sw.kpMax24h, 1)),
                        fmt.dec(sw.kp, 1),
                    )
                    HorizontalDivider()
                    Reading(tr(R.string.spaceWeather_effectiveSsn), tr(R.string.spaceWeather_effectiveSsnHint), fmt.int(sw.effectiveSsn))
                }
                state.saved.sounding?.let { s ->
                    HorizontalDivider()
                    Column {
                        Text(tr(R.string.spaceWeather_measuredTitle, "value" to fmt.mhz(ctx, s.fof2)), style = MaterialTheme.typography.titleSmall)
                        Text(
                            tr(
                                R.string.spaceWeather_measuredSub,
                                "station" to "${s.station} (${s.ursi})",
                                "distance" to fmt.distance(ctx, s.km),
                                "time" to fmt.hm(s.measuredAt),
                            ),
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(tr(R.string.spaceWeather_measuredNote), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}

@Composable
private fun Reading(title: String, hint: String, value: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Text(hint, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(value, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Medium)
    }
}

/** What the forecast assumes, in words. */
@Composable
fun BasisCard(state: UiState, fmt: Format, modifier: Modifier = Modifier) {
    val f = state.forecast ?: return
    val (title, body) = when (f.model) {
        Model.TRUECAST -> if (f.basis == Basis.NOWCAST) R.string.basis_truecastNowcast_title to R.string.basis_truecastNowcast_body
        else R.string.basis_truecastOffline_title to R.string.basis_truecastOffline_body
        Model.VOACAP -> when (f.basis) {
            Basis.NOWCAST -> R.string.basis_voacapNowcast_title to R.string.basis_voacapNowcast_body
            Basis.FORECAST -> R.string.basis_voacapForecast_title to R.string.basis_voacapForecast_body
            Basis.CLIMATOLOGY -> R.string.basis_voacapClimatology_title to R.string.basis_voacapClimatology_body
        }
    }
    SectionCard(modifier) {
        Text(tr(title), style = MaterialTheme.typography.titleSmall)
        Text(tr(body, "ssn" to fmt.int(f.ssn)), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(tr(R.string.basis_caveat), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
