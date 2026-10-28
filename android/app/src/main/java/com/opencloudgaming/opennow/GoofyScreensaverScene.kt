package com.opencloudgaming.opennow

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.isActive
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import kotlin.random.Random

/** Manual playback uses the same scene without changing the saved idle-screen setting. */
@Composable
internal fun GoofyScreensaverPreview(onDismiss: () -> Unit) {
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        val focus = remember { FocusRequester() }
        LaunchedEffect(focus) { focus.requestFocus() }
        Box(
            Modifier.fillMaxSize().background(Color(0xFF101719))
                .onPreviewKeyEvent {
                    if (it.type == KeyEventType.KeyDown) onDismiss()
                    true
                }
                .focusRequester(focus).focusable().clickable(onClick = onDismiss),
        ) {
            GoofyScreensaverScene()
            Text(
                stringResource(R.string.screensaver_dismiss),
                modifier = Modifier.align(Alignment.BottomCenter).padding(24.dp),
                color = Color(0xFFB6C6C4),
                fontSize = 12.sp,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
internal fun GoofyScreensaverScene() {
    val messages = listOf(
        stringResource(R.string.screensaver_discord),
        stringResource(R.string.screensaver_logs),
        stringResource(R.string.screensaver_open_source),
    )
    val movie = rememberGoofyMovie()
    BoxWithConstraints(
        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)
            .clipToBounds().clearAndSetSemantics {},
    ) {
        val width = maxWidth.value
        val height = maxHeight.value
        val logo = remember(width, height) {
            MascotMotion(
                Random.nextFloat() * (width - MASCOT_SIZE_DP).coerceAtLeast(0f),
                Random.nextFloat() * (height - MASCOT_SIZE_DP).coerceAtLeast(0f), 0f, 0f,
            ).apply { randomizeVelocity() }
        }
        val companion = remember(logo) { createMascotCompanionMotion(logo, width, height) }
        var elapsed by remember(logo) { mutableLongStateOf(0L) }
        var placement by remember(logo) { mutableStateOf(nextGoofyEdgePlacement()) }
        LaunchedEffect(logo) {
            var previous = withFrameNanos { it }
            var activeNanos = 0L
            while (isActive) {
                withFrameNanos { now ->
                    // Active time pauses across long frames instead of skipping the punchline on resume.
                    val delta = (now - previous).coerceIn(0L, 50_000_000L)
                    previous = now
                    val oldCycle = elapsed / GOOFY_SCENE_DURATION_MILLIS
                    activeNanos += delta
                    elapsed = activeNanos / 1_000_000L
                    if (elapsed / GOOFY_SCENE_DURATION_MILLIS != oldCycle) {
                        placement = nextGoofyEdgePlacement(placement)
                    }
                    logo.advance(width, height, delta / 1_000_000_000f)
                    if (goofySceneCue(elapsed).companionAiming) {
                        companion.x = companion.x.coerceIn(0f, (width - MASCOT_SIZE_DP).coerceAtLeast(0f))
                    } else {
                        companion.advance(width, height, delta / 1_000_000_000f)
                    }
                }
            }
        }
        // Reading the clock here also publishes the mutable motion positions once per frame.
        val cue = goofySceneCue(elapsed)
        val logoX = logo.x
        val logoY = logo.y
        val companionX = companion.x
        val companionY = companion.y
        val peeker = movie?.let { placement.frame(width, height, it.width().toFloat(), it.height().toFloat()) }
        if (movie != null && peeker != null && cue.peekerReveal > 0f) {
            GoofyEdgeAnimation(movie, peeker, placement.edge, cue.peekerReveal)
        }
        Image(
            painterResource(R.drawable.oniht), null,
            modifier = Modifier.size(MASCOT_SIZE_DP.dp).graphicsLayer {
                translationX = companionX.dp.toPx()
                translationY = companionY.dp.toPx()
                rotationZ = if (cue.companionAiming) -8f else 0f
            },
            contentScale = ContentScale.Fit,
        )
        Image(
            painterResource(R.drawable.opennow_icon), null,
            modifier = Modifier.size(MASCOT_SIZE_DP.dp).graphicsLayer {
                translationX = logoX.dp.toPx()
                translationY = logoY.dp.toPx()
            },
            contentScale = ContentScale.Fit,
        )
        if (peeker != null) {
            cue.lightningProgress?.let { progress ->
                MascotLightning(
                    Offset(companionX + MASCOT_SIZE_DP / 2f, companionY + MASCOT_SIZE_DP / 2f),
                    Offset(peeker.centerX, peeker.centerY), progress,
                )
            }
            if (cue.showBugMessage) {
                MascotSpeechBubble(
                    stringResource(R.string.screensaver_bug), Color(0xFFFFC878), width, height,
                    peeker.centerX, peeker.centerY, MASCOT_SIZE_DP / 2f,
                )
            }
        }
        MascotSpeechBubble(
            messages[((elapsed / GOOFY_LOGO_MESSAGE_DURATION_MILLIS) % messages.size).toInt()],
            Color(0xFFB9F584), width, height,
            logoX + MASCOT_SIZE_DP / 2f, logoY + MASCOT_SIZE_DP / 2f, MASCOT_SIZE_DP / 2f,
        )
        MascotCatchCounter(
            catches = if (peeker != null) goofyCatchCount(elapsed) else 0L,
            viewportWidth = width, viewportHeight = height,
            snowmanX = companionX, snowmanY = companionY,
        )
    }
}

@Composable
private fun MascotLightning(from: Offset, to: Offset, progress: Float) {
    Canvas(Modifier.fillMaxSize()) {
        val start = Offset(from.x.dp.toPx(), from.y.dp.toPx())
        val target = Offset(to.x.dp.toPx(), to.y.dp.toPx())
        val end = start + (target - start) * (progress / 0.5f).coerceIn(0f, 1f)
        val vector = end - start
        val length = vector.getDistance().coerceAtLeast(1f)
        val normal = Offset(-vector.y / length, vector.x / length)
        val zigzag = (length * 0.045f).coerceIn(5.dp.toPx(), 30.dp.toPx())
        val path = Path().apply {
            moveTo(start.x, start.y)
            for (i in 1..9) {
                val point = start + vector * (i / 10f) + normal * (if (i % 2 == 0) -zigzag else zigzag)
                lineTo(point.x, point.y)
            }
            lineTo(end.x, end.y)
        }
        val alpha = if (progress < 0.75f) 1f else (1f - progress) / 0.25f
        drawPath(path, Color(0xFFFF9A24).copy(alpha = alpha * 0.25f), style = Stroke(12.dp.toPx(), cap = StrokeCap.Round))
        drawPath(path, Color(0xFFFFB52E).copy(alpha = alpha), style = Stroke(4.dp.toPx(), cap = StrokeCap.Round))
        drawPath(path, Color(0xFFFFF4C2).copy(alpha = alpha), style = Stroke(1.5.dp.toPx(), cap = StrokeCap.Round))
        if (progress >= 0.5f) {
            val radius = (8f + (progress - 0.5f) * 36f).dp.toPx()
            repeat(6) { index ->
                val angle = index * PI.toFloat() / 3f
                val direction = Offset(cos(angle), sin(angle))
                drawLine(Color(0xFFFFD783).copy(alpha = alpha), target + direction * radius * 0.5f,
                    target + direction * radius, 2.dp.toPx(), StrokeCap.Round)
            }
        }
    }
}

@Composable
private fun MascotSpeechBubble(
    text: String,
    accent: Color,
    viewportWidth: Float,
    viewportHeight: Float,
    speakerX: Float,
    speakerY: Float,
    speakerHalfHeight: Float,
) {
    val density = LocalDensity.current
    var measured by remember { mutableStateOf(IntSize.Zero) }
    val position = mascotBubblePlacement(
        viewportWidth, viewportHeight,
        with(density) { measured.width.toDp().value }, with(density) { measured.height.toDp().value },
        speakerX, speakerY, speakerHalfHeight,
    )
    val shape = RoundedCornerShape(14.dp)
    val bubbleColor = Color(0xFF1C292C)
    Box(
        Modifier.widthIn(max = minOf(220.dp, viewportWidth.dp))
            .onSizeChanged { measured = it }
            .graphicsLayer {
                translationX = position.x.dp.toPx()
                translationY = position.y.dp.toPx()
            }
            .shadow(4.dp, shape, clip = false)
            .drawBehind {
                val tailX = position.tailX.dp.toPx()
                val edgeY = if (position.above) size.height else 0f
                val tipY = edgeY + (if (position.above) 8.dp.toPx() else -8.dp.toPx())
                val tail = Path().apply {
                    moveTo(tailX - 6.dp.toPx(), edgeY)
                    lineTo(tailX, tipY)
                    lineTo(tailX + 6.dp.toPx(), edgeY)
                    close()
                }
                drawPath(tail, bubbleColor)
                drawPath(tail, accent.copy(alpha = 0.5f), style = Stroke(1.dp.toPx()))
            }
            .background(bubbleColor, shape).border(1.dp, accent.copy(alpha = 0.5f), shape)
            .padding(horizontal = 13.dp, vertical = 10.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(5.dp).background(accent, CircleShape))
            Text(text, color = Color(0xFFF1F6F3), fontSize = 13.sp, lineHeight = 18.sp,
                fontWeight = FontWeight.Medium, textAlign = TextAlign.Center)
        }
    }
}
