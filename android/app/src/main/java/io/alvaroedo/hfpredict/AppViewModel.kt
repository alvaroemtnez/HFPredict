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
import io.alvaroedo.hfpredict.core.Place
import io.alvaroedo.hfpredict.core.Probe
import io.alvaroedo.hfpredict.core.Screen
import io.alvaroedo.hfpredict.core.Tile
import io.alvaroedo.hfpredict.core.qualityOf
import io.alvaroedo.hfpredict.core.semaphoreColor
import io.alvaroedo.hfpredict.core.tileLevel
import io.alvaroedo.hfpredict.core.tilesVisible
import io.alvaroedo.hfpredict.core.viewZoomedIn
import io.alvaroedo.hfpredict.ui.MapColors
import io.alvaroedo.hfpredict.core.SavedState
import io.alvaroedo.hfpredict.core.Settings
import io.alvaroedo.hfpredict.core.Station
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.nio.ByteBuffer

/**
 * A rendered tile of the map (see the core's `tiles`): its image, the
 * content version it shows, and when it was last drawn.
 */
class TileEntry(val image: ImageBitmap, val epoch: Long, var used: Long)

/** Tiles kept, about 1 MB each. */
private const val MAX_TILES = 64

/** How many hours behind "now" the hour track starts. */
const val PAST_HOURS = 6

/** How long the hour must stay put before a fine map is worked out, ms. */
private const val FINE_DELAY_MS = 280L

/** How long after the last move a pan or zoom counts as settled, ms. */
private const val SETTLE_MS = 180L

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
    /** Bumped whenever a tile arrives, so the map redraws. */
    val tiles: Int = 0,
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
    /** The map areas on screen: the card's, and the full-screen one while open. */
    private var cardScreen: Screen? = null
    private var fullScreen: Screen? = null
    private var colors: MapColors? = null
    /** Pixels per dp: line widths on the map. */
    private var density = 1f
    /** The held tiles, each the newest drawn; read by the map while drawing. */
    val tileCache = HashMap<Tile, TileEntry>()
    private var epoch = 0L
    private var uses = 0L
    private var tileBusy = false
    private var settleJob: Job? = null
    private var prefetchJob: Job? = null
    private var foreground = true
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
        _state.update { it.copy(reach = emptyList(), status = null) }
        // A selected point stays; its chance is worked out again.
        refreshProbe()
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
        _state.update { it.copy(band = band, hour = Math.floorMod(hour, 24)) }
        updateMap()
        refreshProbe()
    }

    /** The coarse grid at once, then the fine one, then a patch if zoomed. */
    private fun updateMap() {
        mapJob?.cancel()
        prefetchJob?.cancel()
        val generation = ++mapGeneration
        _state.update { it.copy(mapWorking = true, mapError = null) }
        mapChanged()
        val s = _state.value
        val hour = s.hour.toUInt()
        mapJob = viewModelScope.launch {
            val reach = withContext(Dispatchers.Default) { runCatching { core.coarseMaps(hour, s.now) } }
            if (generation != mapGeneration) return@launch
            _state.update { st -> reach.fold({ st.copy(reach = it) }, { st.copy(mapError = it.message) }) }
            mapChanged()
            // A fine map not yet held waits until the hour stays put: sliding
            // across the day then costs a coarse run per hour, not a fine one
            // each (a started run cannot be stopped), and the last stop wins.
            val held = withContext(Dispatchers.Default) { runCatching { core.hasFineMap(s.band, hour, s.now) }.getOrDefault(false) }
            if (!held) delay(FINE_DELAY_MS)
            if (generation != mapGeneration) return@launch
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
            mapChanged()
            prefetch()
        }
    }

    /**
     * Readies the rest of the day while the reader looks (Core.prefetch),
     * one map at a time, only while the app is on screen, and stops on any
     * new request. A step already running finishes (it cannot be stopped)
     * but on half the cores.
     */
    private fun prefetch() {
        prefetchJob?.cancel()
        if (!foreground) return
        val generation = mapGeneration
        prefetchJob = viewModelScope.launch {
            while (generation == mapGeneration && foreground && !_state.value.mapWorking) {
                val s = _state.value
                // Null: the whole day is ready, or it failed; either way, stop.
                withContext(Dispatchers.Default) {
                    runCatching { core.prefetch(s.band, s.hour.toUInt(), s.now) }.getOrNull()
                } ?: break
            }
        }
    }

    /** The app came on screen or left it. */
    fun setForeground(on: Boolean) {
        foreground = on
        if (on) prefetch() else prefetchJob?.cancel()
    }

    /**
     * The map's area on screen, in pixels, and the theme's colours. The
     * full-screen map sets its own over the card's while it is open, and
     * `null` there hands back to the card.
     */
    fun setMapScreen(screen: Screen?, colors: MapColors, density: Float, full: Boolean = false) {
        if (full) fullScreen = screen else cardScreen = screen
        if (colors != this.colors || density != this.density) {
            this.colors = colors
            this.density = density
            mapChanged()
        } else {
            pumpTiles()
        }
    }

    /** The map area tiles are chosen for. */
    fun activeScreen(): Screen? = fullScreen ?: cardScreen

    /**
     * A pan or zoom: the held tiles move at once and missing ones follow;
     * once the view settles (a short pause after the last move counts),
     * a patch is worked out for it.
     */
    fun setView(view: MapView, settled: Boolean) {
        _state.update { it.copy(view = view) }
        settleJob?.cancel()
        pumpTiles()
        if (!settled) {
            settleJob = viewModelScope.launch {
                delay(SETTLE_MS)
                setView(_state.value.view, settled = true)
            }
            return
        }
        val s = _state.value
        val generation = mapGeneration
        viewModelScope.launch {
            val patched = withContext(Dispatchers.Default) {
                runCatching { core.patchMap(s.hour.toUInt(), s.now, view) }.getOrDefault(false)
            }
            // A patch for this view (new or held) changes what its tiles show.
            if (patched && generation == mapGeneration) mapChanged()
        }
    }

    /** Zooms about the middle of the map's screen. */
    fun zoomBy(factor: Double) {
        val scr = activeScreen() ?: return
        setView(viewZoomedIn(_state.value.view, factor, scr.width / 2, scr.height / 2, scr), settled = true)
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
            // A second tap on the same cell clears it.
            _state.update { it.copy(probe = if (p != null && p.grid == it.probe?.grid) null else p) }
            mapChanged()
            refreshProbe()
        }
    }

    /** The selected point's own chance of contact, for the band and hour shown. */
    private fun refreshProbe() {
        val s = _state.value
        val p = s.probe ?: return
        viewModelScope.launch {
            val full = withContext(Dispatchers.Default) { core.probeAt(s.band, s.hour.toUInt(), s.now, p.lat, p.lon) }
            val now = _state.value
            if (now.probe?.grid == p.grid && now.band == s.band && now.hour == s.hour) {
                _state.update { it.copy(probe = full) }
                mapChanged()
            }
        }
    }

    /** Makes the selected point the path's destination. */
    fun probeToDestination() {
        val p = _state.value.probe ?: return
        _state.update { it.copy(probe = null) }
        setTo(core.placeAt(p.lat, p.lon))
    }

    fun toggleTable() = _state.update { it.copy(showTable = !it.showTable) }

    /**
     * What the map shows has changed (data, band, hour, colours, the
     * selection): every tile is now stale and is redrawn, those on screen
     * first. Stale tiles stay on screen until then.
     */
    private fun mapChanged() {
        epoch++
        pumpTiles()
    }

    /** One tick of the use clock, stamped on each tile drawn. */
    fun nextUse(): Long = ++uses

    /**
     * Draws the next tile the screen lacks, one at a time: the one in the
     * middle first; the whole-earth tile first when none is held at all, so
     * zooming out never shows empty space.
     */
    private fun pumpTiles() {
        if (tileBusy) return
        val screen = activeScreen() ?: return
        val c = colors ?: return
        if (screen.side < 32.0) return
        val s = _state.value
        val e = epoch
        val world = Tile(0u, 0u, 0u)
        val z = tileLevel(s.view, screen)
        val wanted = buildList {
            if (world !in tileCache) add(world)
            tilesVisible(s.view, screen, z).forEach { add(it.tile) }
            add(world)
        }
        val tile = wanted.firstOrNull { tileCache[it]?.epoch != e } ?: return
        tileBusy = true
        val quality = s.probe?.let { p -> p.chance?.let { qualityOf(it) } ?: p.quality }
        val options = MapOptions(
            showNight = true, showNvis = true, showPath = true, highlight = s.probe,
            highlightColor = semaphoreColor(quality, c.dark, c.lowLight),
        )
        viewModelScope.launch {
            val result = withContext(Dispatchers.Default) {
                runCatching {
                    val img = core.renderTile(s.band, s.hour.toUInt(), s.now, s.view, tile, density, c.palette, options)
                    val size = img.size.toInt()
                    val bitmap = createBitmap(size, size, Bitmap.Config.ARGB_8888)
                    // Premultiplied RGBA bytes: exactly ARGB_8888's memory layout.
                    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(img.pixels))
                    // Mipmaps keep a shrunken tile sharp when zoomed out.
                    bitmap.setHasMipMap(true)
                    img.detail to bitmap.asImageBitmap()
                }.getOrNull()
            }
            tileBusy = false
            if (result != null) {
                keepTile(tile, TileEntry(result.second, e, nextUse()))
                _state.update { it.copy(detail = result.first, tiles = it.tiles + 1) }
            }
            pumpTiles()
        }
    }

    private fun keepTile(tile: Tile, entry: TileEntry) {
        // A slow tile from an older version never replaces a newer one.
        val held = tileCache[tile]
        if (held != null && held.epoch > entry.epoch) return
        tileCache[tile] = entry
        while (tileCache.size > MAX_TILES) {
            val oldest = tileCache.entries.filter { it.key.z != 0u }.minByOrNull { it.value.used } ?: break
            tileCache.remove(oldest.key)
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
