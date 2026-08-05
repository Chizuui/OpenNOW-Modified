package com.opencloudgaming.opennow.screens.tv

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.FavoriteBorder
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.opencloudgaming.opennow.isTvActivateKey
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.GameInfo
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import kotlinx.coroutines.delay

/**
 * TV components built from the Google TV Design Kit (see `docs/figma-design-kits-analysis.md`).
 *
 * Everything here is self-contained: it takes a [GameInfo] plus plain lambdas, so it can be
 * dropped into Home, Library or a future TV-only shell without touching the ViewModel layer.
 * Focus is handled per-component with the kit's conventions — a blue selection ring and a gentle
 * scale-up — which also gives D-pad users an explicit focus target (Compose `clickable` is
 * focusable by default and D-pad CENTER triggers it).
 */

private fun GameInfo.thumbUrl(): String? = tvBannerUrl ?: tvCardImageUrl ?: imageUrl ?: screenshotUrl

private fun GameInfo.subtitle(): String {
    val genres = genres.take(2).joinToString(" · ").ifBlank { null }
    return listOfNotNull(genres, publisherName).joinToString(" · ")
}

/**
 * The kit's signature list: full-width rows whose actions fade in when the row is focused.
 * Reads like a settings list but sized for a 3-metre viewing distance.
 */
@Composable
fun TvImmersiveList(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier,
        contentPadding = PaddingValues(vertical = OpenNowSpacing.sm),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        items(games, key = { it.id }) { game ->
            TvImmersiveListItem(
                game = game,
                favorite = game.id in favoriteIds,
                onSelect = { onSelect(game) },
                onFavorite = { onFavorite(game.id) },
                onPlay = { onPlay(game) },
            )
        }
    }
}

@Composable
private fun TvImmersiveListItem(
    game: GameInfo,
    favorite: Boolean,
    onSelect: () -> Unit,
    onFavorite: () -> Unit,
    onPlay: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(
        targetValue = if (focused) 1.03f else 1f,
        label = "tv-immersive-scale",
    )
    val thumbShape = RoundedCornerShape(OpenNowRadius.sm)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .zIndex(if (focused) 1f else 0f)
            .graphicsLayer { scaleX = scale; scaleY = scale }
            .clip(RoundedCornerShape(OpenNowRadius.md))
            .background(if (focused) TvPalette.SurfaceElevated else Color.Transparent)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .clickable(onClick = onSelect)
            .padding(horizontal = OpenNowSpacing.lg, vertical = OpenNowSpacing.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.xl),
    ) {
        Box(
            modifier = Modifier
                .width(208.dp)
                .aspectRatio(16f / 9f)
                .clip(thumbShape)
                .background(OpenNowPalette.ImagePlaceholder),
        ) {
            game.thumbUrl()?.let { url ->
                AsyncImage(
                    model = url,
                    contentDescription = game.title,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                )
            }
            if (focused) {
                Box(Modifier.fillMaxSize().border(2.dp, TvPalette.FocusPrimary, thumbShape))
            }
        }
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = game.title,
                style = TvTypography.Title,
                color = OpenNowPalette.TextPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val subtitle = game.subtitle()
            if (subtitle.isNotBlank()) {
                Text(
                    text = subtitle,
                    style = TvTypography.Subtitle,
                    color = OpenNowPalette.TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        AnimatedVisibility(
            visible = focused,
            enter = fadeIn(tween(OpenNowMotion.DurationStandard)),
            exit = fadeOut(tween(OpenNowMotion.DurationFast)),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
            ) {
                TvLongButton(
                    text = stringResource(R.string.action_play),
                    onClick = onPlay,
                    icon = Icons.Filled.PlayArrow,
                )
                TvIconActionButton(
                    onClick = onFavorite,
                    icon = if (favorite) Icons.Filled.Favorite else Icons.Filled.FavoriteBorder,
                    tint = if (favorite) OpenNowPalette.AccentDefault else OpenNowPalette.TextMuted,
                    contentDescription = stringResource(
                        if (favorite) R.string.controller_action_unfavorite else R.string.controller_action_favorite,
                    ),
                )
            }
        }
    }
}

/**
 * The kit's "Long button": a wide, rounded CTA that reads clearly from across the room.
 * Fills with the TV focus blue when focused.
 */
@Composable
fun TvLongButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(OpenNowRadius.lg)
    val scale by animateFloatAsState(
        targetValue = if (focused) 1.05f else 1f,
        label = "tv-long-button-scale",
    )
    Surface(
        modifier = modifier
            .graphicsLayer { scaleX = scale; scaleY = scale }
            .onFocusChanged { focused = it.isFocused }
            .clickable(onClick = onClick),
        shape = shape,
        color = if (focused) TvPalette.FocusPrimary else OpenNowPalette.Panel,
        contentColor = if (focused) Color.White else OpenNowPalette.TextPrimary,
        border = if (focused) BorderStroke(2.dp, Color.White.copy(alpha = 0.9f)) else null,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 32.dp, vertical = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null) {
                Icon(icon, contentDescription = null, modifier = Modifier.size(28.dp))
            }
            Text(text, style = TvTypography.Action, maxLines = 1)
        }
    }
}

/** Square icon action (favorite toggle) sized for D-pad reachability. */
@Composable
private fun TvIconActionButton(
    onClick: () -> Unit,
    icon: ImageVector,
    tint: Color,
    contentDescription: String?,
) {
    var focused by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(
        targetValue = if (focused) 1.08f else 1f,
        label = "tv-icon-action-scale",
    )
    Box(
        modifier = Modifier
            .size(64.dp)
            .graphicsLayer { scaleX = scale; scaleY = scale }
            .clip(RoundedCornerShape(OpenNowRadius.lg))
            .background(if (focused) TvPalette.FocusPrimary else OpenNowPalette.Panel)
            .onFocusChanged { focused = it.isFocused }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            tint = if (focused) Color.White else tint,
            modifier = Modifier.size(28.dp),
        )
    }
}

/**
 * The kit's "Wide standard" card: a 16:9 landscape poster with a title gradient.
 * Used on TV wherever a compact grid tile would be lost against bright artwork.
 *
 * [width] lets the caller size it inside a rail's fitted-card layout; without it the card fills
 * the available width. While focused, Play/Favorite actions fade in at the bottom-right, and a
 * long-press on the card opens the store picker (mirroring the existing rail cards).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TvWideCard(
    game: GameInfo,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    width: Dp? = null,
    favorite: Boolean = false,
    onFavorite: (() -> Unit)? = null,
    onPlay: (() -> Unit)? = null,
    onChooseStore: (() -> Unit)? = null,
) {
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val scale by animateFloatAsState(
        targetValue = if (focused) 1.06f else 1f,
        label = "tv-wide-card-scale",
    )
    val shape = RoundedCornerShape(OpenNowRadius.md)
    val showActions = focused && (onPlay != null || onFavorite != null)
    Box(
        modifier = modifier
            .then(if (width != null) Modifier.width(width) else Modifier.fillMaxWidth())
            .aspectRatio(16f / 9f)
            .zIndex(if (focused) 1f else 0f)
            .graphicsLayer { scaleX = scale; scaleY = scale }
            .clip(shape)
            .background(OpenNowPalette.ImagePlaceholder)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            // Mirrors the existing rail cards: intercept TV remote activate keys and route D-pad
            // moves through the focus manager so hero → rail → grid navigation stays consistent.
            .onPreviewKeyEvent { event ->
                when {
                    isTvActivateKey(event) -> {
                        onClick()
                        true
                    }
                    event.type == KeyEventType.KeyDown -> {
                        val direction = when (event.key) {
                            Key.DirectionUp -> FocusDirection.Up
                            Key.DirectionDown -> FocusDirection.Down
                            Key.DirectionLeft -> FocusDirection.Left
                            Key.DirectionRight -> FocusDirection.Right
                            else -> null
                        }
                        if (direction != null) focusManager.moveFocus(direction) else false
                    }
                    else -> false
                }
            }
            .combinedClickable(
                onClick = onClick,
                onLongClick = { onChooseStore?.invoke() },
                onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
            ),
    ) {
        game.thumbUrl()?.let { url ->
            AsyncImage(
                model = url,
                contentDescription = game.title,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
            )
        }
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(Color.Transparent, Color.Black.copy(alpha = 0.85f)),
                    ),
                ),
        )
        if (focused) {
            Box(Modifier.fillMaxSize().border(3.dp, TvPalette.FocusPrimary, shape))
        }
        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(OpenNowSpacing.lg),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = game.title,
                style = TvTypography.Title,
                color = Color.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val subtitle = game.subtitle()
            if (subtitle.isNotBlank()) {
                Text(
                    text = subtitle,
                    style = TvTypography.Subtitle,
                    color = Color.White.copy(alpha = 0.8f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        AnimatedVisibility(
            visible = showActions,
            enter = fadeIn(tween(OpenNowMotion.DurationStandard)),
            exit = fadeOut(tween(OpenNowMotion.DurationFast)),
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(OpenNowSpacing.md),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (onPlay != null) {
                    TvIconActionButton(
                        onClick = onPlay,
                        icon = Icons.Filled.PlayArrow,
                        tint = OpenNowPalette.TextPrimary,
                        contentDescription = stringResource(R.string.action_play),
                    )
                }
                if (onFavorite != null) {
                    TvIconActionButton(
                        onClick = onFavorite,
                        icon = if (favorite) Icons.Filled.Favorite else Icons.Filled.FavoriteBorder,
                        tint = if (favorite) OpenNowPalette.AccentDefault else OpenNowPalette.TextMuted,
                        contentDescription = stringResource(
                            if (favorite) R.string.controller_action_unfavorite else R.string.controller_action_favorite,
                        ),
                    )
                }
            }
        }
    }
}

/**
 * The kit's Featured Carousel: one full-bleed hero at a time, auto-advancing every few seconds
 * (paused while focused or when reduced motion is requested), with pill indicators.
 */
@Composable
fun TvFeaturedCarousel(
    games: List<GameInfo>,
    onSelect: (GameInfo) -> Unit,
    onPlay: (GameInfo) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) return
    val reduceMotion = LocalReduceMotion.current
    var page by remember(games) { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(games, page, focused, reduceMotion) {
        if (games.size > 1 && !focused && !reduceMotion) {
            delay(TvMotion.CarouselAdvanceMs)
            page = (page + 1) % games.size
        }
    }
    val game = games[page.coerceIn(games.indices)]
    val shape = RoundedCornerShape(OpenNowRadius.xl)
    Box(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(16f / 6f)
            .clip(shape)
            .background(OpenNowPalette.ImagePlaceholder)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .clickable { onSelect(game) },
    ) {
        game.thumbUrl()?.let { url ->
            AsyncImage(
                model = url,
                contentDescription = game.title,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
            )
        }
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.horizontalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.72f),
                            Color.Transparent,
                            Color.Transparent,
                        ),
                    ),
                ),
        )
        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(horizontal = 40.dp, vertical = 32.dp),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
        ) {
            Text(
                text = game.title,
                style = TvTypography.Display,
                color = Color.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val subtitle = game.subtitle()
            if (subtitle.isNotBlank()) {
                Text(
                    text = subtitle,
                    style = TvTypography.Subtitle,
                    color = Color.White.copy(alpha = 0.85f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            TvLongButton(
                text = stringResource(R.string.action_play),
                onClick = { onPlay(game) },
                icon = Icons.Filled.PlayArrow,
            )
        }
        Row(
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(20.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            games.forEachIndexed { index, _ ->
                Box(
                    modifier = Modifier
                        .width(if (index == page) 26.dp else 10.dp)
                        .height(6.dp)
                        .clip(CircleShape)
                        .background(
                            if (index == page) TvPalette.AccentTint
                            else Color.White.copy(alpha = 0.35f),
                        ),
                )
            }
        }
    }
}

/** TV motion timing — slightly slower than phone so focus changes read at distance. */
private object TvMotion {
    const val CarouselAdvanceMs = 6_000L
}
