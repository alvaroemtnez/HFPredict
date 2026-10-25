package io.alvaroedo.hfpredict

import android.app.Application
import android.graphics.Bitmap
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.createBitmap
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.alvaroedo.hfpredict.core.Band
import io.alvaroedo.hfpredict.core.BandReach
import io.alvaroedo.hfpredict.core.Core
import io.alvaroedo.hfpredict.core.CoreException
import io.alvaroedo.hfpredict.core.Forecast
import io.alvaroedo.hfpredict.core.MapDetail
import io.alvaroedo.hfpredict.core.MapOptions
import io.alvaroedo.hfpredict.core.MapStatus
import io.alvaroedo.hfpredict.core.MapView
import io.alvaroedo.hfpredict.core.Palette
import io.alvaroedo.hfpredict.core.Place
import io.alvaroedo.hfpredict.core.Probe
import io.alvaroedo.hfpredict.core.SavedState
import io.alvaroedo.hfpredict.core.Settings
import io.alvaroedo.hfpredict.core.Station
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.nio.ByteBuffer

/** The last map drawn, and the view it was drawn for. */
data class RenderedMap(val image: ImageBitmap, val view: MapView)

/** How many hours behind "now" the hour track starts. */
const val PAST_HOURS = 6

val WHOLE_GLOBE = MapView(1.0, 0.5, 0.5)

data class UiState(
    val saved: SavedState,
    val band: Band,
    val hour: Int,
    val now: Long,
    val forecast: Forecast? = null,
    val forecastError: String? = null,
    val view: MapView = WHOLE_GLOBE,
    val reach: List<BandReach> = emptyList(),
    val status: MapStatus? = null,
    val detail: MapDetail = MapDetail.NONE,
    val mapWorking: Boolean = false,
    val mapError: String? = null,
    val rendered: RenderedMap? = null,
    val probe: Probe? = null,
    val showTable: Boolean = false,
    val live: Boolean = false,
    val refreshing: Boolean = false,
) {
    val nowHour: Int get() = Math.floorMod(now, 86_400L).toInt() / 3600
    /** The first hour on the track. */
    val trackStart: Int get() = (nowHour + 24 - PAST_HOURS) % 24
    val firstRun: Boolean get() = saved.from == null
}

fun unixNow(): Long = System.currentTimeMillis() / 1000

class AppViewModel(application: Application) : AndroidViewModel(application) {
    val core: Core = (application as HfpApplication).core

    private val _state: MutableStateFlow<UiState> = run {
        val saved = core.state()
        val now = unixNow()
        MutableStateFlow(
            UiState(saved = saved, band = saved.band, hour = Math.floorMod(now, 86_400L).toInt() / 3600, now = now),
        )
    }
    val state: StateFlow<UiState> = _state.asStateFlow()

    private var forecastJob: Job? = null
    private var mapJob: Job? = null
    private var mapGeneration = 0
    private var surfacePx = 0
    private var density = 1f
    private var palette: Palette? = null
    private var rendering = false
    private var renderAgain = false
    private var lastReadings = 0L

    init {
        _state.update { it.copy(live = core.isNowcast(it.now)) }
        reload()
        refreshReadings(force = true)
    }

    // ---- Saved state ---------------------------------------------------

    /** Applies a change through the core (which saves it), then recomputes. */
    fun save(change: (Core) -> SavedState) {
        viewModelScope.launch {
            val saved = withContext(Dispatchers.IO) {
                try {
                    change(core)
                } catch (_: CoreException) {
                    null
                }
            } ?: return@launch
            _state.update { it.copy(saved = saved, band = saved.band, live = core.isNowcast(it.now)) }
            reload()
        }
    }

    fun setFrom(place: Place) = save { it.setFrom(place) }
    fun setTo(place: Place?) = save { it.setTo(place) }
    fun swapEnds() = save { it.swapEnds() }
    fun putStation(station: Station) = save { it.putStation(station) }
    fun selectStation(id: String) = save { it.selectStation(id) }
    fun deleteStation(id: String) = save { it.deleteStation(id) }

    fun setSettings(settings: Settings) {
        val before = _state.value.saved.settings
        save { it.setSettings(settings) }
        if (settings.network && !before.network) refreshReadings(force = true)
    }

    // ---- Forecast and maps ---------------------------------------------

    fun reload() {
        forecastJob?.cancel()
        val now = _state.value.now
        _state.update { it.copy(reach = emptyList(), status = null, probe = null) }
        forecastJob = viewModelScope.launch {
            val result = withContext(Dispatchers.Default) { runCatching { core.forecast(now) } }
            _state.update { s ->
                result.fold(
                    { s.copy(forecast = it, forecastError = null) },
                    { s.copy(forecastError = it.message ?: it.toString()) },
                )
            }
        }
        updateMap()
    }

    fun select(band: Band, hour: Int) {
        val s = _state.value
        if (band != s.band) {
            viewModelScope.launch(Dispatchers.IO) { runCatching { core.setBand(band) } }
        }
        _state.update { it.copy(band = band, hour = Math.floorMod(hour, 24), probe = null) }
        updateMap()
    }

    /** The coarse grid at once, then the fine one, then a patch if zoomed. */
    private fun updateMap() {
        mapJob?.cancel()
        val generation = ++mapGeneration
        _state.update { it.copy(mapWorking = true, mapError = null) }
        render()
        val s = _state.value
        val hour = s.hour.toUInt()
        mapJob = viewModelScope.launch {
            val reach = withContext(Dispatchers.Default) { runCatching { core.coarseMaps(hour, s.now) } }
            if (generation != mapGeneration) return@launch
            _state.update { st -> reach.fold({ st.copy(reach = it) }, { st.copy(mapError = it.message) }) }
            render()
            val fine = withContext(Dispatchers.Default) {
                runCatching {
                    val status = core.fineMap(s.band, hour, s.now)
                    runCatching { core.patchMap(hour, s.now, _state.value.view) }
                    status
                }
            }
            if (generation != mapGeneration) return@launch
            _state.update { st ->
                fine.fold({ st.copy(status = it, mapWorking = false) }, { st.copy(mapError = it.message, mapWorking = false) })
            }
            render()
        }
    }

    /** The map's size on screen, its density and the theme's colours. */
    fun setMapSurface(px: Int, density: Float, palette: Palette) {
        if (px == surfacePx && density == this.density && palette == this.palette) return
        surfacePx = px
        this.density = density
        this.palette = palette
        render()
    }

    /** A pan or zoom: redraw at once, and work out a patch once it settles. */
    fun setView(view: MapView, settled: Boolean) {
        _state.update { it.copy(view = view) }
        render()
        if (settled) {
            val s = _state.value
            val generation = mapGeneration
            viewModelScope.launch {
                val patched = withContext(Dispatchers.Default) {
                    runCatching { core.patchMap(s.hour.toUInt(), s.now, view) }.getOrDefault(false)
                }
                if (patched && generation == mapGeneration) render()
            }
        }
    }

    fun fitReach() {
        val s = _state.value
        viewModelScope.launch {
            val v = withContext(Dispatchers.Default) { core.fitView(s.band, s.hour.toUInt(), s.now) }
            setView(v, settled = true)
        }
    }

    fun probe(x: Double, y: Double, size: Double) {
        val s = _state.value
        viewModelScope.launch {
            val p = withContext(Dispatchers.Default) { core.probe(s.band, s.hour.toUInt(), s.now, s.view, x, y, size) }
            _state.update { it.copy(probe = p) }
        }
    }

    fun toggleTable() = _state.update { it.copy(showTable = !it.showTable) }

    /** Draws the map from what is computed; one render at a time, the rest folded. */
    private fun render() {
        val pal = palette ?: return
        if (surfacePx < 16) return
        if (rendering) {
            renderAgain = true
            return
        }
        rendering = true
        val s = _state.value
        val px = surfacePx
        val d = density
        viewModelScope.launch {
            val image = withContext(Dispatchers.Default) {
                val img = core.renderMap(
                    s.band, s.hour.toUInt(), s.now, s.view, px.toUInt(), d, pal,
                    MapOptions(showNight = true, showNvis = true, showPath = true),
                )
                val size = img.size.toInt()
                val bitmap = createBitmap(size, size, Bitmap.Config.ARGB_8888)
                // Premultiplied RGBA bytes: exactly ARGB_8888's memory layout.
                bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(img.pixels))
                img.detail to bitmap.asImageBitmap()
            }
            rendering = false
            _state.update { it.copy(rendered = RenderedMap(image.second, s.view), detail = image.first) }
            if (renderAgain) {
                renderAgain = false
                render()
            }
        }
    }

    // ---- Readings and the clock ----------------------------------------

    /** NOAA and GIRO, when the network is allowed; at most once a minute. */
    fun refreshReadings(force: Boolean = false) {
        val now = unixNow()
        if (!_state.value.saved.settings.network) return
        if (!force && now - lastReadings < 60) {
            _state.update { it.copy(refreshing = false) }
            return
        }
        lastReadings = now
        _state.update { it.copy(refreshing = true) }
        viewModelScope.launch {
            val (before, after) = withContext(Dispatchers.IO) {
                val old = core.state().spaceWeather
                runCatching { core.refreshSpaceWeather(now) }
                runCatching { core.refreshSounding(now) }
                old to core.state()
            }
            val changed = before?.effectiveSsn != after.spaceWeather?.effectiveSsn ||
                before?.kpMax24h != after.spaceWeather?.kpMax24h
            _state.update { it.copy(saved = after, refreshing = false, live = core.isNowcast(it.now)) }
            if (changed) reload()
        }
    }

    /** Every minute: rolls "now", following the clock if on the current hour. */
    fun tick() {
        val now = unixNow()
        val s = _state.value
        val following = s.hour == s.nowHour
        val rolled = now / 3600 != s.now / 3600
        _state.update { it.copy(now = now) }
        if (rolled) {
            if (following) _state.update { it.copy(hour = it.nowHour) }
            _state.update { it.copy(live = core.isNowcast(now)) }
            reload()
        }
        if (now - lastReadings > 30 * 60) refreshReadings()
    }
}
