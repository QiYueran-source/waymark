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
	import CategoryFilter from './CategoryFilter.svelte';
	import DetailPanel from './DetailPanel.svelte';
	import ReadFailure from './ReadFailure.svelte';
	import TreeView from './TreeView.svelte';
	import UnresolvedList from './UnresolvedList.svelte';
	import { ApiFailure, fetchHealth, fetchItem, fetchTree } from '$lib/api';
	import type { Failure } from '$lib/failure';
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
	import type { Category, CategoryGroup, ItemDetail, TreeData, ViewName } from '$lib/types';

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
	/** The grouped view's groups after the scope is applied; the seam for context rows. */
	const scopedGroups: FilteredGroups = $derived(
		filterGroups(treeData?.view === 'grouped' ? treeData.groups : [], selectedCategories)
	);
	const groups: CategoryGroup[] = $derived(scopedGroups.groups);
	const unresolved = $derived(treeData?.unresolved ?? []);
	/** The payload holds work, before any category filtering. */
	const hasWork = $derived(
		treeData !== null &&
			(treeData.view === 'combined' ? treeData.roots.length > 0 : treeData.groups.length > 0)
	);
	const isEmpty = $derived(
		!loadingTree && readFailure === null && treeData !== null && !hasWork && unresolved.length === 0
	);
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
		<p class="notice" role="status" data-testid="persistence-notice">{persistenceNotice}</p>
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

		{#if view === 'grouped' && categories.length > 0}
			<CategoryFilter
				{categories}
				selected={selectedCategories}
				onToggle={toggleCategory}
				onSelectAll={selectAllCategories}
				onClearAll={clearAllCategories}
			/>
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
			bind:this={treePane}
			onscroll={handleScroll}
			aria-label="Work items"
			aria-busy={loadingTree}
			tabindex="-1"
		>
			{#if loadingTree}
				<p class="status" role="status">Reading the project tree…</p>
			{:else if readFailure}
				<ReadFailure failure={readFailure} />
			{:else if isEmpty}
				<div class="empty" data-testid="empty-state">
					<h2>No active work items</h2>
					<p>
						This project has no non-archived work items. Archived items stay out of this
						view.
					</p>
				</div>
			{:else if isFilteredEmpty}
				<div class="empty" data-testid="filtered-empty-state">
					<h2>Nothing in the selected categories</h2>
					<p>
						The by-category view shows only the categories you select, and none of the
						selected ones has active work right now. Use Select all above to see every
						category again.
					</p>
				</div>
			{:else if treeData}
				<TreeView
					{view}
					{roots}
					{groups}
					{expanded}
					{dependenciesVisible}
					selectedId={selectedId}
					onToggle={toggleNode}
					onSelect={(id, trigger) => void selectItem(id, trigger)}
				/>
				{#if unresolved.length > 0}
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
