<script lang="ts">
	/**
	 * The read-only overview: it loads one health read, one tree read per view,
	 * and one item detail per selection. It owns only browser-local presentation
	 * state.
	 *
	 * There is no write path here: the toolbar's controls change local view state
	 * (selected view, expansion, dependency overlays) and the detail panel is
	 * read-only. No form, no non-GET request, no refresh control, no archive
	 * browser.
	 */
	import { onMount, tick } from 'svelte';
	import CategoryPicker from './CategoryPicker.svelte';
	import DetailPanel from './DetailPanel.svelte';
	import GraphText from './GraphText.svelte';
	import GraphView from './GraphView.svelte';
	import PaneState from './PaneState.svelte';
	import ReadFailure from './ReadFailure.svelte';
	import TreeView from './TreeView.svelte';
	import UnresolvedList from './UnresolvedList.svelte';
	import { ApiFailure, fetchHealth, fetchItem, fetchTree } from '$lib/api';
	import type { Failure } from '$lib/failure';
	import { buildGraph, collapseGraph, graphOutline } from '$lib/graph';
	import type { GraphViewport } from '$lib/graph';
	import {
		allIds,
		categoryScopeKeys,
		filterGroups,
		resolveCategoryScope,
		summarize,
		summarizeCounts,
		visibleRoots
	} from '$lib/tree';
	import type { FilteredGroups } from '$lib/tree';
	import { browserStorage, PreferenceStore, projectIdFor } from '$lib/storage';
	import type {
		Category,
		CategoryGroup,
		ItemDetail,
		PresentationName,
		TreeData,
		ViewName
	} from '$lib/types';

	let {
		fetchImpl = globalThis.fetch,
		storage = browserStorage()
	}: {
		/** Injected in tests; the page always uses the same-origin `fetch`. */
		fetchImpl?: typeof fetch;
		/** Injected in tests; `null` simulates unavailable browser storage. */
		storage?: Storage | null;
	} = $props();

	let projectRoot = $state('');
	let view = $state<ViewName>('combined');
	let presentation = $state<PresentationName>('tree');
	/**
	 * Whether the graph is being read as its structured text alternative.
	 *
	 * It belongs to the graph presentation and is stored per project; the
	 * drawing is the default because the alternative is an explicit choice.
	 */
	let graphAlternative = $state(false);
	let treeData = $state<TreeData | null>(null);
	let readFailure = $state<Failure | null>(null);
	let loadingTree = $state(true);
	let expanded = $state<string[]>([]);
	let dependenciesVisible = $state(false);
	let selectedId = $state<string | null>(null);
	let detail = $state<ItemDetail | null>(null);
	let detailFailure = $state<Failure | null>(null);
	let detailLoading = $state(false);
	let persistenceNotice = $state<string | null>(null);
	let treePane: HTMLElement | undefined = $state();
	let restoreToken = $state(0);
	/**
	 * The graph nodes the reader folded shut, by id.
	 *
	 * Kept apart from the tree's `expanded`: folding a branch in the drawing is
	 * a different decision from folding a row in the list, and the two are
	 * stored on different keys so neither can move the other.
	 */
	let collapsedIds = $state<string[]>([]);
	/** The viewport this project last left, applied when the graph next mounts. */
	let restoredViewport = $state<GraphViewport | null>(null);
	/**
	 * The category keys this project has in scope, in configured order, or `null`
	 * before a grouped read has resolved them. `[]` is a real choice — the reader
	 * cleared every category — so it is not the same as `null`.
	 */
	let categoryScope = $state<string[] | null>(null);

	let store: PreferenceStore | null = null;
	let detailRequest = 0;
	/** The tree button that opened the detail, so focus can return to it. */
	let detailTrigger: HTMLElement | null = null;

	const roots = $derived(treeData?.view === 'combined' ? treeData.roots : []);
	/** Every category the project configures, whether or not it holds work. */
	const categories: Category[] = $derived(treeData === null ? [] : treeData.categories);
	const selectedCategories: ReadonlySet<string> = $derived(
		resolveCategoryScope(categories, categoryScope)
	);
	/**
	 * The grouped view's groups after the scope is applied. An ancestor outside
	 * the scope stays when work in scope sits below it, and is reported in
	 * `contextIds` so the tree can mark it as outside the selection.
	 */
	const scopedGroups: FilteredGroups = $derived(
		filterGroups(treeData?.view === 'grouped' ? treeData.groups : [], selectedCategories)
	);
	const groups: CategoryGroup[] = $derived(scopedGroups.groups);
	const contextIds: ReadonlySet<string> = $derived(scopedGroups.contextIds);
	const unresolved = $derived(treeData?.unresolved ?? []);
	/** The payload holds work, before any category filtering. */
	const hasWork = $derived(
		treeData !== null &&
			(treeData.view === 'combined' ? treeData.roots.length > 0 : treeData.groups.length > 0)
	);
	const isEmpty = $derived(
		!loadingTree && readFailure === null && treeData !== null && !hasWork && unresolved.length === 0
	);
	/** The reader explicitly selected nothing; the filter restored in one click. */
	const noCategorySelected = $derived(categories.length > 0 && selectedCategories.size === 0);
	/** The project has work, but none of it is in the categories in scope. */
	const isFilteredEmpty = $derived(
		!loadingTree &&
			readFailure === null &&
			treeData?.view === 'grouped' &&
			hasWork &&
			groups.length === 0
	);
	/** The screen's one finding: how much work is shown and what is held up. */
	const summary = $derived(
		treeData === null
			? ''
			: treeData.view === 'combined'
				? summarize(treeData)
				: summarizeCounts(scopedGroups.counts, treeData.unresolved.length)
	);
	/**
	 * The graph is drawn from the tree payload already loaded, so switching
	 * presentation costs no read. `null` until the first tree arrives. The
	 * grouped payload is the filtered one, so the graph and the grouped tree
	 * show the same work.
	 */
	const graphData = $derived<TreeData | null>(
		treeData === null
			? null
			: treeData.view === 'grouped'
				? { ...treeData, groups }
				: treeData
	);
	/**
	 * The graph model. The dependency overlay is a model decision, so the toggle
	 * travels with the build: while it is off the graph holds no dependency edge
	 * at all, and switching it on costs no read.
	 */
	const workGraph = $derived(
		graphData === null
			? null
			: buildGraph(graphData, {
					dependenciesVisible,
					// The same ids the tree marks, so a kept ancestor reads as context
					// in both presentations. The combined view filters nothing, so its
					// set is empty and every node stays ordinary work.
					contextIds
				})
	);
	/** The folded set, as the model helper takes it. */
	const collapsed: ReadonlySet<string> = $derived(new Set(collapsedIds));
	/**
	 * The one drawing both presentations read.
	 *
	 * Category scope and the dependency overlay already reach the model through
	 * `buildGraph`, so they apply to the drawing and the outline alike. This is
	 * also the seam the graph's collapse state is applied at: whichever filtered
	 * and collapsed model the drawing is given is also the one `graphOutline`
	 * reads, so the two cannot disagree about which items are showing. There is
	 * no second projection to keep in step.
	 *
	 * Collapsing is presentation only: the model is rebuilt from the same tree
	 * payload and then filtered, so no item is read, written, or re-parented.
	 * Ids this project no longer contains match nothing.
	 */
	const visibleGraph = $derived(
		workGraph === null ? null : collapseGraph(workGraph, collapsed)
	);

	/**
	 * The alternative's outline, built from the same model the drawing renders.
	 * It is only built while the alternative is on.
	 */
	const outline = $derived(
		visibleGraph === null || !graphAlternative ? [] : graphOutline(visibleGraph)
	);
	/** The alternative is the graph presentation read as text. */
	const readingAlternative = $derived(presentation === 'graph' && graphAlternative);
	/**
	 * Whether the drawing itself occupies the pane.
	 *
	 * Only then does the pane become the full-height flex column the canvas
	 * fills; the structured alternative and the blocking states keep the
	 * ordinary padded, scrollable pane.
	 */
	const showingGraphDrawing = $derived(
		presentation === 'graph' &&
			!graphAlternative &&
			!loadingTree &&
			readFailure === null &&
			treeData !== null &&
			!isEmpty &&
			!isFilteredEmpty
	);

	const PERSISTENCE_NOTICE =
		'Preferences could not be saved in this browser. The view still works, but these choices may reset when the page reloads.';

	function toFailure(error: unknown): Failure {
		if (error instanceof ApiFailure) {
			return { code: error.code, message: error.message, diagnostics: error.diagnostics };
		}
		return {
			code: 'UNEXPECTED_ERROR',
			message: error instanceof Error ? error.message : String(error),
			diagnostics: []
		};
	}

	function syncNotice(): void {
		persistenceNotice = store?.failed ? PERSISTENCE_NOTICE : null;
	}

	/**
	 * Write one preference and report the store's state once.
	 *
	 * Every preference write goes through here, so the persistence notice is
	 * raised in one place rather than after each of the nine writes.
	 */
	function persist(write: (store: PreferenceStore) => void): void {
		if (store === null) return;
		write(store);
		syncNotice();
	}

	async function loadTree(next: ViewName): Promise<void> {
		view = next;
		loadingTree = true;
		readFailure = null;
		treeData = null;
		try {
			const data = await fetchTree(next, fetchImpl);
			treeData = data;
			const saved = store?.readExpanded(next) ?? null;
			expanded = saved ?? allIds(visibleRoots(data));
			dependenciesVisible = store?.readDependencies(next) ?? false;
			// The stored scope is read as it was written; stale keys are dropped
			// where it meets this project's configured categories, which the
			// payload carries. `null` stays `null` so the default is "everything".
			categoryScope = store?.readCategories() ?? null;
			// One fold set and one viewport per project, not per data view: the
			// graph draws whichever view is selected, so switching the view must
			// not reopen a branch or move the drawing.
			collapsedIds = store?.readCollapsed() ?? [];
			restoredViewport = store?.readViewport() ?? null;
			restoreToken += 1;
			syncNotice();
		} catch (error) {
			readFailure = toFailure(error);
		} finally {
			loadingTree = false;
		}
	}

	async function start(): Promise<void> {
		try {
			const health = await fetchHealth(fetchImpl);
			projectRoot = health.project?.root ?? '';
			// The preference namespace is derived once, here, from the health payload.
			store = new PreferenceStore(storage, projectIdFor(projectRoot));
			syncNotice();
			// The presentation is a property of the project, not of the data view:
			// restore it once, before the first tree read.
			presentation = store.readPresentation();
			graphAlternative = store.readGraphAlternative();
			await loadTree(store.readView());
		} catch (error) {
			readFailure = toFailure(error);
			loadingTree = false;
		}
	}

	async function selectView(next: ViewName): Promise<void> {
		if (next === view) return;
		const offset = treePane?.scrollTop ?? 0;
		persist((current) => current.writeScroll(view, offset));
		persist((current) => current.writeView(next));
		await loadTree(next);
	}

	/**
	 * Switch between the tree and the graph.
	 *
	 * The choice is one per project, so it is stored on its own key and survives
	 * a change of data view. Nothing is re-read: both presentations render the
	 * tree payload already in memory.
	 */
	function selectPresentation(next: PresentationName): void {
		if (next === presentation) return;
		presentation = next;
		persist((current) => current.writePresentation(next));
	}

	/**
	 * Switch the graph between the drawing and its structured text alternative.
	 *
	 * Nothing is re-read: both render the graph model already in memory, so the
	 * selected category scope, the dependency overlay, and the selection stay
	 * exactly as they were across the switch.
	 */
	function toggleGraphAlternative(): void {
		graphAlternative = !graphAlternative;
		persist((current) => current.writeGraphAlternative(graphAlternative));
	}

	function toggleNode(id: string): void {
		expanded = expanded.includes(id)
			? expanded.filter((entry) => entry !== id)
			: [...expanded, id];
		persist((current) => current.writeExpanded(view, expanded));
	}

	function expandAll(): void {
		if (!treeData) return;
		expanded = allIds(visibleRoots(treeData));
		persist((current) => current.writeExpanded(view, expanded));
	}

	function collapseAll(): void {
		expanded = [];
		persist((current) => current.writeExpanded(view, expanded));
	}

	/**
	 * Fold or unfold one node's descendants.
	 *
	 * Only the drawing changes. The stored set is written whole, so it survives
	 * a reload and stays this project's own; folding never touches a work item.
	 */
	function toggleCollapse(id: string): void {
		collapsedIds = collapsedIds.includes(id)
			? collapsedIds.filter((entry) => entry !== id)
			: [...collapsedIds, id];
		persist((current) => current.writeCollapsed(collapsedIds));
	}

	/** Remember where the reader left the drawing, for this project only. */
	function rememberViewport(viewport: GraphViewport): void {
		restoredViewport = viewport;
		persist((current) => current.writeViewport(viewport));
	}

	/**
	 * Forget the remembered viewport.
	 *
	 * Resetting returns the drawing to its fitted state, so a reload must start
	 * fitted too; keeping the old transform would resurrect what was discarded.
	 */
	function forgetViewport(): void {
		restoredViewport = null;
		persist((current) => current.clearViewport());
	}

	function toggleDependencies(): void {
		dependenciesVisible = !dependenciesVisible;
		persist((current) => current.writeDependencies(view, dependenciesVisible));
	}

	/**
	 * Remember a category scope.
	 *
	 * What is stored is a set of category keys, in configured order, so a stored
	 * scope compares and reloads predictably. An empty scope is written rather
	 * than cleared: "I chose none" is a decision the reader made, not a missing
	 * preference, and it must survive a reload.
	 */
	function setCategoryScope(next: ReadonlySet<string>): void {
		const keys = categoryScopeKeys(categories, next);
		categoryScope = keys;
		persist((current) => current.writeCategories(keys));
	}

	function toggleCategory(key: string): void {
		const next = new Set(selectedCategories);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		setCategoryScope(next);
	}

	function selectAllCategories(): void {
		setCategoryScope(new Set(categories.map((category) => category.key)));
	}

	function clearAllCategories(): void {
		setCategoryScope(new Set());
	}

	async function selectItem(id: string, trigger?: HTMLElement): Promise<void> {
		if (trigger) detailTrigger = trigger;
		const request = ++detailRequest;
		selectedId = id;
		detail = null;
		detailFailure = null;
		detailLoading = true;
		try {
			const loaded = await fetchItem(id, fetchImpl);
			if (request !== detailRequest) return;
			detail = loaded;
		} catch (error) {
			if (request !== detailRequest) return;
			detailFailure = toFailure(error);
		} finally {
			if (request === detailRequest) detailLoading = false;
		}
	}

	/** Close the panel and return focus to the row that opened it. */
	function closeDetail(): void {
		detailRequest += 1;
		selectedId = null;
		detail = null;
		detailFailure = null;
		detailLoading = false;
		const trigger = detailTrigger;
		detailTrigger = null;
		void tick().then(() => {
			if (trigger?.isConnected) trigger.focus();
			else treePane?.focus();
		});
	}

	function handleScroll(): void {
		if (!treePane) return;
		const offset = treePane.scrollTop;
		persist((current) => current.writeScroll(view, offset));
	}

	$effect(() => {
		const token = restoreToken;
		const pane = treePane;
		if (token === 0 || pane === undefined) return;
		const offset = store?.readScroll(view) ?? 0;
		void tick().then(() => {
			pane.scrollTop = offset;
		});
	});

	onMount(() => {
		void start();
	});
</script>

<!--
	The one category control. In the drawing it is rendered inside the graph's
	upper-left cluster; in the tree and the structured list it stays in the top
	toolbar. Combined mode filters nothing, so the snippet is empty there.
-->
{#snippet categoryControl()}
	{#if view === 'grouped' && categories.length > 0}
		<CategoryPicker
			{categories}
			selected={selectedCategories}
			onToggle={toggleCategory}
			onSelectAll={selectAllCategories}
			onClearAll={clearAllCategories}
		/>
	{/if}
{/snippet}

<div class="explorer">
	<header class="app-header">
		<div class="brand">
			<svg class="brand-mark" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
				<rect x="4.5" y="16.6" width="15" height="3.3" rx="1.65" />
				<rect x="7" y="11.8" width="10" height="3.3" rx="1.65" />
				<rect x="9.5" y="7" width="5" height="3.3" rx="1.65" />
			</svg>
			<div class="brand-text">
				<h1>Waymark</h1>
				<p class="project" data-testid="project-root" title={projectRoot}>
					{projectRoot || 'Read-only overview'}
				</p>
			</div>
		</div>
		<span class="read-only">Read-only</span>
	</header>

	{#if persistenceNotice}
		<!-- Non-blocking: this stays inline above the toolbar, never centred. -->
		<p
			class="notice"
			role="status"
			data-state-layout="inline"
			data-testid="persistence-notice"
		>
			{persistenceNotice}
		</p>
	{/if}

	<div class="toolbar">
		<fieldset class="views">
			<legend class="sr-only">Tree view</legend>
			<label class="segment">
				<input
					type="radio"
					name="view"
					value="combined"
					checked={view === 'combined'}
					onchange={() => void selectView('combined')}
				/>
				<span>Combined</span>
			</label>
			<label class="segment">
				<input
					type="radio"
					name="view"
					value="grouped"
					checked={view === 'grouped'}
					onchange={() => void selectView('grouped')}
				/>
				<span>By category</span>
			</label>
		</fieldset>

		<fieldset class="views">
			<legend class="sr-only">Presentation</legend>
			<label class="segment">
				<input
					type="radio"
					name="presentation"
					value="tree"
					checked={presentation === 'tree'}
					onchange={() => selectPresentation('tree')}
				/>
				<span>Tree</span>
			</label>
			<label class="segment">
				<input
					type="radio"
					name="presentation"
					value="graph"
					checked={presentation === 'graph'}
					onchange={() => selectPresentation('graph')}
				/>
				<span>Graph</span>
			</label>
		</fieldset>

		{#if presentation === 'graph'}
			<label class="deps-toggle alternative-toggle">
				<input
					type="checkbox"
					checked={graphAlternative}
					onchange={toggleGraphAlternative}
					data-testid="graph-alternative-toggle"
				/>
				Structured list
			</label>
		{/if}

		{#if !showingGraphDrawing}
			<!-- Tree and structured-list placement: the same compact control stays
			     in the top toolbar, with no canvas overlay. -->
			{@render categoryControl()}
		{/if}

		{#if summary}
			<p class="summary" data-testid="tree-summary">{summary}</p>
		{/if}

		<div class="tree-actions">
			<button type="button" onclick={expandAll} disabled={!treeData}>Expand all</button>
			<button type="button" onclick={collapseAll} disabled={!treeData}>Collapse all</button>
			<label class="deps-toggle">
				<input type="checkbox" checked={dependenciesVisible} onchange={toggleDependencies} />
				Show dependency edges
			</label>
		</div>
	</div>

	<main class="layout">
		<section
			class="tree-pane"
			class:graph-pane-host={showingGraphDrawing}
			bind:this={treePane}
			onscroll={handleScroll}
			aria-label="Work items"
			aria-busy={loadingTree}
			tabindex="-1"
		>
			{#if loadingTree}
				<!-- Loading is a status, not one of the five blocking/empty states. -->
				<p class="status" role="status">Reading the project tree…</p>
			{:else if readFailure}
				<PaneState testId="read-failure-state" kind="read-failure">
					<ReadFailure failure={readFailure} />
				</PaneState>
			{:else if isEmpty}
				<PaneState testId="empty-state" kind="empty-project">
					<div class="empty">
						<h2>No active work items</h2>
						<p>
							This project has no non-archived work items. Archived items stay out of this
							view.
						</p>
					</div>
				</PaneState>
			{:else if isFilteredEmpty}
				<!-- Two ways to reach an empty filtered pane — every category cleared,
				     or selected categories that hold no active work — and one recovery
				     for both, repeated where the reader is looking. -->
				<PaneState
					testId="filtered-empty-state"
					kind={noCategorySelected ? 'categories-cleared' : 'no-active-work'}
					emptyKind={noCategorySelected ? 'categories-cleared' : 'no-active-work'}
				>
					<div class="empty">
						{#if noCategorySelected}
							<h2>No categories selected</h2>
							<p>
								The by-category {presentation === 'graph' ? 'graph' : 'view'} shows only the
								categories you select, and none is selected right now.
							</p>
						{:else}
							<h2>Nothing in the selected categories</h2>
							<p>
								The by-category {presentation === 'graph' ? 'graph' : 'view'} shows only the
								categories you select, and none of the selected ones has active work right
								now.
							</p>
						{/if}
						<button
							class="empty-action"
							type="button"
							data-testid="select-all-from-empty"
							onclick={selectAllCategories}
						>
							Select all categories
						</button>
					</div>
				</PaneState>
			{:else if treeData}
				{#if readingAlternative && visibleGraph}
					<GraphText
						{outline}
						{selectedId}
						onSelect={(id, trigger) => void selectItem(id, trigger)}
					/>
				{:else if presentation === 'graph' && visibleGraph}
					<GraphView
						graph={visibleGraph}
						{selectedId}
						{restoredViewport}
						categoryControl={categoryControl}
						onSelect={(id, trigger) => void selectItem(id, trigger)}
						onToggle={toggleCollapse}
						onViewportChange={rememberViewport}
						onViewportReset={forgetViewport}
					/>
				{:else}
					<TreeView
						{view}
						{roots}
						{groups}
						{contextIds}
						{expanded}
						{dependenciesVisible}
						{selectedId}
						onToggle={toggleNode}
						onSelect={(id, trigger) => void selectItem(id, trigger)}
					/>
				{/if}
				{#if presentation === 'tree' && unresolved.length > 0}
					<!-- The tree shows its full unresolved panel. The drawing hosts
					     the same list in its own collapsible bottom drawer, and the
					     structured alternative states the reasons inline. -->
					<UnresolvedList items={unresolved} />
				{/if}
			{/if}
		</section>

		{#if selectedId}
			<DetailPanel
				id={selectedId}
				{detail}
				failure={detailFailure}
				loading={detailLoading}
				onClose={closeDetail}
				onSelect={(id) => void selectItem(id)}
			/>
		{/if}
	</main>
</div>
