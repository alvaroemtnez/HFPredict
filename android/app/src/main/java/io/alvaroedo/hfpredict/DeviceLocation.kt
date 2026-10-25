package io.alvaroedo.hfpredict

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import android.os.Looper
import androidx.core.content.ContextCompat
import androidx.core.location.LocationListenerCompat
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.Executors
import kotlin.coroutines.resume

/**
 * The device's position from the platform's own location service: no
 * Google Play services, so it works on GrapheneOS and other de-Googled
 * systems. Coarse permission only; a grid square needs no more.
 */
object DeviceLocation {
    sealed interface Result {
        data class Found(val lat: Double, val lon: Double) : Result
        data object NoPermission : Result
        data object Disabled : Result
        data object Unavailable : Result
    }

    fun hasPermission(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    /** A recent last-known fix if there is one, otherwise a fresh one. */
    @SuppressLint("MissingPermission")
    suspend fun locate(context: Context): Result {
        if (!hasPermission(context)) return Result.NoPermission
        val lm = context.getSystemService(LocationManager::class.java) ?: return Result.Unavailable
        val providers = buildList {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(LocationManager.FUSED_PROVIDER)
            add(LocationManager.NETWORK_PROVIDER)
            add(LocationManager.GPS_PROVIDER)
        }.filter { runCatching { lm.isProviderEnabled(it) }.getOrDefault(false) }
        if (providers.isEmpty()) return Result.Disabled

        val recent = providers
            .mapNotNull { runCatching { lm.getLastKnownLocation(it) }.getOrNull() }
            .filter { System.currentTimeMillis() - it.time < 30 * 60_000L }
            .maxByOrNull { it.time }
        if (recent != null) return Result.Found(recent.latitude, recent.longitude)

        for (provider in providers) {
            val fix = withTimeoutOrNull(20_000) { current(lm, provider) }
            if (fix != null) return Result.Found(fix.latitude, fix.longitude)
        }
        return Result.Unavailable
    }

    @SuppressLint("MissingPermission")
    private suspend fun current(lm: LocationManager, provider: String): Location? =
        suspendCancellableCoroutine { cont ->
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val cancel = CancellationSignal()
                cont.invokeOnCancellation { cancel.cancel() }
                lm.getCurrentLocation(provider, cancel, Executors.newSingleThreadExecutor()) { loc ->
                    if (cont.isActive) cont.resume(loc)
                }
            } else {
                val listener = object : LocationListenerCompat {
                    override fun onLocationChanged(location: Location) {
                        lm.removeUpdates(this)
                        if (cont.isActive) cont.resume(location)
                    }
                }
                cont.invokeOnCancellation { lm.removeUpdates(listener) }
                @Suppress("DEPRECATION")
                lm.requestSingleUpdate(provider, listener, Looper.getMainLooper())
            }
        }
}
