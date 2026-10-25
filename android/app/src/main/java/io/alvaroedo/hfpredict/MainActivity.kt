package io.alvaroedo.hfpredict

import android.app.LocaleManager
import android.content.Context
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.os.LocaleList
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.alvaroedo.hfpredict.ui.AboutScreen
import io.alvaroedo.hfpredict.ui.CompassGoal
import io.alvaroedo.hfpredict.ui.CompassScreen
import io.alvaroedo.hfpredict.ui.FullScreenMap
import io.alvaroedo.hfpredict.ui.compassSensor
import io.alvaroedo.hfpredict.ui.FirstRunScreen
import io.alvaroedo.hfpredict.ui.HelpScreen
import io.alvaroedo.hfpredict.ui.HfpTheme
import io.alvaroedo.hfpredict.ui.isAppDark
import io.alvaroedo.hfpredict.ui.MainScreen
import io.alvaroedo.hfpredict.ui.Overlay
import io.alvaroedo.hfpredict.ui.PathScreen
import io.alvaroedo.hfpredict.ui.PreferencesScreen
import io.alvaroedo.hfpredict.ui.StationScreen
import kotlinx.coroutines.delay
import java.util.Locale

/** The app's languages, with their own names; null follows the system. */
object Languages {
    val ALL = listOf(
        "en" to "English",
        "es" to "Español",
        "ca" to "Valencià",
        "de" to "Deutsch",
        "ja" to "日本語",
        "ar" to "العربية",
    )

    /**
     * Android 13 and later keep the choice themselves (and show it in the
     * system's per-app language settings) and hand the activity the new
     * configuration, which Compose redraws in place (the manifest takes
     * locale changes, so nothing is torn down). Older versions read it
     * from the saved state as the activity starts, so it is saved first
     * and the activity then recreated.
     */
    fun set(context: Context, code: String?) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.getSystemService(LocaleManager::class.java).applicationLocales =
                if (code == null) LocaleList.getEmptyLocaleList() else LocaleList.forLanguageTags(code)
        } else {
            val core = (context.applicationContext as HfpApplication).core
            runCatching { core.setSettings(core.state().settings.copy(language = code)) }
            (context.findActivity() as? MainActivity)?.recreate()
        }
    }

    fun current(context: Context): String? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.getSystemService(LocaleManager::class.java).applicationLocales.get(0)?.language
        } else {
            (context.applicationContext as HfpApplication).core.state().settings.language
        }
}

private tailrec fun Context.findActivity(): android.app.Activity? = when (this) {
    is android.app.Activity -> this
    is android.content.ContextWrapper -> baseContext.findActivity()
    else -> null
}

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels()

    override fun attachBaseContext(base: Context) {
        val code = if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            (base.applicationContext as? HfpApplication)?.core?.state()?.settings?.language
        } else null
        if (code == null) {
            super.attachBaseContext(base)
            return
        }
        val config = Configuration(base.resources.configuration)
        config.setLocales(LocaleList(Locale.forLanguageTag(code)))
        super.attachBaseContext(base.createConfigurationContext(config))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        lockPhonesToPortrait()

        setContent {
            val state by vm.state.collectAsStateWithLifecycle()
            // The clock ticks while the app is on screen.
            LaunchedEffect(Unit) {
                lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                    while (true) {
                        vm.tick()
                        delay(60_000)
                    }
                }
            }
            // The system bars follow the app's theme, not the system's.
            val dark = isAppDark(state.saved.settings.theme)
            LaunchedEffect(dark) {
                val transparent = android.graphics.Color.TRANSPARENT
                enableEdgeToEdge(
                    statusBarStyle = if (dark) SystemBarStyle.dark(transparent) else SystemBarStyle.light(transparent, transparent),
                    navigationBarStyle = if (dark) SystemBarStyle.dark(transparent) else SystemBarStyle.light(transparent, transparent),
                )
            }
            HfpTheme(state.saved.settings.theme) {
                // Read through Compose, so a language change redraws in place.
                val locale = androidx.compose.ui.platform.LocalConfiguration.current.locales[0]
                val imperial = state.saved.settings.units.let {
                    when (it) {
                        io.alvaroedo.hfpredict.core.Units.METRIC -> false
                        io.alvaroedo.hfpredict.core.Units.IMPERIAL -> true
                        io.alvaroedo.hfpredict.core.Units.AUTO -> locale.country in setOf("US", "LR", "MM")
                    }
                }
                val fmt = Format(locale, imperial)
                var overlay by rememberSaveable(stateSaver = OverlaySaver) { mutableStateOf<Overlay>(Overlay.None) }
                BackHandler(enabled = overlay != Overlay.None) { overlay = Overlay.None }
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
                    BoxWithConstraints(Modifier.fillMaxSize()) {
                        val compact = maxWidth < 600.dp
                        if (state.firstRun) {
                            Surface(Modifier.fillMaxSize().safeDrawingPadding()) { FirstRunScreen(state, vm) }
                        } else {
                            MainScreen(state, vm, fmt) { overlay = it }
                        }
                        val dismiss = { overlay = Overlay.None }
                        when (val o = overlay) {
                            Overlay.None -> Unit
                            is Overlay.Path -> PathScreen(state, vm, o.to, compact, dismiss)
                            Overlay.Station -> StationScreen(state, vm, fmt, compact, dismiss)
                            Overlay.Preferences -> PreferencesScreen(state, vm, compact, dismiss)
                            Overlay.Help -> HelpScreen(fmt, compact, dismiss)
                            Overlay.About -> AboutScreen(compact, dismiss)
                            Overlay.Map -> FullScreenMap(
                                state, vm, fmt,
                                onPoint = if (compassSensor(this@MainActivity) != null) { b, name -> overlay = Overlay.Compass(b, name) } else null,
                                onDismiss = dismiss,
                            )
                            is Overlay.Compass -> {
                                val saved = state.saved
                                val station = saved.stations.firstOrNull { it.id == saved.activeStation } ?: saved.stations.first()
                                val from = saved.from ?: io.alvaroedo.hfpredict.core.greenwich()
                                CompassScreen(CompassGoal.Point(o.bearing, o.name), station, from.lat, from.lon, fmt, compact, dismiss)
                            }
                        }
                    }
                }
            }
        }
    }

    /**
     * Phones stay upright: the layout is built for a narrow portrait screen
     * and a phone on its side has too little height for the map. Tablets
     * (smallest width 600 dp or more) turn freely and get the two-column
     * layout in landscape.
     */
    private fun lockPhonesToPortrait() {
        requestedOrientation = if (resources.configuration.smallestScreenWidthDp < 600) {
            ActivityInfo.SCREEN_ORIENTATION_USER_PORTRAIT
        } else {
            ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        lockPhonesToPortrait()
    }
}

/** Keeps the open screen across process death (rotation is handled in-process). */
private val OverlaySaver = androidx.compose.runtime.saveable.Saver<Overlay, String>(
    save = {
        when (it) {
            Overlay.None -> "none"
            is Overlay.Path -> if (it.to) "path-to" else "path-from"
            Overlay.Station -> "station"
            Overlay.Preferences -> "preferences"
            Overlay.Help -> "help"
            Overlay.About -> "about"
            Overlay.Map -> "map"
            is Overlay.Compass -> "compass|${it.bearing ?: ""}|${it.name ?: ""}"
        }
    },
    restore = {
        when (it) {
            "path-to" -> Overlay.Path(true)
            "path-from" -> Overlay.Path(false)
            "station" -> Overlay.Station
            "preferences" -> Overlay.Preferences
            "help" -> Overlay.Help
            "about" -> Overlay.About
            "map" -> Overlay.Map
            else -> if (it.startsWith("compass|")) {
                val parts = it.split("|", limit = 3)
                Overlay.Compass(parts.getOrNull(1)?.toDoubleOrNull(), parts.getOrNull(2)?.ifEmpty { null })
            } else Overlay.None
        }
    },
)
