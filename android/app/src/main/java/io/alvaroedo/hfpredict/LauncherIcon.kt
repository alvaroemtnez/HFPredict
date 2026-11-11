package io.alvaroedo.hfpredict

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import androidx.annotation.ColorRes
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes

/**
 * The home-screen icon's colourway. Each is an activity-alias in the
 * manifest; exactly one is enabled, and PackageManager is the only record
 * of which (nothing to save, nothing to drift). Icons: tools/gen-icons.py.
 */
enum class LauncherIcon(
    private val alias: String,
    @param:StringRes val label: Int,
    @param:ColorRes val background: Int,
    @param:DrawableRes val foreground: Int,
) {
    DEEP_SEA("LauncherDeepSea", R.string.icon_deepSea, R.color.ic_launcher_background_deep_sea, R.drawable.ic_launcher_foreground_deep_sea),
    NIGHT("LauncherNight", R.string.icon_night, R.color.ic_launcher_background_night, R.drawable.ic_launcher_foreground_night),
    SUNSPOT("LauncherSunspot", R.string.icon_sunspot, R.color.ic_launcher_background_sunspot, R.drawable.ic_launcher_foreground_sunspot),
    BAND_OPEN("LauncherBandOpen", R.string.icon_bandOpen, R.color.ic_launcher_background_band_open, R.drawable.ic_launcher_foreground_band_open),
    AURORA("LauncherAurora", R.string.icon_aurora, R.color.ic_launcher_background_aurora, R.drawable.ic_launcher_foreground_aurora),
    DAYLIGHT("LauncherDaylight", R.string.icon_daylight, R.color.ic_launcher_background_daylight, R.drawable.ic_launcher_foreground_daylight),
    ;

    /** The alias's class name: the manifest's ".Name" resolves against the namespace, never the application id. */
    internal val className get() = "${BuildConfig.NAMESPACE}.$alias"

    private fun component(context: Context) = ComponentName(context, className)

    companion object {
        fun current(context: Context): LauncherIcon {
            val pm = context.packageManager
            return entries.firstOrNull {
                when (pm.getComponentEnabledSetting(it.component(context))) {
                    PackageManager.COMPONENT_ENABLED_STATE_ENABLED -> true
                    // Untouched: the manifest's default, which only Deep sea has on.
                    PackageManager.COMPONENT_ENABLED_STATE_DEFAULT -> it == DEEP_SEA
                    else -> false
                }
            } ?: DEEP_SEA
        }

        /** Enables the new alias before disabling the old, so the app always has a launcher entry. */
        fun set(context: Context, icon: LauncherIcon) {
            val pm = context.packageManager
            pm.setComponentEnabledSetting(
                icon.component(context), PackageManager.COMPONENT_ENABLED_STATE_ENABLED, PackageManager.DONT_KILL_APP,
            )
            entries.filter { it != icon }.forEach {
                pm.setComponentEnabledSetting(
                    it.component(context), PackageManager.COMPONENT_ENABLED_STATE_DISABLED, PackageManager.DONT_KILL_APP,
                )
            }
        }
    }
}
