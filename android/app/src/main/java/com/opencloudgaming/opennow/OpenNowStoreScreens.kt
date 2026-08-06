package com.opencloudgaming.opennow


import android.content.res.Configuration
import androidx.annotation.StringRes
import android.view.KeyEvent
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.MutableIntState
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.screens.tv.TvFeaturedCarousel
import com.opencloudgaming.opennow.screens.tv.TvWideCard
import com.opencloudgaming.opennow.ui.adaptive.isAtLeastMedium
import com.opencloudgaming.opennow.ui.adaptive.windowWidthSizeClassOf
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.tint




// catalogWallpaperSelection (was OpenNowScreens.kt:1980)
internal fun catalogWallpaperSelection(
    preset: CatalogBackgroundPreset,
    customSource: String?,
): CatalogWallpaperSelection =
    customSource
        ?.trim()
        ?.takeIf { it.isNotBlank() }
        ?.let(CatalogWallpaperSelection::Custom)
        ?: CatalogWallpaperSelection.BuiltIn(preset)

// shouldShowCatalogWallpaper (was OpenNowScreens.kt:1990)
internal fun shouldShowCatalogWallpaper(settings: AppSettings): Boolean =
    settings.nerdCatalogBackground

// CatalogWallpaperBackdrop (was OpenNowScreens.kt:1993)
@Composable
internal fun CatalogWallpaperBackdrop(
    settings: AppSettings,
    tvProfile: Boolean,
    width: Dp,
    height: Dp,
) {
    val showBackdrop = shouldShowCatalogWallpaper(settings)
    if (!showBackdrop) {
        return
    }
    val wallpaper = catalogWallpaperSelection(
        preset = settings.catalogBackgroundPreset,
        customSource = settings.nerdCatalogBackgroundUri,
    )
    val scrimAlpha = when {
        tvProfile -> 0.48f
        width > height -> 0.28f
        else -> 0.36f
    }
    Box(Modifier.fillMaxSize().clipToBounds()) {
        when (wallpaper) {
            is CatalogWallpaperSelection.BuiltIn -> {
                CatalogBuiltInWallpaperBackdrop(wallpaper.preset, Modifier.matchParentSize())
            }
            is CatalogWallpaperSelection.Custom -> {
                val fallbackPainter = painterResource(settings.catalogBackgroundPreset.drawableRes)
                AsyncImage(
                    model = imageDataForSource(wallpaper.source),
                    contentDescription = null,
                    modifier = Modifier.matchParentSize(),
                    contentScale = ContentScale.Crop,
                    placeholder = fallbackPainter,
                    error = fallbackPainter,
                    fallback = fallbackPainter,
                )
            }
        }
        Box(
            Modifier
                .matchParentSize()
                .background(Color.Black.copy(alpha = scrimAlpha)),
        )
    }
}

// drawableRes (was OpenNowScreens.kt:2039)
internal val CatalogBackgroundPreset.drawableRes: Int
    get() = when (this) {
        CatalogBackgroundPreset.ColorfulAbstract -> R.drawable.catalog_colorful_abstract_background
        CatalogBackgroundPreset.Original -> R.drawable.catalog_default_background
    }

// CatalogBuiltInWallpaperBackdrop (was OpenNowScreens.kt:2045)
@Composable
internal fun CatalogBuiltInWallpaperBackdrop(
    preset: CatalogBackgroundPreset,
    modifier: Modifier = Modifier,
) {
    Image(
        painter = painterResource(preset.drawableRes),
        contentDescription = null,
        modifier = modifier.background(OpenNowPalette.WallpaperBackdrop),
        contentScale = ContentScale.Crop,
    )
}

// HomeScreen (was OpenNowScreens.kt:3031)
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun HomeScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    controlsInTopBar: Boolean,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val visibleGames = state.games.ifEmpty { state.catalogResult.games }
    val searchingCatalog = state.loadingGames && state.catalogSearch.isNotBlank()
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.catalogSearch.isNotBlank()
    val showScrollActions = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 80
    val scrolledAwayFromTop = gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0
    val hideScrollChrome = hideChromeWhenScrolled && scrolledAwayFromTop
    LaunchedEffect(hideScrollChrome) {
        onScrollChromeHiddenChange(hideScrollChrome)
    }
    DisposableEffect(Unit) {
        onDispose { onScrollChromeHiddenChange(false) }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    SwipeToRefreshContainer(
        refreshing = state.loadingGames,
        enabled = !tvProfile,
        showRefreshIndicator = !searchingCatalog,
        onRefresh = viewModel::refreshGames,
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(
                        // M3 large screens step the content gutter up from 12dp to 24dp. Handheld
                        // only — TV keeps its own gutters from the TV design system.
                        start = if (!tvProfile && windowWidthSizeClassOf(maxWidth).isAtLeastMedium) OpenNowSpacing.xl else 12.dp,
                        top = if (controlsInTopBar) 4.dp else 12.dp,
                        end = if (!tvProfile && windowWidthSizeClassOf(maxWidth).isAtLeastMedium) OpenNowSpacing.xl else 12.dp,
                        bottom = 12.dp,
                    ),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        modifier = Modifier.fillMaxWidth(),
                        query = state.catalogSearch,
                        onQueryChange = { next ->
                            viewModel.setCatalogSearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = stringResource(R.string.search_games),
                        searching = searchingCatalog,
                        focusRequester = searchFocusRequester,
                        onOpen = {
                            if (gridState.firstVisibleItemIndex > 0 || gridState.firstVisibleItemScrollOffset > 0) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        },
                    )
                }
                Box(
                    Modifier
                        .weight(1f)
                        .pointerInput(Unit) {
                            awaitEachGesture {
                                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                                focusManager.clearFocus()
                                keyboardController?.hide()
                            }
                        },
                ) {
                    if (state.loadingGames && visibleGames.isEmpty()) {
                        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            StoreScrollableControls(
                                state = state,
                                onSortChange = viewModel::setCatalogSort,
                                onFilterToggle = viewModel::toggleCatalogFilter,
                                showToolbar = !controlsInTopBar,
                            )
                            RefreshingGamesPlaceholder(
                                settings = state.settings,
                                tvProfile = tvProfile,
                                storeLayout = true,
                                modifier = Modifier.weight(1f),
                            )
                        }
                    } else {
                        StoreGameGrid(
                            games = visibleGames,
                            favoriteIds = state.settings.favoriteGameIds,
                            settings = state.settings,
                            tvProfile = tvProfile,
                            state = state,
                            onSelect = viewModel::selectGame,
                            onFavorite = viewModel::updateFavorites,
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
                            onSortChange = viewModel::setCatalogSort,
                            onFilterToggle = viewModel::toggleCatalogFilter,
                            onClearSearch = {
                                viewModel.setCatalogSearch("")
                                onSearchDismissed()
                            },
                            onClearFilters = viewModel::clearCatalogFilters,
                            gridState = gridState,
                            showToolbar = !controlsInTopBar,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    if (showScrollActions) {
                        Box(Modifier.align(Alignment.BottomEnd).padding(2.dp)) {
                            StoreScrollActionButton(
                                iconRes = R.drawable.ic_arrow_up,
                                contentDescription = stringResource(R.string.action_scroll_top),
                            ) {
                                scope.launch { gridState.animateScrollToItem(0) }
                            }
                        }
                    }
                }
            }
        }
    }
}

// StoreScrollableControls (was OpenNowScreens.kt:3171)
@Composable
internal fun StoreScrollableControls(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    showToolbar: Boolean = true,
) {
    val filterGroups = catalogVisibleFilterGroups(state.catalogResult.filterGroups)
    val filterOptions = catalogFilterOptions(filterGroups)
    val hasSelectedFilters = state.catalogFilterIds.isNotEmpty()
    val hasError = !state.error.isNullOrBlank()
    if (!showToolbar && !hasSelectedFilters && !hasError) return
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (showToolbar) {
            StoreCatalogToolbar(
                state = state,
                onSortChange = onSortChange,
                onFilterToggle = onFilterToggle,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        SelectedFilterChips(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle)
        InlineErrorNotice(error = state.error)
    }
}

// StoreCatalogToolbar (was OpenNowScreens.kt:3197)
@Composable
internal fun StoreCatalogToolbar(
    state: OpenNowUiState,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val filterGroups = catalogVisibleFilterGroups(state.catalogResult.filterGroups)
    val filterOptions = catalogFilterOptions(filterGroups)
    Row(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SortPicker(
            options = state.catalogResult.sortOptions,
            selected = state.catalogSortId,
            onSelect = onSortChange,
            modifier = Modifier.width(if (compact) 118.dp else 172.dp),
            compact = compact,
        )
        if (filterOptions.isNotEmpty()) {
            FilterMenu(options = filterOptions, selectedIds = state.catalogFilterIds, onToggle = onFilterToggle, compact = compact)
        }
    }
}

// InlineErrorNotice (was OpenNowScreens.kt:3225)
@Composable
internal fun InlineErrorNotice(error: String?) {
    if (error.isNullOrBlank()) return
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
        color = OpenNowPalette.ErrorContainer,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(
                compactErrorTitle(error),
                color = OpenNowPalette.OnErrorContainer,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                compactErrorBody(error),
                color = OpenNowPalette.OnErrorContainer,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

// compactErrorTitle (was OpenNowScreens.kt:3254)
internal fun compactErrorTitle(error: String): String =
    when {
        error.contains("DNS lookup failed", ignoreCase = true) -> "Network lookup failed"
        error.contains("Unable to resolve host", ignoreCase = true) -> "Network lookup failed"
        else -> "Something went wrong"
    }

// compactErrorBody (was OpenNowScreens.kt:3261)
internal fun compactErrorBody(error: String): String =
    error
        .replace('\n', ' ')
        .replace(Regex("\\s+"), " ")
        .let { if (it.length > 180) "${it.take(177)}..." else it }

// StoreScrollActionButton (was OpenNowScreens.kt:3267)
@Composable
internal fun StoreScrollActionButton(iconRes: Int, contentDescription: String, onClick: () -> Unit) {
    Surface(
        shape = CircleShape,
        color = PanelAlt.copy(alpha = 0.96f),
        tonalElevation = 4.dp,
        shadowElevation = 4.dp,
    ) {
        IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = contentDescription,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

// StoreGameGrid (was OpenNowScreens.kt:3998)
@Composable
internal fun StoreGameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    state: OpenNowUiState,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onSortChange: (String) -> Unit,
    onFilterToggle: (String) -> Unit,
    onClearSearch: () -> Unit,
    onClearFilters: () -> Unit,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState,
    showToolbar: Boolean = true,
    modifier: Modifier = Modifier,
) {
    if (games.isEmpty()) {
        Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            StoreScrollableControls(state, onSortChange, onFilterToggle, showToolbar = showToolbar)
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                val hasSearch = state.catalogSearch.isNotBlank()
                val hasFilters = state.catalogFilterIds.isNotEmpty()
                if (hasSearch || hasFilters) {
                    SearchEmptyState(
                        title = stringResource(R.string.store_empty_search_title),
                        message = when {
                            hasSearch && hasFilters -> stringResource(R.string.store_empty_search_filters_body)
                            hasSearch -> stringResource(R.string.store_empty_search_body)
                            else -> stringResource(R.string.store_empty_filters_body)
                        },
                        onClearSearch = if (hasSearch) onClearSearch else null,
                        onClearFilters = if (hasFilters) onClearFilters else null,
                    )
                } else {
                    Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                }
            }
        }
        return
    }
    val scale = settings.posterSizeScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE)
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = landscapeLayout && !tvProfile)
    val controllerActionMode = landscapeLayout && !tvProfile && physicalControllerConnected
    val artworkOnly = shouldUseArtworkOnlyCatalogCards(tvProfile, controllerActionMode)
    val showControlsHeader = showToolbar || state.catalogFilterIds.isNotEmpty() || !state.error.isNullOrBlank()
    BoxWithConstraints(modifier.fillMaxSize()) {
        val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout, settings, handheldLayout = !tvProfile)
        CatalogFocusScope {
            LazyVerticalGrid(
                modifier = Modifier.fillMaxSize(),
                state = gridState,
                columns = gridSpec.cells,
                contentPadding = gridSpec.contentPadding,
                horizontalArrangement = Arrangement.spacedBy(gridSpec.horizontalSpacing),
                verticalArrangement = Arrangement.spacedBy(gridSpec.verticalSpacing),
            ) {
                if (showControlsHeader) {
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        StoreScrollableControls(state, onSortChange, onFilterToggle, showToolbar = showToolbar)
                    }
                }
                item(span = { GridItemSpan(maxLineSpan) }) {
                    StoreStartRails(
                        games = games,
                        libraryGames = state.libraryGames,
                        favoriteIds = favoriteIds,
                        queuedGameKeys = state.queuedGameKeys,
                        settings = settings,
                        tvProfile = tvProfile,
                        controllerActionMode = controllerActionMode,
                        onSelect = onSelect,
                        onFavorite = onFavorite,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                    )
                }
                if (games.isNotEmpty()) {
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        SectionHeader(
                            title = stringResource(R.string.store_recommendations),
                            modifier = Modifier.padding(top = OpenNowSpacing.lg, bottom = OpenNowSpacing.sm),
                        )
                    }
                }
                gridItems(games, key = { it.id }) { game ->
                    GameCard(
                        game = game,
                        favorite = game.id in favoriteIds,
                        tvProfile = tvProfile,
                        expressiveUi = settings.expressiveUi,
                        controllerBackgroundAnimations = settings.controllerBackgroundAnimations,
                        showGameStoreLabels = !artworkOnly && shouldShowGameStoreLabels(
                            tvProfile = tvProfile,
                            enabled = settings.showGameStoreLabels,
                        ),
                        showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                            tvProfile = tvProfile,
                            enabled = settings.showCardTitles,
                        ),
                        squareCard = gridSpec.squareCards,
                        thumbnailFavoriteOverlay = !tvProfile,
                        controllerActionMode = controllerActionMode,
                        mediaCard = !tvProfile && !gridSpec.squareCards && windowWidthSizeClassOf(maxWidth).isAtLeastMedium,
                        onSelect = onSelect,
                        onFavorite = onFavorite,
                        onPlay = onPlay,
                        onChooseStore = onChooseStore,
                    )
                }
            }
        }
    }
}

// StoreStartRails (was OpenNowScreens.kt:4117)
@Composable
internal fun StoreStartRails(
    games: List<GameInfo>,
    libraryGames: List<GameInfo>,
    favoriteIds: List<String>,
    queuedGameKeys: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    val startRails = remember(games, libraryGames, favoriteIds, queuedGameKeys) {
        storeStartRailGroups(games, libraryGames, favoriteIds, queuedGameKeys)
    }
    val featured = remember(games, startRails) {
        comingNextStoreGames(games = games, excludedGames = startRails.allGames)
            .take(HERO_CAROUSEL_PAGE_LIMIT)
    }
    if (startRails.isEmpty && featured.isEmpty()) return
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 2.dp, bottom = 6.dp),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
    ) {
        // The hero leads, then the rails — the catalog opens on one thing worth looking at rather
        // than on three equally-weighted horizontal strips.
        if (featured.isNotEmpty()) {
            if (tvProfile) {
                // TV Design Kit featured carousel — hero scale typography for the TV experience.
                TvFeaturedCarousel(
                    games = featured,
                    onSelect = onSelect,
                    onPlay = onPlay,
                )
            } else {
                StoreComingNextCarousel(
                    title = stringResource(R.string.store_coming_next),
                    games = featured,
                    favoriteIds = favoriteIds,
                    settings = settings,
                    tvProfile = tvProfile,
                    controllerActionMode = controllerActionMode,
                    onSelect = onSelect,
                    onFavorite = onFavorite,
                    onPlay = onPlay,
                    onChooseStore = onChooseStore,
                )
            }
        }
        StoreStartRail(R.string.store_continue_playing, startRails.continuePlaying, favoriteIds, settings, tvProfile, controllerActionMode, onSelect, onFavorite, onPlay, onChooseStore)
        StoreStartRail(R.string.store_in_queue, startRails.inQueue, favoriteIds, settings, tvProfile, controllerActionMode, onSelect, onFavorite, onPlay, onChooseStore)
        StoreStartRail(R.string.store_favorites, startRails.favorites, favoriteIds, settings, tvProfile, controllerActionMode, onSelect, onFavorite, onPlay, onChooseStore)
    }
}

/** Small wrapper so the three start rails don't repeat an eleven-argument call three times. */

// StoreStartRail (was OpenNowScreens.kt:4177)
@Composable
internal fun StoreStartRail(
    @StringRes titleRes: Int,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    if (games.isEmpty()) return
    StoreRailSection(
        title = stringResource(titleRes),
        games = games,
        favoriteIds = favoriteIds,
        settings = settings,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
        onSelect = onSelect,
        onFavorite = onFavorite,
        onPlay = onPlay,
        onChooseStore = onChooseStore,
    )
}

/**
 * The one heading treatment used by every catalog section — rails, the hero, and the
 * recommendations grid — so a section title looks the same wherever it appears. Previously each
 * of those sites styled its own `Text` and they had drifted apart.
 */

// SectionHeader (was OpenNowScreens.kt:4210)
@Composable
internal fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                title,
                color = TextPrimary,
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!subtitle.isNullOrBlank()) {
                Text(
                    subtitle,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        trailing?.invoke()
    }
}

// StoreComingNextCarousel (was OpenNowScreens.kt:4243)
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun StoreComingNextCarousel(
    title: String,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    if (games.isEmpty()) return
    val context = LocalContext.current
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    var page by remember(games) { mutableIntStateOf(0) }
    var focused by remember { mutableStateOf(false) }
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    val reduceMotion = LocalReduceMotion.current
    LaunchedEffect(games, page, focused, reduceMotion) {
        // Never auto-advance under the reader's hands: not while focused, and not at all when the
        // user has asked for reduced motion.
        if (games.size > 1 && !focused && !reduceMotion) {
            delay(HERO_CAROUSEL_ADVANCE_MS)
            page = (page + 1) % games.size
        }
    }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 6.dp),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
    ) {
        SectionHeader(
            title = title,
            subtitle = stringResource(R.string.store_coming_next_subtitle),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                games.forEachIndexed { index, _ ->
                    Box(
                        Modifier
                            .width(if (index == page) 22.dp else 7.dp)
                            .height(5.dp)
                            .clip(CircleShape)
                            .background(if (index == page) MaterialTheme.colorScheme.primary else TextMuted.copy(alpha = 0.32f)),
                    )
                }
            }
        }
        AnimatedContent(
            targetState = page,
            transitionSpec = {
                fadeIn(tween(if (reduceMotion) 0 else OpenNowMotion.DurationStandard)) togetherWith
                    fadeOut(tween(if (reduceMotion) 0 else OpenNowMotion.DurationFast))
            },
            label = "coming-next-carousel",
        ) { targetPage ->
            val featured = games[targetPage.coerceIn(games.indices)]
            val shape = RoundedCornerShape(if (settings.expressiveUi) 24.dp else 16.dp)
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    // Aspect ratio rather than a fixed height, so the hero scales with the screen
                    // instead of dominating a small phone and looking stunted on a tablet.
                    .aspectRatio(heroAspectRatio(tvProfile, landscape))
                    .onFocusChanged { focused = it.isFocused || it.hasFocus }
                    .border(
                        width = if (focused) 3.dp else 1.dp,
                        color = when {
                            enhancedControllerFocus -> Color.Transparent
                            focused -> Color.White
                            else -> Color.White.copy(alpha = 0.08f)
                        },
                        shape = shape,
                    )
                    .onPreviewKeyEvent { event ->
                        if (event.type != KeyEventType.KeyUp) return@onPreviewKeyEvent false
                        when {
                            controllerActionMode && event.key == Key.DirectionLeft && games.size > 1 -> {
                                page = (page - 1 + games.size) % games.size
                                true
                            }
                            controllerActionMode && event.key == Key.DirectionRight && games.size > 1 -> {
                                page = (page + 1) % games.size
                                true
                            }
                            !tvProfile && controllerActionMode && handleCatalogControllerAction(
                                event = event,
                                onFavorite = { onFavorite(featured.id) },
                                onPlay = { onPlay(featured) },
                            ) -> true
                            isTvActivateKey(event) -> {
                                onSelect(featured)
                                true
                            }
                            else -> false
                        }
                    }
                    .focusable()
                    .combinedClickable(
                        onClick = { onSelect(featured) },
                        onLongClick = { onChooseStore(featured) },
                        onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                    ),
                shape = shape,
                color = Panel,
                tonalElevation = if (focused) 5.dp else 0.dp,
                shadowElevation = if (focused) 9.dp else 1.dp,
            ) {
                Box(Modifier.fillMaxSize()) {
                    UrlImage(gameHeroImageUrl(context, featured), Modifier.fillMaxSize())
                    // Horizontal scrim carries the title block; the vertical one settles the art
                    // into the surface below so the hero reads as part of the page, not a sticker.
                    Box(
                        Modifier
                            .matchParentSize()
                            .background(
                                Brush.horizontalGradient(
                                    listOf(Color.Black.copy(alpha = 0.88f), Color.Black.copy(alpha = 0.3f), Color.Transparent),
                                ),
                            ),
                    )
                    Box(
                        Modifier
                            .matchParentSize()
                            .background(
                                Brush.verticalGradient(
                                    0.45f to Color.Transparent,
                                    1f to Background.copy(alpha = 0.85f),
                                ),
                            ),
                    )
                    Column(
                        Modifier
                            .align(Alignment.BottomStart)
                            .fillMaxWidth(0.66f)
                            .padding(18.dp),
                        verticalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        Text(
                            featured.title,
                            color = Color.White,
                            style = when {
                                // Across a room the hero title is the only thing readable at a
                                // glance, so TV gets the display scale.
                                tvProfile -> MaterialTheme.typography.displaySmall
                                landscape -> MaterialTheme.typography.headlineSmall
                                else -> MaterialTheme.typography.headlineMedium
                            },
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            listOfNotNull(featured.publisherName, displayStoresForGame(featured).takeIf { it.isNotBlank() })
                                .distinct()
                                .joinToString("  •  "),
                            color = Color.White.copy(alpha = 0.72f),
                            style = MaterialTheme.typography.labelMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (shouldShowCatalogCardActions(tvProfile, controllerActionMode)) {
                        FavoriteIconButton(
                            favorite = featured.id in favoriteIds,
                            onClick = { onFavorite(featured.id) },
                            modifier = Modifier.align(Alignment.TopEnd).padding(14.dp),
                            size = 38.dp,
                        )
                    }
                    ControllerFocusFrame(
                        visible = enhancedControllerFocus,
                        animate = settings.controllerBackgroundAnimations,
                        cornerRadius = if (settings.expressiveUi) 24.dp else 16.dp,
                    )
                }
            }
        }
    }
}

// StoreRailSection (was OpenNowScreens.kt:4430)
@Composable
internal fun StoreRailSection(
    title: String,
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
        SectionHeader(title = title)
        // The row breaks out of the grid's edge padding and re-applies it as content padding, so
        // cards scroll all the way under the screen edge instead of stopping short of it. The
        // header stays aligned to the content because the bleed is only on the row.
        BoxWithConstraints(Modifier.horizontalBleed(OpenNowSpacing.ScreenEdge)) {
            val spacing = OpenNowSpacing.md
            val baseCardWidth = storeRailCardWidth(tvProfile, landscapeLayout)
            val contentInset = OpenNowSpacing.ScreenEdge
            val visibleCount = storeRailVisibleCardCount(
                availableWidthDp = maxWidth.value - contentInset.value * 2f,
                baseCardWidthDp = baseCardWidth.value,
                spacingDp = spacing.value,
                cardScale = settings.posterSizeScale,
            )
            // Leave a sliver of the next card showing — the standard cue that a row keeps going.
            val cardWidth = ((maxWidth.value - contentInset.value * 2f - spacing.value * visibleCount) /
                (visibleCount + PEEK_CARD_FRACTION))
                .coerceAtLeast(1f)
                .dp
            val artworkOnly = shouldUseArtworkOnlyCatalogCards(tvProfile, controllerActionMode)
            CatalogFocusScope {
                LazyRow(
                    horizontalArrangement = Arrangement.spacedBy(spacing),
                    contentPadding = PaddingValues(horizontal = contentInset),
                ) {
                    items(games, key = { storeRailGameKey(it) }) { game ->
                        if (tvProfile) {
                            // TV Design Kit wide card — 16:9 landscape cards for the TV rails.
                            TvWideCard(
                                game = game,
                                favorite = game.id in favoriteIds,
                                width = TV_RAIL_WIDE_CARD_WIDTH,
                                onClick = { onSelect(game) },
                                onFavorite = { onFavorite(game.id) },
                                onPlay = { onPlay(game) },
                                onChooseStore = { onChooseStore(game) },
                            )
                        } else {
                            StoreRailGameCard(
                                game = game,
                                favorite = game.id in favoriteIds,
                                tvProfile = tvProfile,
                                expressiveUi = settings.expressiveUi,
                                controllerBackgroundAnimations = settings.controllerBackgroundAnimations,
                                width = cardWidth,
                                controllerActionMode = controllerActionMode,
                                showGameStoreLabels = !artworkOnly && shouldShowGameStoreLabels(
                                    tvProfile = tvProfile,
                                    enabled = settings.showGameStoreLabels,
                                ),
                                showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                                    tvProfile = tvProfile,
                                    enabled = settings.showCardTitles,
                                ),
                                // Same M3 media-card form as the grid, so the rail and grid agree.
                                mediaCard = !tvProfile && windowWidthSizeClassOf(maxWidth).isAtLeastMedium,
                                onSelect = onSelect,
                                onFavorite = onFavorite,
                                onPlay = onPlay,
                                onChooseStore = onChooseStore,
                            )
                        }
                    }
                }
            }
        }
    }
}

// StoreRailGameCard (was OpenNowScreens.kt:4514)
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun StoreRailGameCard(
    game: GameInfo,
    favorite: Boolean,
    tvProfile: Boolean,
    expressiveUi: Boolean,
    controllerBackgroundAnimations: Boolean,
    width: Dp,
    controllerActionMode: Boolean,
    showCardTitles: Boolean,
    showGameStoreLabels: Boolean,
    /** Medium+ handhelds render the M3 media-card form: caption inside the card below the art. */
    mediaCard: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val shape = RoundedCornerShape(if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    val actionButtonSize = 34.dp
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val reduceMotion = LocalReduceMotion.current
    val cardScale by animateFloatAsState(
        targetValue = when {
            pressed -> 0.965f
            focused || hovered -> when {
                tvProfile -> 1.08f
                controllerActionMode -> 1f
                else -> 1.035f
            }
            else -> 1f
        },
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "rail-card-scale",
    )
    val dimAlpha = rememberCatalogCardAlpha(focused = focused, tvProfile = tvProfile)
    Surface(
        modifier = Modifier
            .width(width)
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp)
            .then(
                // In the media-card form the ratio lives on the art box so the caption can sit
                // below it; the poster form keeps the ratio on the whole card.
                if (mediaCard) Modifier
                else Modifier.aspectRatio(if (tvProfile) 1f else GAME_BOX_ART_ASPECT_RATIO),
            )
            .graphicsLayer {
                scaleX = cardScale
                scaleY = cardScale
                alpha = dimAlpha
            }
            .semantics(mergeDescendants = true) {
                contentDescription = game.title
                role = Role.Button
            }
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .border(
                width = if (focused) 3.dp else 1.dp,
                color = when {
                    enhancedControllerFocus -> Color.Transparent
                    focused -> MaterialTheme.colorScheme.primary
                    else -> Color.Transparent
                },
                shape = shape,
            )
            .onPreviewKeyEvent { event ->
                when {
                    !tvProfile && controllerActionMode && handleCatalogControllerAction(
                        event = event,
                        onFavorite = { onFavorite(game.id) },
                        onPlay = { onPlay(game) },
                    ) -> true
                    isTvActivateKey(event) -> {
                        onSelect(game)
                        true
                    }
                    else -> handleDpadFocusMove(event, focusManager)
                }
            }
            .focusable(interactionSource = interaction),
        shape = shape,
        color = OpenNowPalette.ImagePlaceholder,
        tonalElevation = if (focused) 4.dp else 0.dp,
        shadowElevation = if (focused) 8.dp else 1.dp,
    ) {
        // The whole card is clickable — art and (in the media-card form) caption. Key handling
        // stays on the Surface modifier above, matching the grid GameCard pattern. The clip keeps
        // art and caption inside the rounded shape — Surface does not clip its children by itself.
        Column(
            Modifier
                .fillMaxWidth()
                .then(if (mediaCard) Modifier else Modifier.fillMaxSize())
                .clip(shape)
                .combinedClickable(
                    interactionSource = interaction,
                    indication = null,
                    onClick = { onSelect(game) },
                    onLongClick = { onChooseStore(game) },
                    onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                ),
        ) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .then(
                        if (mediaCard) {
                            Modifier.aspectRatio(if (tvProfile) 1f else GAME_BOX_ART_ASPECT_RATIO)
                        } else {
                            Modifier.fillMaxSize()
                        },
                    ),
            ) {
                GameCardArtworkContent(
                    game = game,
                    tvProfile = tvProfile,
                    thumbnailFavoriteOverlay = true,
                    favorite = favorite,
                    controllerActionMode = controllerActionMode,
                    overlayActionSize = actionButtonSize,
                    overlayActionPadding = 6.dp,
                    enhancedControllerFocus = enhancedControllerFocus,
                    controllerBackgroundAnimations = controllerBackgroundAnimations,
                    reduceMotion = reduceMotion,
                    expressiveUi = expressiveUi,
                    onFavorite = { onFavorite(game.id) },
                )
            }
            if (mediaCard && (showCardTitles || showGameStoreLabels)) {
                GameCardMediaCaption(
                    game = game,
                    showCardTitles = showCardTitles,
                    showGameStoreLabels = showGameStoreLabels,
                )
            }
        }
    }
}

/**
 * The three rails that open the store, kept distinct.
 *
 * These used to be flattened into one "Jump back in" rail of `queued + favorites + recent + owned`
 * capped at 14 items — which meant genuinely recently-played games sat third in priority and were
 * routinely pushed off-screen by favourites, and the tail was padded with owned games the user had
 * never launched. Owned games are the Library tab's job, so they are dropped here entirely.
 */

// StoreStartRailGroups (was OpenNowScreens.kt:4673)
internal data class StoreStartRailGroups(
    val continuePlaying: List<GameInfo>,
    val inQueue: List<GameInfo>,
    val favorites: List<GameInfo>,
) {
    val allGames: List<GameInfo> get() = continuePlaying + inQueue + favorites
    val isEmpty: Boolean get() = continuePlaying.isEmpty() && inQueue.isEmpty() && favorites.isEmpty()
}

// storeStartRailGroups (was OpenNowScreens.kt:4682)
internal fun storeStartRailGroups(
    games: List<GameInfo>,
    libraryGames: List<GameInfo>,
    favoriteIds: List<String>,
    queuedGameKeys: List<String>,
): StoreStartRailGroups {
    val favoriteSet = favoriteIds.toSet()
    val combined = distinctStoreGames(libraryGames + games)
    val byKey = combined.associateBy(::storeRailGameKey)

    val continuePlaying = combined
        .filter { it.recentPlaySortKey() != null }
        .sortedByDescending { it.recentPlaySortKey() }
        .take(CONTINUE_PLAYING_RAIL_LIMIT)
    val continueKeys = continuePlaying.map(::storeRailGameKey).toSet()

    val inQueue = queuedGameKeys
        .mapNotNull(byKey::get)
        .filterNot { storeRailGameKey(it) in continueKeys }
        .take(STORE_RAIL_GAME_LIMIT)
    val shownKeys = continueKeys + inQueue.map(::storeRailGameKey)

    // Favourites already visible above would just be a second sighting of the same card.
    val favorites = combined
        .filter { it.id in favoriteSet }
        .filterNot { storeRailGameKey(it) in shownKeys }
        .take(STORE_RAIL_GAME_LIMIT)

    return StoreStartRailGroups(continuePlaying, inQueue, favorites)
}

// comingNextStoreGames (was OpenNowScreens.kt:4713)
internal fun comingNextStoreGames(
    games: List<GameInfo>,
    excludedGames: List<GameInfo>,
): List<GameInfo> {
    val excludedKeys = excludedGames.map(::storeRailGameKey).toSet()
    return distinctStoreGames(games)
        .filterNot { storeRailGameKey(it) in excludedKeys }
        .filter(GameInfo::isNewOrUpdatedCatalogSection)
        .take(STORE_RAIL_GAME_LIMIT)
}

// isNewOrUpdatedCatalogSection (was OpenNowScreens.kt:4724)
internal fun GameInfo.isNewOrUpdatedCatalogSection(): Boolean {
    val section = catalogSectionTitle?.lowercase(Locale.US)?.trim().orEmpty()
    return section.contains("new") ||
        section.contains("recent") ||
        section.contains("updated") ||
        section.contains("just added")
}

// recentPlaySortKey (was OpenNowScreens.kt:4732)
internal fun GameInfo.recentPlaySortKey(): String? =
    listOfNotNull(
        lastPlayed?.takeIf { it.isNotBlank() },
        variants.mapNotNull { it.lastPlayedDate?.takeIf(String::isNotBlank) }.maxOrNull(),
    ).maxOrNull()

// distinctStoreGames (was OpenNowScreens.kt:4738)
internal fun distinctStoreGames(games: List<GameInfo>): List<GameInfo> {
    val byKey = linkedMapOf<String, GameInfo>()
    games.forEach { game ->
        byKey.putIfAbsent(storeRailGameKey(game), game)
    }
    return byKey.values.toList()
}

// storeRailGameKey (was OpenNowScreens.kt:4746)
internal fun storeRailGameKey(game: GameInfo): String =
    gameTrackingKey(game)

// STORE_RAIL_GAME_LIMIT (was OpenNowScreens.kt:4749)
internal const val STORE_RAIL_GAME_LIMIT = 14

/** Recently-played is a short list by nature — padding it out defeats the point of the rail. */

// CONTINUE_PLAYING_RAIL_LIMIT (was OpenNowScreens.kt:4752)
internal const val CONTINUE_PLAYING_RAIL_LIMIT = 12

/** Five hero pages, five indicator pills. Fourteen was a rash of dots. */

// HERO_CAROUSEL_PAGE_LIMIT (was OpenNowScreens.kt:4755)
internal const val HERO_CAROUSEL_PAGE_LIMIT = 5

// HERO_CAROUSEL_ADVANCE_MS (was OpenNowScreens.kt:4757)
internal const val HERO_CAROUSEL_ADVANCE_MS = 6_000L

/**
 * Wider on surfaces that are already wide, so the hero stays a banner rather than becoming a wall.
 */

// heroAspectRatio (was OpenNowScreens.kt:4762)
internal fun heroAspectRatio(tvProfile: Boolean, landscape: Boolean): Float = when {
    tvProfile -> 16f / 6f
    landscape -> 16f / 5f
    else -> 16f / 7f
}

// GAME_BOX_ART_ASPECT_RATIO (was OpenNowScreens.kt:4767)
internal const val GAME_BOX_ART_ASPECT_RATIO = 628f / 888f

// shouldShowEnhancedControllerFocus (was OpenNowScreens.kt:4769)
internal fun shouldShowEnhancedControllerFocus(
    focused: Boolean,
    tvProfile: Boolean,
    controllerActionMode: Boolean,
): Boolean = focused && (tvProfile || controllerActionMode)

// shouldInitiallyFocusGameDetailsPlay (was OpenNowScreens.kt:4775)
internal fun shouldInitiallyFocusGameDetailsPlay(tvProfile: Boolean): Boolean = tvProfile

// controllerFocusPulseStrokeWidthDp (was OpenNowScreens.kt:4777)
internal fun controllerFocusPulseStrokeWidthDp(progress: Float): Float =
    4f + (9f * progress.coerceIn(0f, 1f))

// controllerFocusPulseAlpha (was OpenNowScreens.kt:4780)
internal fun controllerFocusPulseAlpha(progress: Float): Float {
    val remaining = 1f - progress.coerceIn(0f, 1f)
    return 0.58f * remaining * remaining
}

// GameGridSpec (was OpenNowScreens.kt:4785)
internal data class GameGridSpec(
    val cells: GridCells,
    /** Only used to size skeleton placeholder runs; the real column count is the grid's to decide. */
    val estimatedColumns: Int,
    val horizontalSpacing: Dp,
    val verticalSpacing: Dp,
    val contentPadding: PaddingValues,
    val squareCards: Boolean,
)

/** How much of the next card stays visible past the last fully-visible one. */

// PEEK_CARD_FRACTION (was OpenNowScreens.kt:4796)
internal const val PEEK_CARD_FRACTION = 0.28f

/**
 * Number of catalog cards currently holding focus inside the surrounding grid or rail. A count
 * rather than a flag so that handing focus from one card to its neighbour — where the old card
 * reports losing focus in the same frame the new one reports gaining it — never dips to "nothing
 * is focused" and flickers the dim.
 */

// LocalCatalogFocusCount (was OpenNowScreens.kt:4804)
internal val LocalCatalogFocusCount = compositionLocalOf<MutableIntState?> { null }

/**
 * Scopes the focus count to one grid or one rail, so focusing a card in the grid doesn't dim the
 * rails above it.
 */

// CatalogFocusScope (was OpenNowScreens.kt:4810)
@Composable
internal fun CatalogFocusScope(content: @Composable () -> Unit) {
    val count = remember { mutableIntStateOf(0) }
    CompositionLocalProvider(LocalCatalogFocusCount provides count, content = content)
}

/** Alpha applied to unfocused cards while a sibling is focused. TV only. */

// TV_UNFOCUSED_CARD_ALPHA (was OpenNowScreens.kt:4817)
internal const val TV_UNFOCUSED_CARD_ALPHA = 0.55f

/**
 * Registers this card's focus in the surrounding [CatalogFocusScope] and returns the alpha it
 * should draw at. Dimming the neighbours is what makes the focus cursor readable from across a
 * room — on TV a border and a scale change alone still leave a wall of equally bright artwork.
 */

// rememberCatalogCardAlpha (was OpenNowScreens.kt:4824)
@Composable
internal fun rememberCatalogCardAlpha(focused: Boolean, tvProfile: Boolean): Float {
    val count = LocalCatalogFocusCount.current
    DisposableEffect(focused, count) {
        if (focused) count?.intValue = (count?.intValue ?: 0) + 1
        onDispose {
            if (focused) count?.intValue = ((count?.intValue ?: 1) - 1).coerceAtLeast(0)
        }
    }
    if (!tvProfile) return 1f
    val anyFocused = (count?.intValue ?: 0) > 0
    val target = if (anyFocused && !focused) TV_UNFOCUSED_CARD_ALPHA else 1f
    val reduceMotion = LocalReduceMotion.current
    val alpha by animateFloatAsState(
        targetValue = target,
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "catalog-card-dim",
    )
    return alpha
}

/**
 * Lets a child extend [bleed] past its parent's bounds on both sides without reporting the extra
 * width upward — the standard way to make a horizontally scrolling row run edge to edge inside a
 * padded container.
 */

// horizontalBleed (was OpenNowScreens.kt:4853)
internal fun Modifier.horizontalBleed(bleed: Dp): Modifier = this.layout { measurable, constraints ->
    val extra = bleed.roundToPx() * 2
    val placeable = measurable.measure(
        constraints.copy(
            maxWidth = if (constraints.hasBoundedWidth) constraints.maxWidth + extra else constraints.maxWidth,
        ),
    )
    val reportedWidth = (placeable.width - extra).coerceAtLeast(0)
    layout(reportedWidth, placeable.height) {
        placeable.place(-bleed.roundToPx(), 0)
    }
}

// storeRailCardWidth (was OpenNowScreens.kt:4866)
internal fun storeRailCardWidth(tvProfile: Boolean, landscapeLayout: Boolean): Dp =
    when {
        tvProfile -> 158.dp
        landscapeLayout -> 146.dp
        else -> 142.dp
    }

/**
 * TV wide cards (16:9, TV Design Kit) are deliberately much larger than the square phone-rail
 * cards, so the rail shows fewer, richer tiles that read across the room.
 */

// TV_RAIL_WIDE_CARD_WIDTH (was OpenNowScreens.kt:4877)
internal val TV_RAIL_WIDE_CARD_WIDTH = 300.dp

// ControllerFocusFrame (was OpenNowScreens.kt:4879)
@Composable
internal fun BoxScope.ControllerFocusFrame(
    visible: Boolean,
    animate: Boolean,
    cornerRadius: Dp,
) {
    if (!visible) return
    val accent = MaterialTheme.colorScheme.primary
    if (animate) {
        val transition = rememberInfiniteTransition(label = "controller-focus-pulse")
        val pulseProgress by transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 1_100, easing = FastOutSlowInEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "controller-focus-pulse-progress",
        )
        ControllerFocusFrameCanvas(
            accent = accent,
            cornerRadius = cornerRadius,
            pulseProgress = pulseProgress,
        )
    } else {
        ControllerFocusFrameCanvas(
            accent = accent,
            cornerRadius = cornerRadius,
            pulseProgress = null,
        )
    }
}

// ControllerFocusFrameCanvas (was OpenNowScreens.kt:4912)
@Composable
internal fun BoxScope.ControllerFocusFrameCanvas(
    accent: Color,
    cornerRadius: Dp,
    pulseProgress: Float?,
) {
    Canvas(Modifier.matchParentSize().padding(2.dp)) {
        val outerRadius = (cornerRadius - 2.dp).toPx().coerceAtLeast(0f)

        // Keep every animated pixel on the card edge. The pulse expands by
        // widening the outer stroke and fading; it never creates an inner box.
        drawRoundRect(
            color = accent.copy(alpha = 0.18f),
            cornerRadius = CornerRadius(outerRadius, outerRadius),
            style = Stroke(width = 8.dp.toPx()),
        )
        pulseProgress?.let { progress ->
            drawRoundRect(
                color = accent.copy(alpha = controllerFocusPulseAlpha(progress)),
                cornerRadius = CornerRadius(outerRadius, outerRadius),
                style = Stroke(width = controllerFocusPulseStrokeWidthDp(progress).dp.toPx()),
            )
        }
        drawRoundRect(
            color = Color.White.copy(alpha = 0.96f),
            cornerRadius = CornerRadius(outerRadius, outerRadius),
            style = Stroke(width = 2.dp.toPx()),
        )
    }
}

/**
 * Cell widths the grid aims for at `posterSizeScale == 1`. These are minimums fed to
 * [GridCells.Adaptive], not column counts: the grid fits as many as will hold and shares the
 * remainder out evenly.
 *
 * The previous implementation picked a column count from a table of hardcoded dp breakpoints, so a
 * 360dp budget phone and a 411dp Pixel both got exactly 3 columns — cards ended up 15% wider on one
 * than the other, and gutters never adapted at all. Foldables, tablets, DeX and split-screen were
 * all served by the same four buckets.
 */

// GRID_CELL_WIDTH_PORTRAIT (was OpenNowScreens.kt:4953)
internal val GRID_CELL_WIDTH_PORTRAIT = 96.dp

// GRID_CELL_WIDTH_LANDSCAPE (was OpenNowScreens.kt:4954)
internal val GRID_CELL_WIDTH_LANDSCAPE = 112.dp

// GRID_CELL_WIDTH_TV (was OpenNowScreens.kt:4955)
internal val GRID_CELL_WIDTH_TV = 158.dp

/** M3 adaptive: tablets keep cards substantial instead of inheriting the tiny phone cell. */

// GRID_CELL_WIDTH_TABLET (was OpenNowScreens.kt:4958)
internal val GRID_CELL_WIDTH_TABLET = 168.dp

/** Compact mode shrinks the target cell rather than switching to a separate size table. */

// COMPACT_CELL_WIDTH_FACTOR (was OpenNowScreens.kt:4961)
internal const val COMPACT_CELL_WIDTH_FACTOR = 0.88f

// CATALOG_CONTROLLER_FOCUS_INSET (was OpenNowScreens.kt:4962)
internal val CATALOG_CONTROLLER_FOCUS_INSET = 8.dp

// gameGridSpec (was OpenNowScreens.kt:4964)
internal fun gameGridSpec(
    maxWidth: androidx.compose.ui.unit.Dp,
    compact: Boolean,
    landscapeLayout: Boolean,
    settings: AppSettings,
    handheldLayout: Boolean,
): GameGridSpec {
    val horizontalSpacing = if (compact) OpenNowSpacing.sm else OpenNowSpacing.GridGutter
    val verticalSpacing = if (compact) OpenNowSpacing.md else OpenNowSpacing.GridRowGap
    // M3 large screens use 24dp gutters instead of the phone 16dp. Handheld only — the TV
    // experience keeps its own gutters from the TV design system.
    val horizontalPadding = if (handheldLayout && windowWidthSizeClassOf(maxWidth).isAtLeastMedium) OpenNowSpacing.xl else OpenNowSpacing.ScreenEdge

    val baseCellWidth = when {
        !handheldLayout -> GRID_CELL_WIDTH_TV
        // M3 adaptive: medium+ screens keep cards substantial rather than shrinking the phone cell.
        windowWidthSizeClassOf(maxWidth).isAtLeastMedium -> GRID_CELL_WIDTH_TABLET
        landscapeLayout -> GRID_CELL_WIDTH_LANDSCAPE
        else -> GRID_CELL_WIDTH_PORTRAIT
    }
    // posterSizeScale is persisted user state and keeps its existing meaning: larger scale means
    // larger cards, which now falls out of a wider target cell instead of a divided column count.
    val scale = settings.posterSizeScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE)
    val cellWidth = (baseCellWidth * scale * if (compact) COMPACT_CELL_WIDTH_FACTOR else 1f)
        .coerceIn(64.dp, 240.dp)

    val available = (maxWidth - horizontalPadding * 2).coerceAtLeast(cellWidth)
    val estimatedColumns = ((available + horizontalSpacing) / (cellWidth + horizontalSpacing))
        .toInt()
        .coerceIn(1, 12)

    return GameGridSpec(
        cells = GridCells.Adaptive(minSize = cellWidth),
        estimatedColumns = estimatedColumns,
        horizontalSpacing = horizontalSpacing,
        verticalSpacing = verticalSpacing,
        contentPadding = PaddingValues(horizontal = horizontalPadding, vertical = OpenNowSpacing.md),
        // TV grid cards match the TV rail cards, which have always been square — this is the shape
        // NVIDIA's tvCardImageUrl assets are cut for.
        squareCards = !handheldLayout,
    )
}

// appContentEdgePaddingDp (was OpenNowScreens.kt:5007)
internal fun appContentEdgePaddingDp(
    settings: AppSettings,
    inStream: Boolean,
    tvProfile: Boolean,
): Float = if (inStream || !tvProfile) 0f else settings.tvSafeAreaPaddingDp.coerceIn(0f, 120f)

// storeRailVisibleCardCount (was OpenNowScreens.kt:5013)
internal fun storeRailVisibleCardCount(
    availableWidthDp: Float,
    baseCardWidthDp: Float,
    spacingDp: Float,
    cardScale: Float,
): Int {
    val scaledCardWidth = baseCardWidthDp * cardScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE)
    return ((availableWidthDp + spacingDp) / (scaledCardWidth + spacingDp))
        .toInt()
        .coerceAtLeast(1)
}

// GameCard (was OpenNowScreens.kt:5025)
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun GameCard(
    game: GameInfo,
    favorite: Boolean,
    tvProfile: Boolean,
    expressiveUi: Boolean,
    controllerBackgroundAnimations: Boolean,
    showGameStoreLabels: Boolean,
    showCardTitles: Boolean,
    squareCard: Boolean,
    thumbnailFavoriteOverlay: Boolean,
    controllerActionMode: Boolean,
    /** Medium+ handhelds render the M3 media-card form: caption inside the card below the art. */
    mediaCard: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val cardShape = RoundedCornerShape(if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    val handheldPosterCard = !tvProfile
    val launcherTile = handheldPosterCard && thumbnailFavoriteOverlay
    val overlayActionSize = if (launcherTile) 34.dp else 44.dp
    val overlayActionPadding = if (launcherTile) 6.dp else 8.dp
    val enhancedControllerFocus = shouldShowEnhancedControllerFocus(
        focused = focused,
        tvProfile = tvProfile,
        controllerActionMode = controllerActionMode,
    )
    // On phones the caption lives outside the poster so the artwork stays clean in the tight grid;
    // on medium+ screens the media-card form puts it inside the card instead.
    val showCaption = handheldPosterCard && !mediaCard && (showCardTitles || showGameStoreLabels)

    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val reduceMotion = LocalReduceMotion.current
    val cardScale by animateFloatAsState(
        targetValue = when {
            pressed -> 0.965f
            // A bigger lift on TV: from three metres a border change is nearly invisible, but a
            // card growing out of the grid is unmistakable.
            focused || hovered -> when {
                tvProfile -> 1.08f
                controllerActionMode -> 1f
                else -> 1.035f
            }
            else -> 1f
        },
        animationSpec = tween(
            durationMillis = if (reduceMotion) 0 else OpenNowMotion.DurationStandard,
            easing = OpenNowMotion.EasingStandard,
        ),
        label = "game-card-scale",
    )
    val dimAlpha = rememberCatalogCardAlpha(focused = focused, tvProfile = tvProfile)

    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = if (tvProfile) CATALOG_CONTROLLER_FOCUS_INSET else 0.dp)
            .graphicsLayer {
                scaleX = cardScale
                scaleY = cardScale
                alpha = dimAlpha
            }
            // One merged node per card. Without this TalkBack reads nothing at all here: UrlImage
            // passes a null contentDescription and phone cards carry no title text of their own.
            .semantics(mergeDescendants = true) {
                contentDescription = game.title
                role = Role.Button
            },
    ) {
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .then(
                    if (mediaCard) Modifier
                    else if (squareCard) Modifier.aspectRatio(1f)
                    else Modifier.aspectRatio(GAME_BOX_ART_ASPECT_RATIO),
                )
                .onFocusChanged { focused = it.isFocused || it.hasFocus }
                .border(
                    width = if (focused) 3.dp else 1.dp,
                    color = when {
                        enhancedControllerFocus -> Color.Transparent
                        focused -> MaterialTheme.colorScheme.primary
                        else -> Color.Transparent
                    },
                    shape = cardShape,
                )
                .onPreviewKeyEvent { event ->
                    when {
                        !tvProfile && controllerActionMode && handleCatalogControllerAction(
                            event = event,
                            onFavorite = { onFavorite(game.id) },
                            onPlay = { onPlay(game) },
                        ) -> true
                        isTvActivateKey(event) -> {
                            onSelect(game)
                            true
                        }
                        else -> handleDpadFocusMove(event, focusManager)
                    }
                }
                .focusable(interactionSource = interaction),
            colors = CardDefaults.cardColors(
                containerColor = if (expressiveUi) MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f) else Panel,
            ),
            elevation = CardDefaults.cardElevation(defaultElevation = if (focused) 8.dp else 0.dp),
            shape = cardShape,
        ) {
            // The whole card is clickable — art and (in the media-card form) caption. The
            // controller key handling stays on the Card modifier above, so focus isn't affected.
            Column(
                Modifier
                    .fillMaxWidth()
                    .then(if (mediaCard) Modifier else Modifier.fillMaxSize())
                    .combinedClickable(
                        interactionSource = interaction,
                        indication = null,
                        onClick = { onSelect(game) },
                        onLongClick = { onChooseStore(game) },
                        onLongClickLabel = stringResource(R.string.store_selector_play_long_press),
                    ),
            ) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .then(
                            if (mediaCard) {
                                if (squareCard) Modifier.aspectRatio(1f)
                                else Modifier.aspectRatio(GAME_BOX_ART_ASPECT_RATIO)
                            } else {
                                Modifier.fillMaxSize()
                            },
                        ),
                ) {
                    GameCardArtworkContent(
                        game = game,
                        tvProfile = tvProfile,
                        thumbnailFavoriteOverlay = thumbnailFavoriteOverlay,
                        favorite = favorite,
                        controllerActionMode = controllerActionMode,
                        overlayActionSize = overlayActionSize,
                        overlayActionPadding = overlayActionPadding,
                        enhancedControllerFocus = enhancedControllerFocus,
                        controllerBackgroundAnimations = controllerBackgroundAnimations,
                        reduceMotion = reduceMotion,
                        expressiveUi = expressiveUi,
                        onFavorite = { onFavorite(game.id) },
                    )
                }
                if (mediaCard && (showCardTitles || showGameStoreLabels)) {
                    GameCardMediaCaption(
                        game = game,
                        showCardTitles = showCardTitles,
                        showGameStoreLabels = showGameStoreLabels,
                    )
                }
            }
        }
        if (showCaption) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(top = OpenNowSpacing.sm),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (showCardTitles) {
                    Text(
                        game.title,
                        color = TextPrimary,
                        style = MaterialTheme.typography.titleMedium,
                        // minLines keeps every row in the grid aligned regardless of title length.
                        maxLines = 2,
                        minLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (showGameStoreLabels) {
                    Text(
                        displayStoresForGame(game),
                        color = TextMuted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
        }
    }
}

/**
 * Games sharing the most genres with [game], for the tablet "More like this" pane.
 * The store catalog is the pool; if it has not loaded yet the search/catalog fallback is used.
 */

// similarGamesFor (was OpenNowScreens.kt:5226)
internal fun similarGamesFor(
    game: GameInfo,
    catalog: List<GameInfo>,
    limit: Int = 8,
): List<GameInfo> {
    val ownGenres = game.genres.toSet()
    if (ownGenres.isEmpty()) return emptyList()
    return catalog
        .asSequence()
        .filter { it.id != game.id }
        .map { candidate -> candidate to candidate.genres.count { it in ownGenres } }
        .filter { (_, overlap) -> overlap > 0 }
        .sortedByDescending { (_, overlap) -> overlap }
        .take(limit)
        .map { (candidate, _) -> candidate }
        .toList()
}

// catalogCardImageUrl (was OpenNowScreens.kt:5244)
internal fun catalogCardImageUrl(game: GameInfo, tvProfile: Boolean): String? {
    val source = if (tvProfile) {
        game.tvCardImageUrl?.takeIf { it.isNotBlank() }
            ?: game.imageUrl?.takeIf { it.isNotBlank() }
    } else {
        game.imageUrl
            ?.takeIf { it.isNotBlank() }
            ?.takeIf { !it.contains("img.nvidiagrid.net") || it.contains("/GAME_BOX_ART_") }
    } ?: return null
    return if (tvProfile) optimizedNvidiaImageUrl(source, 272) else source
}

// shouldOverlayCatalogCardTitle (was OpenNowScreens.kt:5256)
@Suppress("UNUSED_PARAMETER")
internal fun shouldOverlayCatalogCardTitle(tvProfile: Boolean): Boolean = false

// shouldUseArtworkOnlyCatalogCards (was OpenNowScreens.kt:5259)
internal fun shouldUseArtworkOnlyCatalogCards(tvProfile: Boolean, controllerActionMode: Boolean): Boolean =
    tvProfile || controllerActionMode

// shouldShowCatalogCardActions (was OpenNowScreens.kt:5262)
internal fun shouldShowCatalogCardActions(tvProfile: Boolean, controllerActionMode: Boolean): Boolean =
    !tvProfile && !controllerActionMode

// shouldShowGameStoreLabels (was OpenNowScreens.kt:5265)
internal fun shouldShowGameStoreLabels(tvProfile: Boolean, enabled: Boolean): Boolean =
    enabled && !tvProfile

/** Titles may be captioned on touch handhelds; controller-first layouts suppress them upstream. */

// shouldShowCatalogCardTitles (was OpenNowScreens.kt:5269)
internal fun shouldShowCatalogCardTitles(tvProfile: Boolean, enabled: Boolean): Boolean =
    enabled && !tvProfile

// GameCardTitleOverlay (was OpenNowScreens.kt:5272)
@Composable
internal fun GameCardTitleOverlay(title: String) {
    Box(
        Modifier
            .fillMaxSize()
            .background(GameCardOverlayGradient),
        contentAlignment = Alignment.BottomStart,
    ) {
        Text(
            text = title,
            color = Color.White,
            fontWeight = FontWeight.ExtraBold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
        )
    }
}

/**
 * The art layer of a [GameCard] — image, optional title overlay, favorite button, and the
 * controller focus frame. Shared by the phone poster and the tablet media-card forms.
 */

// GameCardArtworkContent (was OpenNowScreens.kt:5296)
@Composable
internal fun BoxScope.GameCardArtworkContent(
    game: GameInfo,
    tvProfile: Boolean,
    thumbnailFavoriteOverlay: Boolean,
    favorite: Boolean,
    controllerActionMode: Boolean,
    overlayActionSize: Dp,
    overlayActionPadding: Dp,
    enhancedControllerFocus: Boolean,
    controllerBackgroundAnimations: Boolean,
    reduceMotion: Boolean,
    expressiveUi: Boolean,
    onFavorite: () -> Unit,
) {
    UrlImage(
        catalogCardImageUrl(game, tvProfile),
        Modifier.fillMaxSize(),
        // Always Crop. The card is already locked to NVIDIA's box-art ratio, so for
        // correctly-cut art this is identical to Fit; when the CDN returns something
        // off-ratio, Fit pillarboxed it against a flat swatch and Crop simply trims.
        contentScale = ContentScale.Crop,
    )
    if (shouldOverlayCatalogCardTitle(tvProfile)) {
        GameCardTitleOverlay(game.title)
    }
    if (thumbnailFavoriteOverlay && shouldShowCatalogCardActions(tvProfile, controllerActionMode)) {
        FavoriteIconButton(
            favorite = favorite,
            onClick = onFavorite,
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(overlayActionPadding),
            size = overlayActionSize,
        )
    }
    ControllerFocusFrame(
        visible = enhancedControllerFocus,
        animate = controllerBackgroundAnimations && !reduceMotion,
        cornerRadius = if (expressiveUi) OpenNowRadius.md else OpenNowRadius.sm,
    )
}

/**
 * Caption inside the M3 media-card form: title on the first line, store labels on the second.
 * Rendered below the art (inside the card) instead of outside like the phone poster caption.
 */

// GameCardMediaCaption (was OpenNowScreens.kt:5343)
@Composable
internal fun GameCardMediaCaption(
    game: GameInfo,
    showCardTitles: Boolean,
    showGameStoreLabels: Boolean,
) {
    Column(
        Modifier
            .fillMaxWidth()
            // M3 card content padding (16dp horizontal, 12dp vertical) per the Design Kit.
            .padding(horizontal = OpenNowSpacing.lg, vertical = OpenNowSpacing.md),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (showCardTitles) {
            Text(
                game.title,
                color = TextPrimary,
                style = MaterialTheme.typography.titleSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (showGameStoreLabels) {
            Text(
                displayStoresForGame(game),
                color = TextMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}

// handleCatalogControllerAction (was OpenNowScreens.kt:5377)
internal fun handleCatalogControllerAction(
    event: androidx.compose.ui.input.key.KeyEvent,
    onFavorite: () -> Unit,
    onPlay: () -> Unit,
): Boolean {
    if (event.type != KeyEventType.KeyUp) return false
    return when (event.key) {
        Key.ButtonX -> {
            onFavorite()
            true
        }
        Key.ButtonY -> {
            onPlay()
            true
        }
        else -> false
    }
}

// ControllerCatalogRailActionHints (was OpenNowScreens.kt:5396)
@Composable
internal fun ControllerCatalogRailActionHints(modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.padding(horizontal = 3.dp),
        shape = RoundedCornerShape(8.dp),
        color = Color.Black.copy(alpha = 0.8f),
        tonalElevation = 2.dp,
        shadowElevation = 2.dp,
    ) {
        Column(
            Modifier.padding(horizontal = 4.dp, vertical = 5.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            ControllerCatalogActionHint(
                button = "X",
                label = stringResource(R.string.action_save),
                buttonColor = Color(0xff4aa3ff),
            )
            ControllerCatalogActionHint(
                button = "Y",
                label = stringResource(R.string.action_play),
                buttonColor = Color(0xffffcf40),
            )
        }
    }
}

// ControllerCatalogActionHint (was OpenNowScreens.kt:5423)
@Composable
internal fun ControllerCatalogActionHint(
    button: String,
    label: String,
    buttonColor: Color,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Surface(
            modifier = Modifier.size(18.dp),
            shape = CircleShape,
            color = buttonColor,
        ) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(
                    button,
                    color = Color.Black,
                    fontWeight = FontWeight.Black,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
        Text(
            label,
            color = Color.White,
            style = MaterialTheme.typography.labelSmall,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// launcherBadgeForStoreKey (was OpenNowScreens.kt:5457)
internal fun launcherBadgeForStoreKey(storeKey: String?): LauncherBadge =
    when (storeKey) {
        "STEAM" -> LauncherBadge(R.drawable.ic_store_steam, "Steam", Color(0xff17324d))
        "EPIC", "EGS", "EPIC_GAMES_STORE" -> LauncherBadge(R.drawable.ic_store_epic, "Epic", Color(0xff111111))
        "HOYO", "HOYOVERSE", "HOYOPLAY", "HOYO_PLAY", "MIHOYO" -> LauncherBadge(R.drawable.ic_store_hoyo, "HoYo", Color(0xff2b62d9))
        "XBOX", "XBOX_GAME_PASS", "GAME_PASS" -> LauncherBadge(R.drawable.ic_store_xbox, "Xbox", Color(0xff107c10))
        "MICROSOFT", "MICROSOFT_STORE" -> LauncherBadge(R.drawable.ic_store_microsoft, "Microsoft Store", Color(0xff0067b8))
        "UBISOFT", "UBISOFT_CONNECT" -> LauncherBadge(R.drawable.ic_store_ubisoft, "Ubisoft Connect", Color(0xff006efc))
        "EA", "EA_APP", "ORIGIN" -> LauncherBadge(R.drawable.ic_store_ea, "EA app", Color(0xffff4747))
        "GOG", "GOG.COM", "GOG_COM" -> LauncherBadge(R.drawable.ic_store_gog, "GOG", Color(0xff6a35a8))
        "BATTLENET", "BATTLE.NET", "BATTLE_NET", "BLIZZARD" -> LauncherBadge(R.drawable.ic_store_battlenet, "Battle.net", Color(0xff148eff))
        "RIOT", "RIOT_CLIENT", "RIOT_GAMES" -> LauncherBadge(R.drawable.ic_store_riot, "Riot", Color(0xffd13639))
        "ROCKSTAR", "ROCKSTAR_GAMES", "ROCKSTAR_GAMES_LAUNCHER" -> LauncherBadge(R.drawable.ic_store_rockstar, "Rockstar", Color(0xffffc400), Color(0xff111111))
        "NCSOFT", "NC_SOFT", "PURPLE" -> LauncherBadge(R.drawable.ic_tab_store, "NCSOFT", Color(0xffb4822d), Color(0xff111111))
        "GOOGLE_PLAY", "PLAY_STORE", "ANDROID" -> LauncherBadge(R.drawable.ic_store_google_play, "Google Play", Color(0xff0f9d58))
        "AMAZON", "AMAZON_GAMES" -> LauncherBadge(R.drawable.ic_store_amazon, "Amazon Games", Color(0xffff9900), Color(0xff111111))
        else -> LauncherBadge(R.drawable.ic_tab_store, "GeForce NOW", Color.Black.copy(alpha = 0.72f))
    }

// displayStoresForGame (was OpenNowScreens.kt:5476)
internal fun displayStoresForGame(game: GameInfo): String {
    val stores = displayStoresForVariants(game.variants).ifEmpty {
        game.availableStores.map(::gameStoreDisplayName)
    }.distinctBy { normalizeGameStore(it) }
    return stores.joinToString(", ").ifBlank { "GeForce NOW" }
}

// ZortosPlayMark (was OpenNowScreens.kt:5483)
@Composable
internal fun ZortosPlayMark(
    modifier: Modifier = Modifier,
    ringColor: Color = MaterialTheme.colorScheme.primary,
    playColor: Color = ringColor,
) {
    Canvas(modifier) {
        val play = Path().apply {
            moveTo(size.width * 0.35f, size.height * 0.25f)
            lineTo(size.width * 0.35f, size.height * 0.75f)
            lineTo(size.width * 0.75f, size.height * 0.5f)
            close()
        }
        drawPath(play, playColor)
    }
}

// AnimatedLaunchOverlay (was OpenNowScreens.kt:5500)
@Composable
internal fun AnimatedLaunchOverlay(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val visibleState = remember {
        MutableTransitionState(false).apply {
            targetState = true
        }
    }
    AnimatedVisibility(
        visibleState = visibleState,
        enter = fadeIn() + slideInVertically(initialOffsetY = { it / 4 }) + scaleIn(initialScale = 0.94f),
        exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 4 }) + scaleOut(targetScale = 0.94f),
        modifier = modifier,
    ) {
        content()
    }
}
