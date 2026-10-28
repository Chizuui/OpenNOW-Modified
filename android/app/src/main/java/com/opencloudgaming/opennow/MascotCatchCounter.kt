package com.opencloudgaming.opennow

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
internal fun MascotCatchCounter(catches: Long, viewportWidth: Float, viewportHeight: Float, snowmanX: Float, snowmanY: Float) {
    val context = LocalContext.current
    // The supplied logo is 1505px wide. Decode a small version once, rather than a full bitmap per frame.
    val logo = remember(context) {
        BitmapFactory.decodeResource(context.resources, R.drawable.zortos_mascot,
            BitmapFactory.Options().apply { inSampleSize = 8; inScaled = false })?.asImageBitmap()
    }
    val density = LocalDensity.current
    var measured by remember { mutableStateOf(IntSize.Zero) }
    val position = mascotBubblePlacement(
        viewportWidth, viewportHeight,
        with(density) { measured.width.toDp().value }, with(density) { measured.height.toDp().value },
        snowmanX + MASCOT_SIZE_DP / 2f, snowmanY + MASCOT_SIZE_DP / 2f, MASCOT_SIZE_DP / 2f,
    )
    val shape = RoundedCornerShape(12.dp)
    Row(
        modifier = Modifier.widthIn(max = viewportWidth.dp)
            .onSizeChanged { measured = it }
            .graphicsLayer {
                translationX = position.x.dp.toPx()
                translationY = position.y.dp.toPx()
            }
            .background(Color(0xFF1C292C), shape)
            .border(1.dp, Color(0xFFBFA9ED).copy(alpha = 0.6f), shape)
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (logo != null) Image(logo, contentDescription = null, modifier = Modifier.size(24.dp))
        Text(stringResource(R.string.screensaver_catch_count, catches),
            color = Color(0xFFF1F6F3), fontSize = 12.sp, fontFamily = FontFamily.Monospace,
            fontWeight = FontWeight.SemiBold, maxLines = 1)
    }
}
