package com.opencloudgaming.opennow


import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Cast
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import com.opencloudgaming.opennow.ui.adaptive.TWO_PANE_WIDE_PANE_MIN_WIDTH
import com.opencloudgaming.opennow.ui.adaptive.WIDE_CONTENT_MIN_WIDTH
import com.opencloudgaming.opennow.ui.adaptive.windowSizeClassOf
import kotlinx.coroutines.delay
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.roundToInt




// GameDetailsSheet (was OpenNowScreens.kt:5517)
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun GameDetailsSheet(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    fullScreen: Boolean,
    safeAreaPadding: Dp,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    similarGames: List<GameInfo> = emptyList(),
    onSelectGame: (GameInfo) -> Unit = {},
) {
    val gameFocusRequester = remember(game.id) { FocusRequester() }
    val playFocusRequester = remember(game.id) { FocusRequester() }
    LaunchedEffect(game.id, fullScreen) {
        delay(80)
        val initialRequester = if (shouldInitiallyFocusGameDetailsPlay(tvProfile = fullScreen)) {
            playFocusRequester
        } else {
            gameFocusRequester
        }
        runCatching { initialRequester.requestFocus() }
    }
    BackHandler(onBack = onDismiss)
    // Drag-to-dismiss for the phone sheet. Everyone reaches for this gesture on a bottom sheet and
    // previously nothing happened — there was no handle and no drag response at all. Implemented
    // here rather than by switching to ModalBottomSheet so the sheet keeps its lockedFocusGroup and
    // focus requesters, which the controller and TV navigation depend on.
    val density = LocalDensity.current
    var dragOffset by remember(game.id) { mutableFloatStateOf(0f) }
    val dismissThresholdPx = with(density) { SHEET_DISMISS_DRAG_THRESHOLD.toPx() }
    val dragState = rememberDraggableState { delta ->
        dragOffset = (dragOffset + delta).coerceAtLeast(0f)
    }
    Box(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Surface(
            modifier = Modifier
                .then(
                    if (fullScreen) {
                        Modifier.fillMaxSize()
                    } else {
                        Modifier
                            .fillMaxWidth()
                            .fillMaxHeight(0.92f)
                            .offset { IntOffset(0, dragOffset.roundToInt()) }
                    },
                )
                .clickable(onClick = {}),
            shape = if (fullScreen) RoundedCornerShape(0.dp) else RoundedCornerShape(topStart = OpenNowRadius.xl, topEnd = OpenNowRadius.xl),
            color = Panel,
            tonalElevation = 8.dp,
        ) {
            Column(Modifier.fillMaxSize()) {
                if (!fullScreen) {
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .draggable(
                                state = dragState,
                                orientation = Orientation.Vertical,
                                onDragStopped = { velocity ->
                                    if (dragOffset > dismissThresholdPx || velocity > SHEET_DISMISS_FLING_VELOCITY) {
                                        onDismiss()
                                    } else {
                                        dragOffset = 0f
                                    }
                                },
                            )
                            .padding(vertical = OpenNowSpacing.md),
                        contentAlignment = Alignment.Center,
                    ) {
                        Box(
                            Modifier
                                .size(width = 34.dp, height = 4.dp)
                                .clip(CircleShape)
                                .background(TextMuted.copy(alpha = 0.45f)),
                        )
                    }
                }
            BoxWithConstraints(
                Modifier
                    .fillMaxSize()
                    .padding(if (fullScreen) safeAreaPadding else 0.dp),
            ) {
                val aspect = if (maxHeight.value > 0f) maxWidth.value / maxHeight.value else 1f
                val landscapeTvLayout = maxWidth >= WIDE_CONTENT_MIN_WIDTH && aspect >= 1.35f
                val phoneLandscapeLayout = landscapeTvLayout && windowSizeClassOf(maxWidth, maxHeight).isPhone
                if (landscapeTvLayout) {
                    GameDetailsLandscapeContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        connectedTvName = connectedTvName,
                        onPlayOnTv = onPlayOnTv,
                        onDismiss = onDismiss,
                        gameFocusRequester = gameFocusRequester,
                        playFocusRequester = playFocusRequester,
                        shortHeight = maxHeight <= 620.dp,
                        imageActionsOverlay = phoneLandscapeLayout,
                    )
                } else if (!fullScreen && maxWidth >= WIDE_CONTENT_MIN_WIDTH && similarGames.isNotEmpty()) {
                    // M3 adaptive two-pane: details on the left, "More like this" on the right.
                    // 720dp gate keeps the left pane usable — below that the sheet stays single-pane.
                    GameDetailsTabletTwoPane(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        connectedTvName = connectedTvName,
                        onPlayOnTv = onPlayOnTv,
                        onDismiss = onDismiss,
                        gameFocusRequester = gameFocusRequester,
                        playFocusRequester = playFocusRequester,
                        similarGames = similarGames,
                        onSelectGame = onSelectGame,
                        paneWidth = if (maxWidth >= TWO_PANE_WIDE_PANE_MIN_WIDTH) 360.dp else 300.dp,
                    )
                } else {
                    GameDetailsScrollableContent(
                        game = game,
                        favorite = favorite,
                        defaultVariantId = defaultVariantId,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                        onFavorite = onFavorite,
                        connectedTvName = connectedTvName,
                        onPlayOnTv = onPlayOnTv,
                        onDismiss = onDismiss,
                        gameFocusRequester = gameFocusRequester,
                        playFocusRequester = playFocusRequester,
                    )
                }
            }
            }
        }
    }
}

/**
 * M3 adaptive two-pane game details for tablet-sized sheets (>= 600dp wide, portrait): the normal
 * scrollable details on the left and a "More like this" list on the right. Selecting a suggestion
 * swaps the sheet to that game via [onSelectGame].
 */

// GameDetailsTabletTwoPane (was OpenNowScreens.kt:5677)
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsTabletTwoPane(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    gameFocusRequester: FocusRequester,
    playFocusRequester: FocusRequester,
    similarGames: List<GameInfo>,
    onSelectGame: (GameInfo) -> Unit,
    paneWidth: Dp,
) {
    Row(Modifier.fillMaxSize().padding(horizontal = OpenNowSpacing.xl)) {
        Box(Modifier.weight(1f)) {
            GameDetailsScrollableContent(
                game = game,
                favorite = favorite,
                defaultVariantId = defaultVariantId,
                onPlay = onPlay,
                onChooseStore = onChooseStore,
                onFavorite = onFavorite,
                connectedTvName = connectedTvName,
                onPlayOnTv = onPlayOnTv,
                onDismiss = onDismiss,
                gameFocusRequester = gameFocusRequester,
                playFocusRequester = playFocusRequester,
            )
        }
        Box(
            Modifier
                .width(1.dp)
                .fillMaxHeight()
                .padding(vertical = OpenNowSpacing.lg)
                .background(OpenNowPalette.PanelHairline),
        )
        Column(
            Modifier
                .width(paneWidth)
                .fillMaxHeight()
                .padding(start = OpenNowSpacing.xl, top = OpenNowSpacing.lg),
        ) {
            Text(
                text = stringResource(R.string.game_details_more_like_this),
                style = MaterialTheme.typography.titleLarge,
                color = TextPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(OpenNowSpacing.md))
            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(10.dp),
                contentPadding = PaddingValues(end = OpenNowSpacing.md, bottom = 18.dp),
            ) {
                items(similarGames, key = { it.id }) { similar ->
                    GameRecommendationRow(
                        game = similar,
                        onClick = { onSelectGame(similar) },
                    )
                }
            }
        }
    }
}

/** Compact selectable row for the tablet "More like this" pane. */

// GameRecommendationRow (was OpenNowScreens.kt:5749)
@Composable
private fun GameRecommendationRow(
    game: GameInfo,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(OpenNowRadius.md)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(if (focused) OpenNowPalette.Panel else Color.Transparent)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .clickable(onClick = onClick)
            .padding(OpenNowSpacing.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
    ) {
        Box(
            Modifier
                .width(128.dp)
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(OpenNowRadius.sm))
                .background(OpenNowPalette.ImagePlaceholder),
        ) {
            UrlImage(
                url = catalogCardImageUrl(game, tvProfile = false),
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
            )
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                text = game.title,
                style = MaterialTheme.typography.titleMedium,
                color = TextPrimary,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            game.publisherName?.takeIf { it.isNotBlank() }?.let { publisher ->
                Text(
                    text = publisher,
                    style = MaterialTheme.typography.labelMedium,
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** How far the sheet must be dragged down before letting go dismisses it. */

// SHEET_DISMISS_DRAG_THRESHOLD (was OpenNowScreens.kt:5803)
private val SHEET_DISMISS_DRAG_THRESHOLD = 140.dp

/** A fast enough flick dismisses regardless of distance travelled. */

// SHEET_DISMISS_FLING_VELOCITY (was OpenNowScreens.kt:5806)
private const val SHEET_DISMISS_FLING_VELOCITY = 1_200f

// GameDetailsLandscapeContent (was OpenNowScreens.kt:5808)
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsLandscapeContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    gameFocusRequester: FocusRequester,
    playFocusRequester: FocusRequester,
    shortHeight: Boolean,
    imageActionsOverlay: Boolean,
) {
    val description = gameDescriptionForDetails(game)
    val context = LocalContext.current
    val sideScrollState = rememberScrollState()
    val detailsSpacing = if (shortHeight) 8.dp else 10.dp
    var gameFocused by remember(game.id) { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxSize()
            .padding(horizontal = if (shortHeight) 18.dp else 24.dp, vertical = if (shortHeight) 16.dp else 22.dp),
        horizontalArrangement = Arrangement.spacedBy(if (shortHeight) 16.dp else 22.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .weight(0.92f)
                .fillMaxHeight()
                .focusRequester(gameFocusRequester)
                .focusProperties { right = playFocusRequester }
                .onFocusChanged { gameFocused = it.isFocused }
                .border(
                    width = if (gameFocused) 3.dp else 1.dp,
                    color = if (gameFocused) MaterialTheme.colorScheme.primary else Color.White.copy(alpha = 0.12f),
                    shape = RoundedCornerShape(20.dp),
                )
                .clip(RoundedCornerShape(20.dp))
                .clickable {
                    onDismiss()
                    onPlay(game)
                },
        ) {
            UrlImage(gameHeroImageUrl(context, game), Modifier.fillMaxSize())
            GameImageTitleOverlay(
                game = game,
                compact = shortHeight,
                reserveEndSpace = imageActionsOverlay,
                modifier = Modifier.align(Alignment.BottomStart),
            )
            if (imageActionsOverlay) {
                ImageCloseButton(
                    onClick = onDismiss,
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(10.dp),
                )
            }
            if (imageActionsOverlay) {
                Column(
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(14.dp)
                        .width(150.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    connectedTvName?.let { tvName ->
                        OutlinedButton(
                            onClick = {
                                onDismiss()
                                onPlayOnTv(game)
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text("Play on TV", maxLines = 1)
                        }
                    }
                    LongPressPlayButton(
                        onClick = {
                            onDismiss()
                            onPlay(game)
                        },
                        onLongClick = {
                            onDismiss()
                            onChooseStore(game)
                        },
                        modifier = Modifier
                            .fillMaxWidth()
                            .focusRequester(playFocusRequester),
                    )
                }
            }
        }

        Column(
            Modifier
                .weight(1.08f)
                .fillMaxHeight(),
            verticalArrangement = Arrangement.spacedBy(detailsSpacing),
        ) {
            if (imageActionsOverlay) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .verticalScroll(sideScrollState),
                    verticalArrangement = Arrangement.spacedBy(detailsSpacing),
                ) {
                    GameDetailsCompactInfoContent(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        description = description,
                    )
                }
            } else {
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .verticalScroll(sideScrollState),
                    verticalArrangement = Arrangement.spacedBy(detailsSpacing),
                ) {
                    GameDetailsCompactInfoContent(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        description = description,
                    )
                }
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    var dismissFocused by remember { mutableStateOf(false) }
                    val accent = MaterialTheme.colorScheme.primary
                    OutlinedButton(
                        onClick = onDismiss,
                        border = BorderStroke(1.dp, if (dismissFocused) accent else MaterialTheme.colorScheme.outline),
                        modifier = Modifier
                            .weight(1f)
                            .height(48.dp)
                            .onFocusChanged { dismissFocused = it.isFocused }
                    ) {
                        Text(
                            "Dismiss",
                            color = if (dismissFocused) accent else TextPrimary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                    LongPressPlayButton(
                        onClick = {
                            onDismiss()
                            onPlay(game)
                        },
                        onLongClick = {
                            onDismiss()
                            onChooseStore(game)
                        },
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(playFocusRequester),
                    )
                    connectedTvName?.let {
                        OutlinedButton(
                            onClick = {
                                onDismiss()
                                onPlayOnTv(game)
                            },
                            modifier = Modifier.weight(1f).height(48.dp),
                        ) {
                            Text("Play on TV", maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

// GameDetailsCompactInfoContent (was OpenNowScreens.kt:5991)
@Composable
private fun GameDetailsCompactInfoContent(
    game: GameInfo,
    defaultVariantId: String?,
    description: String?,
) {
    OwnershipStatusRow(game = game, compact = true)
    GameGenreChips(game = game, compact = true)
    GameScreenshotGallery(game = game, compact = true)
    GameDescriptionDisclosure(description = description, compact = true)
    CompactDetailRows(game)
    LaunchOptionsList(
        game = game,
        defaultVariantId = defaultVariantId,
        compact = true,
    )
}

// GameDetailsScrollableContent (was OpenNowScreens.kt:6009)
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GameDetailsScrollableContent(
    game: GameInfo,
    favorite: Boolean,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    connectedTvName: String?,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
    gameFocusRequester: FocusRequester,
    playFocusRequester: FocusRequester,
) {
    val context = LocalContext.current
    var gameFocused by remember(game.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(bottom = OpenNowSpacing.lg),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
        ) {
            item {
                Box(
                    Modifier
                        .fillMaxWidth()
                        // Scales with the screen instead of being pinned at 220dp, which was
                        // cramped on a tablet and oversized on a small phone.
                        .aspectRatio(16f / 9f)
                        .padding(horizontal = 10.dp, vertical = 6.dp)
                        .focusRequester(gameFocusRequester)
                        .focusProperties { down = playFocusRequester }
                        .onFocusChanged { gameFocused = it.isFocused }
                        // M3: the hero is borderless at rest; a focus ring appears only for
                        // controller navigation so the resting card stays visually clean.
                        .border(
                            width = if (gameFocused) 3.dp else 0.dp,
                            color = if (gameFocused) MaterialTheme.colorScheme.primary else Color.Transparent,
                            shape = RoundedCornerShape(OpenNowRadius.lg),
                        )
                        .clip(RoundedCornerShape(OpenNowRadius.lg))
                        .clickable {
                            onDismiss()
                            onPlay(game)
                        },
                ) {
                    UrlImage(
                        gameHeroImageUrl(context, game),
                        Modifier.fillMaxSize(),
                    )
                    // Guarantees the title overlay stays legible over bright key art.
                    Box(
                        Modifier
                            .matchParentSize()
                            .background(
                                Brush.verticalGradient(
                                    0.4f to Color.Transparent,
                                    1f to Color.Black.copy(alpha = 0.75f),
                                ),
                            ),
                    )
                    GameImageTitleOverlay(
                        game = game,
                        compact = false,
                        reserveEndSpace = false,
                        modifier = Modifier.align(Alignment.BottomStart),
                    )
                }
            }
            item {
                Column(
                    Modifier.padding(horizontal = OpenNowSpacing.lg),
                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                ) {
                    val description = gameDescriptionForDetails(game)
                    OwnershipStatusRow(game = game, compact = false)
                    GameGenreChips(game = game, compact = false)
                    GameScreenshotGallery(game = game, compact = false)
                    GameDescriptionDisclosure(description = description, compact = false)
                    DetailRows(game)
                    LaunchOptionsList(
                        game = game,
                        defaultVariantId = defaultVariantId,
                        compact = false,
                    )
                }
            }
        }
        Surface(color = Panel.copy(alpha = 0.98f), tonalElevation = 8.dp) {
            // Play is the point of the screen, so it takes the width. Dismiss and the secondary
            // actions become fixed-size icons rather than equal-weight buttons that squeezed Play
            // down to a third of the bar whenever a TV was connected.
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 14.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                var dismissFocused by remember { mutableStateOf(false) }
                val accent = MaterialTheme.colorScheme.primary
                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier
                        .size(48.dp)
                        .onFocusChanged { dismissFocused = it.isFocused },
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_clear),
                        contentDescription = stringResource(R.string.action_dismiss),
                        tint = if (dismissFocused) accent else TextMuted,
                    )
                }
                // favorite/onFavorite were already threaded into this composable but never used —
                // on phones the only way to favourite a game was from the grid.
                FavoriteIconButton(
                    favorite = favorite,
                    onClick = { onFavorite(game.id) },
                    size = 48.dp,
                )
                LongPressPlayButton(
                    onClick = {
                        onDismiss()
                        onPlay(game)
                    },
                    onLongClick = {
                        onDismiss()
                        onChooseStore(game)
                    },
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(playFocusRequester),
                )
                connectedTvName?.let { tvName ->
                    IconButton(
                        onClick = {
                            onDismiss()
                            onPlayOnTv(game)
                        },
                        modifier = Modifier.size(48.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Outlined.Cast,
                            contentDescription = stringResource(R.string.action_play_on_tv, tvName),
                            tint = TextPrimary,
                        )
                    }
                }
            }
        }
    }
}

// LaunchOptionsList (was OpenNowScreens.kt:6163)
@Composable
private fun LaunchOptionsList(
    game: GameInfo,
    defaultVariantId: String?,
    compact: Boolean,
) {
    val variants = launchableGameVariants(game.variants)
    if (variants.size <= 1) return
    Column(verticalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 8.dp)) {
        Text(
            stringResource(R.string.store_selector_launchers),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
        variants.take(if (compact) 3 else variants.size).forEach { variant ->
            val isDefault = variant.id == defaultVariantId
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
                color = if (isDefault) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
                contentColor = TextPrimary,
            ) {
                Row(
                    Modifier.padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 8.dp else 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val details = variantDetailsText(variant)
                        Text(
                            if (isDefault) {
                                listOf(stringResource(R.string.store_selector_default), details).filter { it.isNotBlank() }.joinToString(" - ")
                            } else {
                                details.ifBlank { stringResource(R.string.store_selector_available_launcher) }
                            },
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = if (compact) 1 else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

// LongPressPlayButton (was OpenNowScreens.kt:6212)
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun LongPressPlayButton(
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val controllerFocusEnabled = LocalControllerFocusEnabled.current
    var focused by remember { mutableStateOf(false) }
    val controllerFocused = focused && controllerFocusEnabled
    val shape = RoundedCornerShape(999.dp)
    val accent = MaterialTheme.colorScheme.primary
    val focusScale by animateFloatAsState(
        targetValue = gameDetailsPlayFocusScale(controllerFocused),
        animationSpec = tween(durationMillis = 150, easing = FastOutSlowInEasing),
        label = "game-details-play-focus-scale",
    )
    val containerColor by animateColorAsState(
        targetValue = if (controllerFocused) Color.White else accent,
        animationSpec = tween(durationMillis = 120),
        label = "game-details-play-focus-color",
    )
    Surface(
        modifier = modifier
            .height(48.dp)
            .onFocusChanged { focusState -> focused = focusState.isFocused }
            .graphicsLayer {
                scaleX = focusScale
                scaleY = focusScale
            }
            .onPreviewKeyEvent { event ->
                if (isTvActivateKey(event)) {
                    onClick()
                    true
                } else {
                    false
                }
            }
            .focusable()
            .combinedClickable(
                onClick = onClick,
                onLongClick = onLongClick,
                onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
            )
            .then(
                if (controllerFocused) {
                    Modifier.border(
                        width = gameDetailsPlayFocusBorderWidthDp(controllerFocused).dp,
                        color = accent,
                        shape = shape,
                    )
                } else {
                    Modifier
                },
            ),
        shape = shape,
        color = containerColor,
        tonalElevation = 0.dp,
        shadowElevation = if (controllerFocused) 12.dp else 0.dp,
    ) {
        Row(
            Modifier.fillMaxSize().padding(horizontal = 18.dp),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ZortosPlayMark(
                modifier = Modifier.size(20.dp),
                ringColor = Color.Black,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                stringResource(R.string.action_play),
                color = Color.Black,
                fontWeight = if (controllerFocused) FontWeight.ExtraBold else FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

// gameDetailsPlayFocusScale (was OpenNowScreens.kt:6293)
internal fun gameDetailsPlayFocusScale(focused: Boolean): Float = if (focused) 1.06f else 1f

// gameDetailsPlayFocusBorderWidthDp (was OpenNowScreens.kt:6295)
internal fun gameDetailsPlayFocusBorderWidthDp(focused: Boolean): Float = if (focused) 4f else 0f

// variantDetailsText (was OpenNowScreens.kt:6297)
private fun variantDetailsText(variant: GameVariant): String =
    listOfNotNull(
        variant.libraryStatus?.takeIf { it.isNotBlank() }?.let(::formatGameMetadataLabel),
        variant.supportedControls.takeIf { it.isNotEmpty() }?.joinToString(", ") { formatGameMetadataLabel(it) },
        variant.lastPlayedDate?.takeIf { it.isNotBlank() }?.let { "Last played $it" },
    ).joinToString(" - ")

// ImageCloseButton (was OpenNowScreens.kt:6304)
@Composable
private fun ImageCloseButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    Surface(
        modifier = modifier
            .size(44.dp)
            .onFocusChanged { focused = it.isFocused }
            .border(
                width = 2.dp,
                color = if (focused) accent else Color.Transparent,
                shape = CircleShape
            )
            .clickable(onClick = onClick),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.58f),
        tonalElevation = 3.dp,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Icon(
                painter = painterResource(R.drawable.ic_clear),
                contentDescription = stringResource(R.string.action_cancel),
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

// FavoriteIconButton (was OpenNowScreens.kt:6333)
@Composable
internal fun FavoriteIconButton(favorite: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, size: Dp = 44.dp) {
    val label = stringResource(if (favorite) R.string.action_saved else R.string.action_save)
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    Surface(
        modifier = modifier
            .minimumInteractiveComponentSize()
            .size(size)
            .onFocusChanged { focused = it.isFocused }
            .semantics {
                contentDescription = label
                role = Role.Button
            }
            .clickable(onClick = onClick)
            .focusable()
            .then(
                if (focused) Modifier.border(2.dp, accent, CircleShape) else Modifier
            ),
        shape = CircleShape,
        color = Color.Black.copy(alpha = 0.35f),
        tonalElevation = 0.dp,
        border = BorderStroke(1.dp, if (focused) accent else Color.White.copy(alpha = 0.2f)),
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Icon(
                painter = painterResource(if (favorite) R.drawable.ic_save_filled else R.drawable.ic_save),
                contentDescription = null,
                tint = if (favorite) MaterialTheme.colorScheme.primary else TextPrimary,
                modifier = Modifier.size(size * 0.5f),
            )
        }
    }
}

// gameDescriptionForDetails (was OpenNowScreens.kt:6368)
internal fun gameDescriptionForDetails(game: GameInfo): String? =
    game.description?.takeIf { it.isNotBlank() }
        ?: game.longDescription?.takeIf { it.isNotBlank() }

// gameHeroImageUrl (was OpenNowScreens.kt:6372)
internal fun gameHeroImageUrl(context: Context, game: GameInfo?): String? {
    val url = game?.screenshotUrl?.takeIf { it.isNotBlank() }
        ?: game?.tvBannerUrl?.takeIf { it.isNotBlank() }
        ?: game?.imageUrl?.takeIf { it.isNotBlank() }
        ?: return null
    return optimizedNvidiaImageUrl(url, wideImageRequestWidth(context))
}

// gameTvBannerImageUrl (was OpenNowScreens.kt:6380)
internal fun gameTvBannerImageUrl(context: Context, game: GameInfo?): String? {
    val url = game?.tvBannerUrl?.takeIf { it.isNotBlank() }
        ?: game?.screenshotUrl?.takeIf { it.isNotBlank() }
        ?: game?.imageUrl?.takeIf { it.isNotBlank() }
        ?: return null
    return optimizedNvidiaImageUrl(url, wideImageRequestWidth(context))
}

// optimizedNvidiaImageUrl (was OpenNowScreens.kt:6388)
internal fun optimizedNvidiaImageUrl(url: String, width: Int): String {
    if (!url.contains("img.nvidiagrid.net")) return url
    val base = url
        .substringBefore(";f=")
        .substringBefore(";w=")
        .substringBefore(";h=")
        .substringBefore(";dpr=")
    return "$base;f=webp;w=$width"
}

// wideImageRequestWidth (was OpenNowScreens.kt:6398)
private fun wideImageRequestWidth(context: Context): Int {
    val connectivity = context.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
    val capabilities = connectivity?.getNetworkCapabilities(connectivity.activeNetwork)
    val downstreamKbps = capabilities?.linkDownstreamBandwidthKbps ?: 0
    return when {
        downstreamKbps >= 25_000 -> 1920
        downstreamKbps in 10_000 until 25_000 -> 1600
        downstreamKbps in 3_000 until 10_000 -> 1280
        downstreamKbps in 1 until 3_000 -> 960
        capabilities?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true -> 1600
        capabilities?.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) == true -> 960
        else -> 1280
    }
}

// GameImageTitleOverlay (was OpenNowScreens.kt:6413)
@Composable
private fun GameImageTitleOverlay(
    game: GameInfo,
    compact: Boolean,
    reserveEndSpace: Boolean,
    modifier: Modifier = Modifier,
) {
    val textShadow = Shadow(
        color = Color.Black,
        offset = Offset(0f, 3f),
        blurRadius = 14f,
    )
    Column(
        modifier
            .fillMaxWidth()
            .padding(
                start = if (compact) 12.dp else 16.dp,
                top = if (compact) 9.dp else 12.dp,
                end = if (reserveEndSpace) 154.dp else if (compact) 12.dp else 16.dp,
                bottom = if (compact) 10.dp else 14.dp,
            ),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Text(
            game.title,
            color = TextPrimary,
            style = (if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall).copy(
                shadow = textShadow,
            ),
            fontWeight = FontWeight.Bold,
            maxLines = if (compact) 2 else 2,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            game.publisherName?.takeIf { it.isNotBlank() } ?: "Unknown publisher",
            color = TextPrimary.copy(alpha = 0.88f),
            style = MaterialTheme.typography.bodyMedium.copy(shadow = textShadow),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// OwnershipStatusRow (was OpenNowScreens.kt:6457)
@Composable
private fun OwnershipStatusRow(game: GameInfo, compact: Boolean) {
    val ownedStores = ownedStoreLabels(game)
    val shape = RoundedCornerShape(if (compact) 12.dp else 14.dp)
    if (ownedStores.isEmpty()) {
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = shape,
            color = Color(0xff4a1216),
            tonalElevation = 0.dp,
        ) {
            Text(
                "Not owned",
                color = OpenNowPalette.OnErrorContainer,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = if (compact) 8.dp else 10.dp),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        return
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        ownedStores.forEach { store ->
            val badge = launcherBadgeForStoreKey(normalizeGameStore(store))
            Surface(
                shape = shape,
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.16f),
                tonalElevation = 0.dp,
            ) {
                Row(
                    Modifier.padding(horizontal = 10.dp, vertical = if (compact) 6.dp else 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    ConnectorStoreIcon(badge)
                    Text(
                        "Owned on $store",
                        color = TextPrimary,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

// ownedStoreLabels (was OpenNowScreens.kt:6508)
private fun ownedStoreLabels(game: GameInfo): List<String> =
    libraryStoreDisplayNames(game).ifEmpty {
        if (isGameInLibrary(game)) listOf("GeForce NOW") else emptyList()
    }

// GameGenreChips (was OpenNowScreens.kt:6513)
@Composable
private fun GameGenreChips(game: GameInfo, compact: Boolean) {
    val genres = game.genres
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .map(::formatGameMetadataLabel)
        .filterNot(::isNoisyGameTag)
        .distinctBy { it.lowercase(Locale.US) }
        .take(if (compact) 12 else 20)
    if (genres.isEmpty()) return
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 7.dp),
        contentPadding = PaddingValues(end = if (compact) 6.dp else 8.dp),
    ) {
        items(genres, key = { it }) { label ->
            AssistChip(onClick = {}, label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
        }
    }
}

// GameScreenshotGallery (was OpenNowScreens.kt:6534)
@Composable
private fun GameScreenshotGallery(game: GameInfo, compact: Boolean) {
    val screenshots = game.screenshotUrls
        .map(String::trim)
        .filter(String::isNotBlank)
        .distinct()
    if (screenshots.isEmpty()) return
    val context = LocalContext.current
    val requestWidth = remember(context) { wideImageRequestWidth(context).coerceAtLeast(960) }
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(if (compact) 7.dp else 9.dp),
    ) {
        Text(
            "Screenshots",
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.labelLarge else MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
        )
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
            contentPadding = PaddingValues(end = 8.dp),
        ) {
            items(screenshots, key = { it }) { screenshot ->
                Surface(
                    modifier = Modifier
                        .width(if (compact) 224.dp else 288.dp)
                        .aspectRatio(16f / 9f),
                    shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
                    color = Color.Black,
                    border = BorderStroke(1.dp, Color.White.copy(alpha = 0.1f)),
                ) {
                    UrlImage(
                        url = optimizedNvidiaImageUrl(screenshot, requestWidth),
                        modifier = Modifier.fillMaxSize(),
                        contentScale = ContentScale.Fit,
                    )
                }
            }
        }
    }
}

// GameDescriptionDisclosure (was OpenNowScreens.kt:6578)
@Composable
private fun GameDescriptionDisclosure(description: String?, compact: Boolean) {
    var expanded by remember(description) { mutableStateOf(true) }
    val text = description?.takeIf { it.isNotBlank() } ?: "No description is available for this game yet."
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(if (compact) 12.dp else 14.dp))
            .onFocusChanged { focused = it.isFocused }
            .border(
                width = 1.dp,
                color = if (focused) accent else Color.Transparent,
                shape = RoundedCornerShape(if (compact) 12.dp else 14.dp)
            )
            .clickable { expanded = !expanded },
        shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
        color = if (focused) PanelAlt.copy(alpha = 0.85f) else PanelAlt,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = if (compact) 8.dp else 10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    "Description",
                    color = TextPrimary,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { expanded = !expanded }, modifier = Modifier.size(36.dp)) {
                    Icon(
                        painter = painterResource(R.drawable.ic_chevron_right),
                        contentDescription = if (expanded) "Hide description" else "Show description",
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .size(20.dp)
                            .graphicsLayer(rotationZ = if (expanded) 90f else 0f),
                    )
                }
            }
            if (expanded) {
                Text(
                    text,
                    color = if (description == null) TextMuted else TextPrimary,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = if (compact) 8 else Int.MAX_VALUE,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

// formatGameMetadataLabel (was OpenNowScreens.kt:6632)
private fun formatGameMetadataLabel(raw: String): String {
    val compact = raw.trim()
        .removePrefix("GFN_")
        .removePrefix("GAME_")
        .replace(Regex("[_-]+"), " ")
        .replace(Regex("\\s+"), " ")
        .trim()
    if (compact.isBlank()) return ""
    val lower = compact.lowercase(Locale.US)
    return when (lower) {
        "full game" -> "Full game"
        "single player" -> "Single-player"
        "multi player", "multiplayer" -> "Multiplayer"
        "controller", "gamepad" -> "Controller"
        "keyboard mouse", "mouse keyboard" -> "Mouse and keyboard"
        else -> compact.split(" ").joinToString(" ") { word ->
            if (word.length <= 3 && word.all { it.isUpperCase() || it.isDigit() }) {
                word
            } else {
                word.lowercase(Locale.US).replaceFirstChar { char -> char.titlecase(Locale.US) }
            }
        }
    }
}

// isNoisyGameTag (was OpenNowScreens.kt:6657)
private fun isNoisyGameTag(label: String): Boolean {
    val normalized = label.trim().lowercase(Locale.US)
    return normalized.isBlank() ||
        normalized == "unknown" ||
        normalized == "gfn" ||
        normalized == "nvidia" ||
        normalized.contains("sku based tag") ||
        normalized.contains("catalog")
}

// CompactDetailRows (was OpenNowScreens.kt:6667)
@Composable
private fun CompactDetailRows(game: GameInfo) {
    val rows = gameDetailRows(game).take(4)
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        rows.forEach { row ->
            DetailRow(row = row, compact = true)
        }
    }
}

// DetailRows (was OpenNowScreens.kt:6678)
@Composable
private fun DetailRows(game: GameInfo) {
    val rows = gameDetailRows(game)
    if (rows.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        rows.forEach { row ->
            DetailRow(row = row, compact = false)
        }
    }
}

// GameDetailRow (was OpenNowScreens.kt:6689)
private data class GameDetailRow(
    val label: String,
    val value: String,
    val copyValue: String? = null,
)

// gameDetailRows (was OpenNowScreens.kt:6695)
private fun gameDetailRows(game: GameInfo): List<GameDetailRow> =
    listOfNotNull(
        game.playabilityState?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Status", formatGameMetadataLabel(it)) },
        gameAppIdForDetails(game)?.let { GameDetailRow("App ID", it, copyValue = it) },
        game.contentRatings.takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { GameDetailRow("Rating", it) },
        game.lastPlayed?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Last played", it) },
        game.availableStores.takeIf { it.isNotEmpty() }?.map(::gameStoreDisplayName)?.distinct()?.joinToString(", ")?.let { GameDetailRow("Stores", it) },
    )

// gameAppIdForDetails (was OpenNowScreens.kt:6704)
private fun gameAppIdForDetails(game: GameInfo): String? =
    game.launchAppId?.takeIf { it.isNotBlank() }
        ?: game.variants.firstNotNullOfOrNull { variant -> variant.id.takeIf { it.isNotBlank() && it.all(Char::isDigit) } }
        ?: game.uuid?.takeIf { it.isNotBlank() }
        ?: game.id.takeIf { it.isNotBlank() }

// DetailRow (was OpenNowScreens.kt:6710)
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DetailRow(row: GameDetailRow, compact: Boolean) {
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    val shape = RoundedCornerShape(if (compact) 10.dp else 12.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(PanelAlt)
            .combinedClickable(
                onClick = {},
                onLongClick = row.copyValue?.let { value ->
                    {
                        clipboard.setText(AnnotatedString(value))
                        Toast.makeText(context, "App ID copied", Toast.LENGTH_SHORT).show()
                    }
                },
            )
            .padding(horizontal = if (compact) 10.dp else 12.dp, vertical = if (compact) 7.dp else 10.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
    ) {
        Text(
            row.label,
            color = TextMuted,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.width(if (compact) 82.dp else 92.dp),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            if (row.copyValue != null) "${row.value}" else row.value,
            color = TextPrimary,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.weight(1f),
            maxLines = if (compact) 1 else 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// StoreLaunchSelector (was OpenNowScreens.kt:6773)
@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun StoreLaunchSelector(
    game: GameInfo,
    defaultVariantId: String?,
    onLaunch: (GameInfo, GameVariant) -> Unit,
    onSetDefaultStore: (String, String?) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val variants = remember(game) { launchableGameVariants(game.variants) }
    val context = LocalContext.current
    val initialVariantId = remember(game.id, defaultVariantId, variants) {
        defaultVariantId?.takeIf { savedId -> variants.any { it.id == savedId } }
            ?: variants.firstOrNull()?.id
    }
    var selectedVariantId by remember(game.id, initialVariantId) { mutableStateOf(initialVariantId) }
    var rememberDefaultStore by remember(game.id, defaultVariantId) { mutableStateOf(defaultVariantId != null) }
    val selectedVariant = variants.firstOrNull { it.id == selectedVariantId }
    val continueFocusRequester = remember(game.id) { FocusRequester() }
    BackHandler(onBack = onDismiss)
    LaunchedEffect(game.id, variants.size) {
        if (variants.isNotEmpty()) {
            runCatching { continueFocusRequester.requestFocus() }
        }
    }
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
        val landscape = maxWidth > maxHeight
        Box(
            Modifier.fillMaxSize(),
            contentAlignment = if (phoneLandscape) Alignment.CenterEnd else Alignment.Center,
        ) {
            Card(
                modifier = modifier
                    .then(
                        if (phoneLandscape) {
                            Modifier
                                .padding(end = 12.dp)
                                .fillMaxWidth(0.9f)
                                .fillMaxHeight(0.9f)
                        } else {
                            Modifier
                                .fillMaxWidth(if (landscape) 0.78f else 0.92f)
                                .fillMaxHeight(if (landscape) 0.86f else 0.64f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel, contentColor = TextPrimary),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        LaunchGameSummary(
                            game = game,
                            subtitle = stringResource(R.string.store_selector_choose_launcher),
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
                    Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            UrlImage(
                                game.imageUrl,
                                Modifier
                                    .width(58.dp)
                                    .height(76.dp)
                                    .clip(RoundedCornerShape(12.dp)),
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(stringResource(R.string.store_selector_choose_launcher), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        StoreLaunchOptionsColumn(
                            variants = variants,
                            selectedVariantId = selectedVariantId,
                            defaultVariantId = defaultVariantId,
                            rememberDefaultStore = rememberDefaultStore,
                            selectedVariant = selectedVariant,
                            continueFocusRequester = continueFocusRequester,
                            onSelectVariant = { selectedVariantId = it },
                            onRememberDefaultStoreChange = { rememberDefaultStore = it },
                            onDismiss = onDismiss,
                            onContinue = { variant ->
                                if (rememberDefaultStore || defaultVariantId != null) {
                                    onSetDefaultStore(game.id, if (rememberDefaultStore) variant.id else null)
                                }
                                if (rememberDefaultStore) {
                                    Toast.makeText(context, context.getString(R.string.store_selector_long_press_tip), Toast.LENGTH_LONG).show()
                                }
                                onLaunch(game, variant)
                            },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

// LaunchGameSummary (was OpenNowScreens.kt:6910)
@Composable
private fun LaunchGameSummary(game: GameInfo, subtitle: String, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            game.imageUrl,
            Modifier
                .fillMaxWidth()
                .weight(1f)
                .clip(RoundedCornerShape(16.dp)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

// StoreLaunchOptionsColumn (was OpenNowScreens.kt:6927)
@Composable
private fun StoreLaunchOptionsColumn(
    variants: List<GameVariant>,
    selectedVariantId: String?,
    defaultVariantId: String?,
    rememberDefaultStore: Boolean,
    selectedVariant: GameVariant?,
    continueFocusRequester: FocusRequester,
    onSelectVariant: (String) -> Unit,
    onRememberDefaultStoreChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
    onContinue: (GameVariant) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(variants, key = { it.id }) { variant ->
                StoreLaunchVariantRow(
                    variant = variant,
                    selected = variant.id == selectedVariantId,
                    savedDefault = variant.id == defaultVariantId,
                    onClick = { onSelectVariant(variant.id) },
                )
            }
        }
        var checkFocused by remember { mutableStateOf(false) }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .onFocusChanged { checkFocused = it.isFocused }
                .background(if (checkFocused) Color.White.copy(alpha = 0.08f) else Color.Transparent)
                .border(
                    width = 1.dp,
                    color = if (checkFocused) MaterialTheme.colorScheme.primary else Color.Transparent,
                    shape = RoundedCornerShape(14.dp)
                )
                .clickable { onRememberDefaultStoreChange(!rememberDefaultStore) }
                .padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Checkbox(
                checked = rememberDefaultStore,
                onCheckedChange = onRememberDefaultStoreChange,
            )
            Text(
                stringResource(R.string.store_selector_default_checkbox),
                color = TextPrimary,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.weight(1f),
            )
        }
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedButton(onClick = onDismiss, modifier = Modifier.weight(1f)) {
                Text(stringResource(R.string.action_cancel))
            }
            Button(
                onClick = {
                    val variant = selectedVariant ?: return@Button
                    onContinue(variant)
                },
                enabled = selectedVariant != null,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(continueFocusRequester),
            ) {
                Text(stringResource(R.string.action_continue), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

// StoreLaunchVariantRow (was OpenNowScreens.kt:7007)
@Composable
private fun StoreLaunchVariantRow(
    variant: GameVariant,
    selected: Boolean,
    savedDefault: Boolean,
    onClick: () -> Unit,
) {
    val badge = launcherBadgeForStoreKey(splitGameStoreKeys(variant.store).firstOrNull())
    var focused by remember { mutableStateOf(false) }
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused }
            .border(
                width = 2.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = RoundedCornerShape(14.dp)
            )
            .clickable { onClick() },
        shape = RoundedCornerShape(14.dp),
        color = if (focused) MaterialTheme.colorScheme.primary.copy(alpha = 0.28f) else if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.18f) else PanelAlt,
        contentColor = TextPrimary,
    ) {
        Row(
            Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ConnectorStoreIcon(badge)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(gameStoreDisplayName(variant.store), fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val details = listOf(
                    if (savedDefault) stringResource(R.string.store_selector_default) else "",
                    variantDetailsText(variant),
                ).filter { it.isNotBlank() }.joinToString(" - ")
                if (details.isNotBlank()) {
                    Text(details, color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (selected) {
                Text(
                    stringResource(R.string.store_selector_selected),
                    color = MaterialTheme.colorScheme.primary,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                )
            }
        }
    }
}
