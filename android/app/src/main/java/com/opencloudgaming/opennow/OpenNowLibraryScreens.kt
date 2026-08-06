package com.opencloudgaming.opennow


import android.content.res.Configuration
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.key
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.screens.tv.TvImmersiveList
import com.opencloudgaming.opennow.ui.adaptive.isAtLeastMedium
import com.opencloudgaming.opennow.ui.adaptive.windowWidthSizeClassOf
import kotlinx.coroutines.delay
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing




// LibraryScreen (was OpenNowScreens.kt:3286)
@Composable
internal fun LibraryScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    hideChromeWhenScrolled: Boolean,
    controlsInTopBar: Boolean,
    searchRequested: Boolean,
    onSearchDismissed: () -> Unit,
    onScrollChromeHiddenChange: (Boolean) -> Unit,
) {
    val orderedGames = remember(state.libraryGames, state.settings.favoriteGameIds) {
        favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds)
    }
    val filterOptions = remember(orderedGames) {
        libraryStoreFilterOptions(orderedGames)
    }
    val games = remember(orderedGames, state.librarySearch, state.libraryFilterIds) {
        orderedGames.filter { game ->
            gameMatchesSearch(game, state.librarySearch) && gameMatchesLibraryFilters(game, state.libraryFilterIds)
        }
    }
    val gridState = rememberLazyGridState()
    val searchFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val showSearch = searchRequested || state.librarySearch.isNotBlank()
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
                        query = state.librarySearch,
                        onQueryChange = { next ->
                            viewModel.setLibrarySearch(next)
                            if (next.isBlank()) onSearchDismissed()
                        },
                        placeholder = "Search library",
                        focusRequester = searchFocusRequester,
                    )
                }
                LibraryFilterControls(
                    gameCount = games.size,
                    totalCount = state.libraryGames.size,
                    options = filterOptions,
                    selectedIds = state.libraryFilterIds,
                    onToggle = viewModel::toggleLibraryFilter,
                    showToolbar = !controlsInTopBar,
                )
                if (state.loadingGames && state.libraryGames.isEmpty()) {
                    RefreshingGamesPlaceholder(
                        settings = state.settings,
                        tvProfile = tvProfile,
                        modifier = Modifier.weight(1f),
                    )
                } else if (tvProfile && games.isNotEmpty()) {
                    // TV Design Kit immersive list — full-width rows with focus-revealed actions.
                    TvImmersiveList(
                        games = games,
                        favoriteIds = state.settings.favoriteGameIds,
                        onSelect = viewModel::selectGame,
                        onFavorite = viewModel::updateFavorites,
                        onPlay = viewModel::play,
                        modifier = Modifier.weight(1f),
                    )
                } else {
                    GameGrid(
                        games,
                        state.settings.favoriteGameIds,
                        state.settings,
                        tvProfile,
                        viewModel::selectGame,
                        viewModel::updateFavorites,
                        viewModel::play,
                        viewModel::chooseStore,
                        modifier = Modifier.weight(1f),
                        gridState = gridState,
                        emptyContent = {
                            val hasSearch = state.librarySearch.isNotBlank()
                            val hasFilters = state.libraryFilterIds.isNotEmpty()
                            if ((hasSearch || hasFilters) && state.libraryGames.isNotEmpty()) {
                                SearchEmptyState(
                                    title = stringResource(R.string.library_empty_search_title),
                                    message = when {
                                        hasSearch && hasFilters -> stringResource(R.string.library_empty_search_filters_body)
                                        hasSearch -> stringResource(R.string.library_empty_search_body)
                                        else -> stringResource(R.string.library_empty_filters_body)
                                    },
                                    onClearSearch = if (hasSearch) {
                                        {
                                            viewModel.setLibrarySearch("")
                                            onSearchDismissed()
                                        }
                                    } else {
                                        null
                                    },
                                    onClearFilters = if (hasFilters) {
                                        viewModel::clearLibraryFilters
                                    } else {
                                        null
                                    },
                                )
                            } else {
                                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
                            }
                        },
                    )
                }
            }
        }
    }
}

// LibraryFilterControls (was OpenNowScreens.kt:3431)
@Composable
internal fun LibraryFilterControls(
    gameCount: Int,
    totalCount: Int,
    options: List<CatalogFilterOption>,
    selectedIds: List<String>,
    onToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
    showToolbar: Boolean = true,
    showSelectedChips: Boolean = true,
) {
    if (!showToolbar && (!showSelectedChips || selectedIds.isEmpty())) return
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (showToolbar) {
            Row(
                if (compact) Modifier else Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val countModifier = if (compact) Modifier else Modifier.weight(1f)
                Text(
                    text = if (gameCount == totalCount) {
                        stringResource(R.string.library_count, totalCount)
                    } else {
                        "$gameCount / ${stringResource(R.string.library_count, totalCount)}"
                    },
                    color = TextMuted,
                    style = if (compact) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Start,
                    modifier = countModifier,
                )
                if (options.isNotEmpty()) {
                    FilterMenu(options = options, selectedIds = selectedIds, onToggle = onToggle, compact = compact)
                }
            }
        }
        if (showSelectedChips) {
            SelectedFilterChips(options = options, selectedIds = selectedIds, onToggle = onToggle)
        }
    }
}

// libraryStoreFilterOptions (was OpenNowScreens.kt:3476)
internal fun libraryStoreFilterOptions(games: List<GameInfo>): List<CatalogFilterOption> {
    val labelsById = linkedMapOf<String, String>()
    games.forEach { game ->
        libraryStoreFilterIds(game).forEach { (id, label) ->
            labelsById.putIfAbsent(id, label)
        }
    }
    return labelsById.entries
        .sortedBy { it.value.lowercase(Locale.US) }
        .map { (id, label) ->
            CatalogFilterOption(
                id = id,
                rawId = id.removePrefix(LIBRARY_STORE_FILTER_PREFIX),
                label = label,
                groupId = "library_store",
                groupLabel = "Launcher",
            )
        }
}

// gameMatchesLibraryFilters (was OpenNowScreens.kt:3496)
internal fun gameMatchesLibraryFilters(game: GameInfo, selectedIds: List<String>): Boolean {
    if (selectedIds.isEmpty()) return true
    val gameFilterIds = libraryStoreFilterIds(game).map { it.first }.toSet()
    return selectedIds.any { it in gameFilterIds }
}

// libraryStoreFilterIds (was OpenNowScreens.kt:3502)
internal fun libraryStoreFilterIds(game: GameInfo): List<Pair<String, String>> {
    val labels = libraryStoreDisplayNames(game)
    return labels
        .mapNotNull { label ->
            val normalized = normalizeGameStore(label)
            if (normalized.isBlank()) return@mapNotNull null
            LIBRARY_STORE_FILTER_PREFIX + normalized to label
        }
        .distinctBy { it.first }
}

// LIBRARY_STORE_FILTER_PREFIX (was OpenNowScreens.kt:3513)
internal const val LIBRARY_STORE_FILTER_PREFIX = "library_store:"

// activeSessionGame (was OpenNowScreens.kt:3516)
internal fun activeSessionGame(state: OpenNowUiState, active: ActiveSessionInfo): GameInfo? =
    (state.games + state.libraryGames).firstOrNull { game ->
        game.launchAppId == active.appId.toString() ||
            game.variants.any { variant -> variant.id == active.appId.toString() }
    }

// activeSessionSummary (was OpenNowScreens.kt:3522)
internal fun activeSessionSummary(active: ActiveSessionInfo): String =
    listOfNotNull(
        when (active.status) {
            1 -> active.queuePosition?.takeIf { it > 0 }?.let { "Queue $it" } ?: "Starting"
            2, 3 -> "Ready"
            else -> "Active"
        },
        active.resolution,
        active.fps?.let { "${it} FPS" },
        active.gpuType,
        active.sessionId.take(8).takeIf { it.isNotBlank() }?.let { "Session $it" },
    ).joinToString(" - ")

// SearchEmptyState (was OpenNowScreens.kt:3535)
@Composable
internal fun SearchEmptyState(
    title: String,
    message: String,
    onClearSearch: (() -> Unit)? = null,
    onClearFilters: (() -> Unit)? = null,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            title,
            color = TextPrimary,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        Text(
            message,
            color = TextMuted,
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.Center,
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            onClearSearch?.let { clearSearch ->
                OutlinedButton(onClick = clearSearch) {
                    Text(stringResource(R.string.search_clear), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            onClearFilters?.let { clearFilters ->
                OutlinedButton(onClick = clearFilters) {
                    Text(stringResource(R.string.action_clear_filters), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

// RefreshingGamesPlaceholder (was OpenNowScreens.kt:3580)
@Composable
internal fun RefreshingGamesPlaceholder(
    settings: AppSettings,
    tvProfile: Boolean,
    storeLayout: Boolean = false,
    modifier: Modifier = Modifier,
) {
    GameGridSkeleton(
        settings = settings,
        tvProfile = tvProfile,
        storeLayout = storeLayout,
        modifier = modifier,
    )
}

// GameGrid (was OpenNowScreens.kt:3927)
@Composable
internal fun GameGrid(
    games: List<GameInfo>,
    favoriteIds: List<String>,
    settings: AppSettings,
    tvProfile: Boolean,
    onSelect: (GameInfo) -> Unit,
    onFavorite: (String) -> Unit,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    modifier: Modifier = Modifier,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState = rememberLazyGridState(),
    emptyContent: (@Composable () -> Unit)? = null,
) {
    if (games.isEmpty()) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (emptyContent != null) {
                emptyContent()
            } else {
                Text(stringResource(R.string.no_games_loaded), color = TextMuted)
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

// gameMatchesSearch (was OpenNowScreens.kt:6753)
internal fun gameMatchesSearch(game: GameInfo, query: String): Boolean {
    val normalized = query.trim().lowercase()
    if (normalized.isBlank()) return true
    val haystack = buildString {
        append(game.title).append(' ')
        append(game.description.orEmpty()).append(' ')
        append(game.longDescription.orEmpty()).append(' ')
        append(game.publisherName.orEmpty()).append(' ')
        append(game.genres.joinToString(" ")).append(' ')
        append(game.featureLabels.joinToString(" ")).append(' ')
        append(displayStoresForGame(game))
    }.lowercase()
    return normalized.split(Regex("\\s+")).all { it in haystack }
}

// favoriteOrderedGames (was OpenNowScreens.kt:6768)
internal fun favoriteOrderedGames(games: List<GameInfo>, favoriteIds: List<String>): List<GameInfo> {
    val favorites = games.filter { it.id in favoriteIds }
    return if (favorites.isNotEmpty()) favorites + games.filterNot { it.id in favoriteIds } else games
}

// SortPicker (was OpenNowScreens.kt:14025)
@Composable
internal fun SortPicker(
    options: List<CatalogSortOption>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val labels = options.ifEmpty { listOf(CatalogSortOption("relevance", "Relevance", "")) }
    val selectedLabel = labels.firstOrNull { it.id == selected }?.label ?: labels.first().label
    var expanded by remember { mutableStateOf(false) }
    val controlShape = RoundedCornerShape(OpenNowRadius.full)
    val controlColor = Color.White.copy(alpha = 0.1f)
    Box(modifier) {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier.fillMaxWidth().height(if (compact) TopBarCompactControlHeight else 40.dp),
            shape = controlShape,
            border = BorderStroke(1.dp, Color.White.copy(alpha = 0.2f)),
            colors = ButtonDefaults.outlinedButtonColors(
                containerColor = controlColor,
                contentColor = TextPrimary,
            ),
            contentPadding = PaddingValues(horizontal = if (compact) 8.dp else 12.dp),
        ) {
            Text(
                "Sort: $selectedLabel",
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = if (compact) MaterialTheme.typography.labelMedium else MaterialTheme.typography.labelLarge,
            )
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            labels.forEach { option ->
                DropdownMenuItem(
                    text = {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(if (option.id == selected) "✓" else "", modifier = Modifier.width(24.dp))
                            Text(option.label)
                        }
                    },
                    onClick = {
                        expanded = false
                        onSelect(option.id)
                    },
                )
            }
        }
    }
}

// SelectedFilterChips (was OpenNowScreens.kt:14076)
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun SelectedFilterChips(options: List<CatalogFilterOption>, selectedIds: List<String>, onToggle: (String) -> Unit) {
    val selectedOptions = options.filter { it.id in selectedIds }
    if (selectedOptions.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        selectedOptions.take(4).forEach { option ->
            AssistChip(onClick = { onToggle(option.id) }, label = { Text(option.label, maxLines = 1, overflow = TextOverflow.Ellipsis) })
        }
        if (selectedOptions.size > 4) {
            AssistChip(onClick = {}, label = { Text("+${selectedOptions.size - 4}") })
        }
    }
}

// catalogVisibleFilterGroups (was OpenNowScreens.kt:14091)
internal fun catalogVisibleFilterGroups(groups: List<CatalogFilterGroup>): List<CatalogFilterGroup> =
    groups.filter { it.id in setOf("digital_store", "genre", "subscriptions") }

// catalogFilterOptions (was OpenNowScreens.kt:14094)
internal fun catalogFilterOptions(groups: List<CatalogFilterGroup>): List<CatalogFilterOption> =
    groups.flatMap { group -> group.options.take(if (group.id == "genre") 10 else group.options.size) }

// FilterMenu (was OpenNowScreens.kt:14097)
@Composable
internal fun FilterMenu(
    options: List<CatalogFilterOption>,
    selectedIds: List<String>,
    onToggle: (String) -> Unit,
    compact: Boolean = false,
) {
    var expanded by remember { mutableStateOf(false) }
    val filterControlShape = RoundedCornerShape(OpenNowRadius.full)
    val filterControlColor = Color.White.copy(alpha = 0.1f)
    Box {
        OutlinedButton(
            onClick = { expanded = true },
            modifier = Modifier.height(if (compact) TopBarCompactControlHeight else 36.dp),
            shape = filterControlShape,
            border = BorderStroke(1.dp, Color.White.copy(alpha = 0.2f)),
            colors = ButtonDefaults.outlinedButtonColors(
                containerColor = filterControlColor,
                contentColor = TextPrimary,
            ),
            contentPadding = PaddingValues(horizontal = 10.dp),
        ) {
            Text(if (selectedIds.isEmpty()) "Filters" else "Filters ${selectedIds.size}", maxLines = 1, style = MaterialTheme.typography.labelMedium)
        }
        if (expanded) {
            AlertDialog(
                onDismissRequest = { expanded = false },
                title = {
                    Text(
                        "Filters",
                        fontWeight = FontWeight.Bold,
                        style = MaterialTheme.typography.titleMedium,
                        color = TextPrimary,
                    )
                },
                text = {
                    LazyColumn(
                        modifier = Modifier.fillMaxHeight(0.6f),
                        verticalArrangement = Arrangement.spacedBy(4.dp)
                    ) {
                        items(options) { option ->
                            val isSelected = option.id in selectedIds
                            var rowFocused by remember { mutableStateOf(false) }
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(OpenNowRadius.sm))
                                    .onFocusChanged { rowFocused = it.isFocused }
                                    .background(if (rowFocused) Color.White.copy(alpha = 0.08f) else Color.Transparent)
                                    .border(
                                        width = 1.dp,
                                        color = if (rowFocused) MaterialTheme.colorScheme.primary else Color.Transparent,
                                        shape = RoundedCornerShape(OpenNowRadius.sm)
                                    )
                                    .clickable { onToggle(option.id) }
                                    .padding(horizontal = 8.dp, vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Checkbox(
                                    checked = isSelected,
                                    onCheckedChange = null
                                )
                                Spacer(Modifier.width(12.dp))
                                Text(
                                    option.label,
                                    style = MaterialTheme.typography.bodyLarge,
                                    color = TextPrimary
                                )
                            }
                        }
                    }
                },
                confirmButton = {
                    Button(onClick = { expanded = false }) {
                        Text("Done")
                    }
                }
            )
        }
    }
}
