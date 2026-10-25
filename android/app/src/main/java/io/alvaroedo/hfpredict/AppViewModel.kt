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
import io.alvaroedo.hfpredict.core.tileLineScale
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
 * fingerprint of the content it shows (the core's map signature with the
 * app's colours and selection), and when it was last drawn.
 */
class TileEntry(val image: ImageBitmap, val signature: Long, var used: Long)

/**
 * What the map draws from, apart from the rest of the screen so that a
 * gesture or an arriving tile redraws the map alone: the view, and a tick
 * bumped whenever the held tiles change.
 */
data class MapFrame(val view: MapView, val tick: Int = 0)

/**
 * The map at rest: one image drawn for exactly [screen] (pixels) at [view],
 * at full or (while the hour slides) half resolution. During a gesture it
 * is moved and scaled to the view on screen until a new one is drawn.
 */
class ScreenFrame(val image: ImageBitmap, val view: MapView, val screen: Screen)

/** Tiles kept, about 1 MB each. */
private const val MAX_TILES = 64

/** How many hours behind "now" the hour track starts. */
const val PAST_HOURS = 6

/** How long the hour must stay put before a fine map is worked out, ms. */
private const val FINE_DELAY_MS = 280L

/** How long after the last move a pan or zoom counts as settled, ms. */
private const val SETTLE_MS = 180L

/** Hour changes closer than this count as sliding: coarser tiles first, ms. */
private const val SCRUB_MS = 350L

/** Readying more of the day waits this long after the last touch, ms. */
private const val IDLE_MS = 2000L

val WHOLE_GLOBE = MapView(1.0, 0.5, 0.5)

data class UiState(
    val saved: SavedState,
    val band: Band,
    val hour: Int,
    val now: Long,
    val forecast: Forecast? = null,
    val forecastError: String? = null,
    val reach: List<BandReach> = emptyList(),
    val status: MapStatus? = null,
    val detail: MapDetail = MapDetail.NONE,
    val mapWorking: Boolean = false,
    val mapError: String? = null,
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

    private val _frame = MutableStateFlow(MapFrame(WHOLE_GLOBE))
    /** The map's own state (see [MapFrame]). */
    val frame: StateFlow<MapFrame> = _frame.asStateFlow()
    private val view: MapView get() = _frame.value.view

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
    /** The content fingerprint of the newest tiles; older ones are stale. */
    var signature = 0L
        private set
    private var uses = 0L
    private var scrubbing = false
    private var scrubJob: Job? = null
    private var lastSelect = 0L
    private var lastTouch = 0L
    private var tileBusy = false
    private var frameBusy = false
    private var frameAgain = false
    /** The map at rest: one image for exactly the screen. */
    var screenFrame: ScreenFrame? = null
        private set
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
        // Hours changing quickly (the slider, a drag along the grid): the
        // map shows coarser tiles until it stops.
        val t = android.os.SystemClock.uptimeMillis()
        if (t - lastSelect < SCRUB_MS) scrubbing = true
        lastSelect = t
        lastTouch = t
        scrubJob?.cancel()
        scrubJob = viewModelScope.launch {
            delay(SCRUB_MS)
            if (scrubbing) {
                scrubbing = false
                requestFrame()
            }
        }
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
                    runCatching { core.patchMap(hour, s.now, view) }
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
                // Never while the reader is at work: a step cannot be stopped
                // once started, and would slow their own maps.
                val idle = android.os.SystemClock.uptimeMillis() - lastTouch
                if (idle < IDLE_MS) {
                    delay(IDLE_MS - idle)
                    continue
                }
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
        this.colors = colors
        this.density = density
        requestFrame()
    }

    /** The map area tiles are chosen for. */
    fun activeScreen(): Screen? = fullScreen ?: cardScreen

    /**
     * A pan or zoom: the last frame and the held tiles move at once (missing
     * tiles follow); once the view settles (a short pause after the last
     * move counts), a sharp frame and a patch are worked out for it.
     */
    fun setView(view: MapView, settled: Boolean) {
        lastTouch = android.os.SystemClock.uptimeMillis()
        _frame.update { it.copy(view = view) }
        settleJob?.cancel()
        if (!settled) {
            pumpTiles()
            settleJob = viewModelScope.launch {
                delay(SETTLE_MS)
                setView(view, settled = true)
            }
            return
        }
        requestFrame()
        val s = _state.value
        val generation = mapGeneration
        viewModelScope.launch {
            val patched = withContext(Dispatchers.Default) {
                runCatching { core.patchMap(s.hour.toUInt(), s.now, view) }.getOrDefault(false)
            }
            // A patch for this view (new or held) changes what it shows.
            if (patched && generation == mapGeneration) mapChanged()
        }
    }

    /** Zooms about the middle of the map's screen. */
    fun zoomBy(factor: Double) {
        val scr = activeScreen() ?: return
        setView(viewZoomedIn(view, factor, scr.width / 2, scr.height / 2, scr), settled = true)
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
            val v = view
            val p = withContext(Dispatchers.Default) { core.probe(s.band, s.hour.toUInt(), s.now, v, x, y, size) }
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
            val v = view
            val full = withContext(Dispatchers.Default) { core.probeAt(s.band, s.hour.toUInt(), s.now, v, p.lat, p.lon) }
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
     * What the map shows may have changed (data, band, hour, colours, the
     * selection): a new frame is drawn, and replaces the old one whole.
     */
    private fun mapChanged() = requestFrame()

    /** One tick of the use clock, stamped on each tile drawn. */
    fun nextUse(): Long = ++uses

    /** The app's own part of a picture's look: colours, lines, selection, path. */
    private fun appKey(options: MapOptions, palette: io.alvaroedo.hfpredict.core.Palette): Long =
        java.util.Objects.hash(options, palette, density).toLong()

    /** What a picture of the map is drawn from now. */
    private class Inputs(val s: UiState, val view: MapView, val palette: io.alvaroedo.hfpredict.core.Palette, val options: MapOptions, val key: Long)

    private fun inputs(c: MapColors): Inputs {
        val s = _state.value
        val quality = s.probe?.let { p -> p.chance?.let { qualityOf(it) } ?: p.quality }
        val options = MapOptions(
            showNight = true, showNvis = true, showPath = true, highlight = s.probe,
            highlightColor = semaphoreColor(quality, c.dark, c.lowLight),
        )
        // The path in the traffic-light colour of the chance to its end.
        val toCell = s.forecast?.takeIf { it.to != null }?.cells?.firstOrNull { it.band == s.band && it.hour.toInt() == s.hour }
        val palette = if (toCell == null) c.palette
        else c.palette.copy(path = semaphoreColor(qualityOf(toCell.reliability), c.dark, c.lowLight))
        return Inputs(s, view, palette, options, appKey(options, palette))
    }

    private fun bitmapOf(img: io.alvaroedo.hfpredict.core.MapImage, mipmaps: Boolean): ImageBitmap {
        val bitmap = createBitmap(img.width.toInt(), img.height.toInt(), Bitmap.Config.ARGB_8888)
        // Premultiplied RGBA bytes: exactly ARGB_8888's memory layout.
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(img.pixels))
        if (mipmaps) bitmap.setHasMipMap(true)
        return bitmap.asImageBitmap()
    }

    /**
     * Draws the map for exactly the screen, sharp and whole, and shows it
     * in one go. One at a time; requests meanwhile fold into one more, so
     * sliding through the hours shows the latest it can, never a mix. While
     * the hour slides it is drawn at half the pixels, sharpened once it stops.
     */
    private fun requestFrame() {
        val screen = activeScreen() ?: return
        val c = colors ?: return
        if (screen.side < 32.0) return
        if (frameBusy) {
            frameAgain = true
            return
        }
        frameBusy = true
        val i = inputs(c)
        val k = if (scrubbing) 0.5 else 1.0
        val drawn = Screen(screen.side * k, screen.x0 * k, screen.y0 * k, screen.width * k, screen.height * k)
        val t0 = android.os.SystemClock.elapsedRealtime()
        viewModelScope.launch {
            val result = withContext(Dispatchers.Default) {
                runCatching {
                    val content = core.mapSignature(i.s.band, i.s.hour.toUInt(), i.s.now, i.view).toLong()
                    val img = core.renderScreen(i.s.band, i.s.hour.toUInt(), i.s.now, i.view, drawn, density * k.toFloat(), i.palette, i.options)
                    Triple(content, img.detail, bitmapOf(img, mipmaps = false))
                }.getOrNull()
            }
            frameBusy = false
            if (BuildConfig.DEBUG) {
                android.util.Log.d("HFPredict", "frame ${drawn.width.toInt()}x${drawn.height.toInt()}: ${android.os.SystemClock.elapsedRealtime() - t0} ms")
            }
            if (result != null) {
                signature = result.first * 31 + i.key
                screenFrame = ScreenFrame(result.third, i.view, screen)
                if (_state.value.detail != result.second) _state.update { it.copy(detail = result.second) }
                _frame.update { it.copy(tick = it.tick + 1) }
            }
            if (frameAgain) {
                frameAgain = false
                requestFrame()
            } else {
                // Ready tiles for the next gesture while nothing else runs.
                pumpTiles()
            }
        }
    }

    /**
     * Draws the next tile a gesture would need, one at a time and only when
     * no frame is being drawn: the one in the middle first, the whole-earth
     * tile first when none is held, so a pan or a zoom never shows empty
     * space. Tiles of older content are never shown.
     */
    private fun pumpTiles() {
        if (tileBusy || frameBusy) return
        val screen = activeScreen() ?: return
        val c = colors ?: return
        if (screen.side < 32.0) return
        tileBusy = true
        val i = inputs(c)
        val z = tileLevel(i.view, screen)
        val world = Tile(0u, 0u, 0u)
        viewModelScope.launch {
            val sig = withContext(Dispatchers.Default) {
                runCatching { core.mapSignature(i.s.band, i.s.hour.toUInt(), i.s.now, i.view).toLong() }.getOrDefault(0L)
            } * 31 + i.key
            if (sig != signature) {
                // The content moved on: the next frame comes first.
                tileBusy = false
                requestFrame()
                return@launch
            }
            val wanted = buildList {
                add(world)
                tilesVisible(i.view, screen, z).forEach { add(it.tile) }
            }
            val tile = wanted.firstOrNull { tileCache[it]?.signature != sig }
            if (tile == null) {
                tileBusy = false
                return@launch
            }
            // Lines thicker by as much as the tile will be shrunk.
            val lines = density * tileLineScale(i.view, screen, tile.z).toFloat()
            val image = withContext(Dispatchers.Default) {
                runCatching {
                    bitmapOf(core.renderTile(i.s.band, i.s.hour.toUInt(), i.s.now, i.view, tile, lines, i.palette, i.options), mipmaps = true)
                }.getOrNull()
            }
            tileBusy = false
            if (image != null) {
                keepTile(tile, TileEntry(image, sig, nextUse()))
                _frame.update { it.copy(tick = it.tick + 1) }
            }
            pumpTiles()
        }
    }

    private fun keepTile(tile: Tile, entry: TileEntry) {
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
