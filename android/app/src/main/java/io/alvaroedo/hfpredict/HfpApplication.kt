package io.alvaroedo.hfpredict

import android.app.Application
import io.alvaroedo.hfpredict.core.Core

/** Holds the one [Core]: saved state, caches and the engine, shared app-wide. */
class HfpApplication : Application() {
    val core: Core by lazy { Core(filesDir.absolutePath, cacheDir.absolutePath) }
}
