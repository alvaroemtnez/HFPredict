package io.alvaroedo.hfpredict.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.alvaroedo.hfpredict.R
import io.alvaroedo.hfpredict.core.Quality
import io.alvaroedo.hfpredict.description
import io.alvaroedo.hfpredict.label
import io.alvaroedo.hfpredict.tr

@Composable
fun SectionCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    ElevatedCard(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(24.dp),
        colors = CardDefaults.elevatedCardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
        elevation = CardDefaults.elevatedCardElevation(defaultElevation = 0.dp),
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
    }
}

/** "Reliable", in the bucket's own colour. */
@Composable
fun QualityPill(q: Quality) {
    val colors = LocalMapColors.current
    val bg = colors.cell(q)
    val fg = if (q == Quality.CLOSED) MaterialTheme.colorScheme.onSurface
    else if (bg.luminance() > 0.45f) Color.Black.copy(alpha = 0.87f) else Color.White
    Surface(shape = RoundedCornerShape(50), color = bg, contentColor = fg) {
        Text(
            tr(q.label()),
            Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.Bold,
        )
    }
}

private fun Color.luminance(): Float = 0.2126f * red + 0.7152f * green + 0.0722f * blue

/** Small colour squares and a word, for legends. */
@Composable
fun Swatches(colors: List<Color>, text: String, round: Boolean = false, outline: Color? = null) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            colors.forEach { c ->
                Canvas(Modifier.size(12.dp)) {
                    val r = if (round) size.minDimension / 2 else 3.dp.toPx()
                    drawRoundRect(c, cornerRadius = CornerRadius(r, r))
                    if (outline != null) drawRoundRect(outline, cornerRadius = CornerRadius(r, r), style = Stroke(1.dp.toPx()))
                }
            }
        }
        Text(text, style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
fun QualityLegend() {
    val colors = LocalMapColors.current
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        listOf(Quality.RELIABLE, Quality.PATCHY, Quality.WEAK, Quality.CLOSED).forEach { q ->
            Swatches(
                listOf(colors.cell(q)),
                tr(q.label()),
                outline = if (q == Quality.CLOSED) MaterialTheme.colorScheme.outline else null,
            )
        }
    }
}

@Composable
fun qualityHint(q: Quality): String = tr(q.description())

@Composable
fun Kicker(text: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Text(text.uppercase(), modifier, style = MaterialTheme.typography.labelMedium, color = color, fontWeight = FontWeight.Bold)
}

/**
 * A screen over the main one: full screen on a phone, a dialog on a tablet
 * or a large window, as Material recommends.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FullScreenDialog(
    title: String,
    onDismiss: () -> Unit,
    compact: Boolean,
    actions: @Composable () -> Unit = {},
    maxWidth: Dp = 600.dp,
    content: @Composable (Modifier) -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        val shape = if (compact) RoundedCornerShape(0.dp) else RoundedCornerShape(28.dp)
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Surface(
                modifier = if (compact) Modifier.fillMaxSize()
                else Modifier.widthIn(max = maxWidth).fillMaxWidth(0.9f).fillMaxSize(0.9f),
                shape = shape,
                color = MaterialTheme.colorScheme.surfaceContainerLow,
            ) {
                Scaffold(
                    containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                    topBar = {
                        TopAppBar(
                            title = { Text(title) },
                            navigationIcon = {
                                IconButton(onClick = onDismiss) {
                                    Icon(painterResource(R.drawable.ic_close), contentDescription = tr(R.string.common_close))
                                }
                            },
                            actions = { actions() },
                        )
                    },
                ) { padding -> content(Modifier.padding(padding)) }
            }
        }
    }
}
