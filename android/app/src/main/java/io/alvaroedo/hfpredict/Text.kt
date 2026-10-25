package io.alvaroedo.hfpredict

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import io.alvaroedo.hfpredict.core.AntennaKind
import io.alvaroedo.hfpredict.core.Mode
import io.alvaroedo.hfpredict.core.Quality
import java.text.NumberFormat
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

/*
 * Text, from the string tables shared with the GNOME app (i18n/<lang>.json,
 * generated into res/values*). Placeholders are named, `{band}`, so every
 * language can order them freely.
 */

fun fill(text: String, args: Array<out Pair<String, Any>>): String =
    args.fold(text) { acc, (k, v) -> acc.replace("{$k}", v.toString()) }

@Composable
fun tr(@StringRes id: Int, vararg args: Pair<String, Any>): String = fill(stringResource(id), args)

fun Context.tr(@StringRes id: Int, vararg args: Pair<String, Any>): String = fill(getString(id), args)

@StringRes
fun Mode.label(): Int = when (this) {
    Mode.FM -> R.string.station_mode_fm
    Mode.AM -> R.string.station_mode_am
    Mode.SSB -> R.string.station_mode_ssb
    Mode.RTTY -> R.string.station_mode_rtty
    Mode.CW -> R.string.station_mode_cw
    Mode.PSK31 -> R.string.station_mode_psk31
    Mode.FT8 -> R.string.station_mode_ft8
    Mode.JS8 -> R.string.station_mode_js8
    Mode.WSPR -> R.string.station_mode_wspr
}

@StringRes
fun AntennaKind.label(): Int = when (this) {
    AntennaKind.ISOTROPIC -> R.string.station_antenna_isotropic
    AntennaKind.DIPOLE -> R.string.station_antenna_dipole
    AntennaKind.INVERTED_V -> R.string.station_antenna_invertedV
    AntennaKind.VERTICAL -> R.string.station_antenna_vertical
    AntennaKind.INVERTED_L -> R.string.station_antenna_invertedL
    AntennaKind.YAGI -> R.string.station_antenna_yagi
}

@StringRes
fun Quality.label(): Int = when (this) {
    Quality.RELIABLE -> R.string.quality_reliable
    Quality.PATCHY -> R.string.quality_patchy
    Quality.WEAK -> R.string.quality_weak
    Quality.CLOSED -> R.string.quality_closed
}

@StringRes
fun Quality.description(): Int = when (this) {
    Quality.RELIABLE -> R.string.qualityDescription_reliable
    Quality.PATCHY -> R.string.qualityDescription_patchy
    Quality.WEAK -> R.string.qualityDescription_weak
    Quality.CLOSED -> R.string.qualityDescription_closed
}

/** Numbers and units in the reader's locale. */
class Format(private val locale: Locale, val imperial: Boolean) {
    private val ints = NumberFormat.getIntegerInstance(locale)
    private fun decimals(places: Int) = NumberFormat.getNumberInstance(locale).apply {
        minimumFractionDigits = places
        maximumFractionDigits = places
    }

    fun int(v: Double): String = ints.format(v.roundToInt())
    fun dec(v: Double, places: Int): String = decimals(places).format(v)
    fun percent(share: Double): String = NumberFormat.getPercentInstance(locale).format(share)
    fun hour(h: Int): String = "%02d:00".format(Locale.ROOT, ((h % 24) + 24) % 24)
    fun hm(unix: Long): String {
        val s = Math.floorMod(unix, 86_400L)
        return "%02d:%02d".format(Locale.ROOT, s / 3600, (s % 3600) / 60)
    }
    fun degrees(d: Double): String = int(((d % 360) + 360) % 360)

    /** Rounded to what a reader can hold: fives below 100, tens below 1,000. */
    fun distance(ctx: Context, km: Double): String {
        val v = if (imperial) km / 1.609344 else km
        val r = when {
            v < 100 -> (v / 5).roundToInt() * 5.0
            v < 1000 -> (v / 10).roundToInt() * 10.0
            else -> (v / 100).roundToInt() * 100.0
        }
        return ctx.tr(if (imperial) R.string.units_miles else R.string.units_kilometres, "value" to int(r))
    }

    fun height(ctx: Context, m: Double): String =
        if (imperial) ctx.tr(R.string.units_feet, "value" to int(m / 0.3048))
        else ctx.tr(R.string.units_metres, "value" to int(m))

    fun watts(ctx: Context, w: Double): String {
        val v = if (w < 1 || (w < 10 && abs(w - w.roundToInt()) > 1e-9)) dec(w, 1) else int(w)
        return ctx.tr(R.string.units_watts, "value" to v)
    }

    fun mhz(ctx: Context, v: Double): String = ctx.tr(R.string.units_megahertz, "value" to dec(v, 1))
}

@Composable
fun currentLocale(): Locale = LocalConfiguration.current.locales[0]
