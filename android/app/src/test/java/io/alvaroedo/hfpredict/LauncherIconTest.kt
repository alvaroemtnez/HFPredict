package io.alvaroedo.hfpredict

import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

class LauncherIconTest {
    /** The activity-aliases the manifest declares, as full class names. */
    private fun manifestAliases(): Set<String> {
        val manifest = File("src/main/AndroidManifest.xml").readText()
        return Regex("""<activity-alias\s[^>]*android:name="([^"]+)"""")
            .findAll(manifest)
            .map { it.groupValues[1] }
            .map { if (it.startsWith(".")) BuildConfig.NAMESPACE + it else it }
            .toSet()
    }

    @Test
    fun everyIconNamesAnAliasInTheManifest() {
        assertEquals(manifestAliases(), LauncherIcon.entries.map { it.className }.toSet())
    }

    @Test
    fun aliasesLiveInTheCodeNamespaceNotTheApplicationId() {
        assertEquals(LauncherIcon::class.java.`package`!!.name, BuildConfig.NAMESPACE)
    }
}
