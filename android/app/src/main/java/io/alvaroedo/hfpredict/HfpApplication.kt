package io.alvaroedo.hfpredict

import android.app.Application
import io.alvaroedo.hfpredict.core.Core

/** Holds the one [Core]: saved state, caches and the engine, shared app-wide. */
class HfpApplication : Application() {
    val core: Core by lazy { Core(filesDir.absolutePath, cacheDir.absolutePath) }

    override fun onCreate() {
        super.onCreate()
        // The time-zone map takes a moment to load: load it before it is needed.
        Thread { io.alvaroedo.hfpredict.core.localClock(0.0, 0.0, 0L) }.start()
    }
}
