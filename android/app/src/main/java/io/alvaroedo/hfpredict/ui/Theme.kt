package io.alvaroedo.hfpredict.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import io.alvaroedo.hfpredict.core.Palette
import io.alvaroedo.hfpredict.core.Quality
import io.alvaroedo.hfpredict.core.Theme

/*
 * The theme is the device's: Material You colours from the wallpaper on
 * Android 12 and later, a built-in teal scheme before that. Low light is a
 * dim red-on-black scheme for keeping night vision in the field.
 */

private val TealLight = lightColorScheme(
    primary = Color(0xFF00677E), onPrimary = Color.White,
    primaryContainer = Color(0xFFB7EAFF), onPrimaryContainer = Color(0xFF001F28),
    secondary = Color(0xFF4C626A), onSecondary = Color.White,
    secondaryContainer = Color(0xFFCFE6F0), onSecondaryContainer = Color(0xFF071E25),
    tertiary = Color(0xFF8A5100), onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFDCBE), onTertiaryContainer = Color(0xFF2C1600),
    background = Color(0xFFF6FAFC), onBackground = Color(0xFF171C1E),
    surface = Color(0xFFF6FAFC), onSurface = Color(0xFF171C1E),
    surfaceVariant = Color(0xFFDBE4E8), onSurfaceVariant = Color(0xFF3F484C),
    surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFF0F4F6),
    surfaceContainer = Color(0xFFEAEFF1), surfaceContainerHigh = Color(0xFFE4E9EB),
    surfaceContainerHighest = Color(0xFFDFE3E5),
    outline = Color(0xFF70787C), outlineVariant = Color(0xFFBFC8CC),
)

private val TealDark = darkColorScheme(
    primary = Color(0xFF5DD5F9), onPrimary = Color(0xFF003543),
    primaryContainer = Color(0xFF004E5F), onPrimaryContainer = Color(0xFFB7EAFF),
    secondary = Color(0xFFB3CAD3), onSecondary = Color(0xFF1E333B),
    secondaryContainer = Color(0xFF344A52), onSecondaryContainer = Color(0xFFCFE6F0),
    tertiary = Color(0xFFFFB870), onTertiary = Color(0xFF4A2800),
    tertiaryContainer = Color(0xFF693C00), onTertiaryContainer = Color(0xFFFFDCBE),
    background = Color(0xFF0F1416), onBackground = Color(0xFFDEE3E6),
    surface = Color(0xFF0F1416), onSurface = Color(0xFFDEE3E6),
    surfaceVariant = Color(0xFF3F484C), onSurfaceVariant = Color(0xFFBFC8CC),
    surfaceContainerLowest = Color(0xFF0A0F11), surfaceContainerLow = Color(0xFF171C1E),
    surfaceContainer = Color(0xFF1B2022), surfaceContainerHigh = Color(0xFF252B2D),
    surfaceContainerHighest = Color(0xFF303638),
    outline = Color(0xFF899296), outlineVariant = Color(0xFF3F484C),
)

private val LowLight = darkColorScheme(
    primary = Color(0xFFD9402A), onPrimary = Color(0xFF1A0000),
    primaryContainer = Color(0xFF5A0E06), onPrimaryContainer = Color(0xFFFF8A75),
    secondary = Color(0xFFB0331F), onSecondary = Color(0xFF140000),
    secondaryContainer = Color(0xFF3A0803), onSecondaryContainer = Color(0xFFE05A44),
    tertiary = Color(0xFFFF6A50), onTertiary = Color(0xFF1A0000),
    tertiaryContainer = Color(0xFF4A0B05), onTertiaryContainer = Color(0xFFFF8A75),
    background = Color.Black, onBackground = Color(0xFFD9402A),
    surface = Color.Black, onSurface = Color(0xFFD9402A),
    surfaceVariant = Color(0xFF1C0402), onSurfaceVariant = Color(0xFFA8301F),
    surfaceContainerLowest = Color.Black, surfaceContainerLow = Color(0xFF0A0101),
    surfaceContainer = Color(0xFF100201), surfaceContainerHigh = Color(0xFF160302),
    surfaceContainerHighest = Color(0xFF1C0402),
    outline = Color(0xFF7A1D12), outlineVariant = Color(0xFF3A0803),
    error = Color(0xFFFF6A50), onError = Color.Black,
)

/** Colours for the map and the band grid, from the scheme. */
@Immutable
data class MapColors(
    /** Closed, weak, patchy, reliable. */
    val cells: List<Color>,
    val now: Color,
    val palette: Palette,
) {
    fun cell(q: Quality): Color = cells[when (q) {
        Quality.CLOSED -> 0
        Quality.WEAK -> 1
        Quality.PATCHY -> 2
        Quality.RELIABLE -> 3
    }]
}

private fun Color.argb(): UInt = toArgb().toUInt()

fun mapColors(scheme: ColorScheme, dark: Boolean, lowLight: Boolean): MapColors {
    // Three steps of the primary tone, strongest where the band is surest.
    val reliable = if (dark) scheme.primary else lerp(scheme.primary, Color.Black, 0.15f)
    val weak = if (dark) lerp(scheme.primaryContainer, scheme.surface, 0.15f) else scheme.primaryContainer
    val patchy = lerp(weak, reliable, if (dark) 0.5f else 0.55f)
    val closed = scheme.onSurface.copy(alpha = 0.07f)
    val disc = if (dark) scheme.surfaceContainerLowest else scheme.surfaceContainerHigh
    val station = if (lowLight) Color(0xFFFF8A65) else scheme.tertiary
    val palette = Palette(
        disc = disc.argb(),
        reliable = reliable.argb(),
        patchy = patchy.argb(),
        weak = weak.argb(),
        night = (if (dark) Color.Black.copy(alpha = 0.36f) else Color(0xFF000020).copy(alpha = 0.14f)).argb(),
        coastline = scheme.onSurface.copy(alpha = if (dark) 0.72f else 0.62f).argb(),
        gridLines = scheme.onSurface.copy(alpha = 0.28f).argb(),
        path = station.argb(),
        station = station.argb(),
        halo = disc.argb(),
        nvis = scheme.onSurface.argb(),
    )
    return MapColors(listOf(closed, weak, patchy, reliable), station, palette)
}

val LocalMapColors = staticCompositionLocalOf { mapColors(TealDark, dark = true, lowLight = false) }

@Composable
fun HfpTheme(theme: Theme, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val systemDark = isSystemInDarkTheme()
    val lowLight = theme == Theme.LOW_LIGHT
    val dark = when (theme) {
        Theme.SYSTEM -> systemDark
        Theme.LIGHT -> false
        Theme.DARK, Theme.LOW_LIGHT -> true
    }
    val scheme = when {
        lowLight -> LowLight
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        else -> if (dark) TealDark else TealLight
    }
    val colors = remember(scheme, dark, lowLight) { mapColors(scheme, dark, lowLight) }
    CompositionLocalProvider(LocalMapColors provides colors) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
