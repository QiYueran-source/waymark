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
	import DetailPanel from './DetailPanel.svelte';
	import ReadFailure from './ReadFailure.svelte';
	import TreeView from './TreeView.svelte';
	import UnresolvedList from './UnresolvedList.svelte';
	import { ApiFailure, fetchHealth, fetchItem, fetchTree } from '$lib/api';
	import type { Failure } from '$lib/failure';
	import { allIds } from '$lib/tree';
	import { browserStorage, PreferenceStore } from '$lib/storage';
	import type { CategoryGroup, ItemDetail, TreeData, TreeNode, ViewName } from '$lib/types';

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

	let store: PreferenceStore | null = null;
	let detailRequest = 0;

	const roots = $derived(treeData?.view === 'combined' ? treeData.roots : []);
	const groups: CategoryGroup[] = $derived(treeData?.view === 'grouped' ? treeData.groups : []);
	const unresolved = $derived(treeData?.unresolved ?? []);
	const isEmpty = $derived(
		!loadingTree &&
			readFailure === null &&
			treeData !== null &&
			(treeData.view === 'combined'
				? treeData.roots.length === 0
				: treeData.groups.length === 0) &&
			treeData.unresolved.length === 0
	);
	/** The screen's one finding: how much work there is and what is held up. */
	const summary = $derived(treeData === null ? '' : summarize(treeData));

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

	function treeIds(data: TreeData): string[] {
		return data.view === 'combined'
			? allIds(data.roots)
			: allIds(data.groups.flatMap((group) => group.roots));
	}

	function countNodes(nodes: TreeNode[]): { items: number; blocked: number } {
		let items = 0;
		let blocked = 0;
		const walk = (list: TreeNode[]): void => {
			for (const node of list) {
				items += 1;
				if (node.state === 'blocked') blocked += 1;
				walk(node.children);
			}
		};
		walk(nodes);
		return { items, blocked };
	}

	function summarize(data: TreeData): string {
		const rootsOf = data.view === 'combined' ? data.roots : data.groups.flatMap((g) => g.roots);
		const { items, blocked } = countNodes(rootsOf);
		const parts = [`${items} item${items === 1 ? '' : 's'}`, `${blocked} blocked`];
		if (data.unresolved.length > 0) {
			parts.push(`${data.unresolved.length} unresolved`);
		}
		return parts.join(' · ');
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
			expanded = saved ?? treeIds(data);
			dependenciesVisible = store?.readDependencies(next) ?? false;
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
			store = new PreferenceStore(storage, projectRoot);
			syncNotice();
			await loadTree(store.readView());
		} catch (error) {
			readFailure = toFailure(error);
			loadingTree = false;
		}
	}

	async function selectView(next: ViewName): Promise<void> {
		if (next === view) return;
		if (store && treePane) {
			store.writeScroll(view, treePane.scrollTop);
		}
		store?.writeView(next);
		syncNotice();
		await loadTree(next);
	}

	function toggleNode(id: string): void {
		expanded = expanded.includes(id)
			? expanded.filter((entry) => entry !== id)
			: [...expanded, id];
		store?.writeExpanded(view, expanded);
		syncNotice();
	}

	function expandAll(): void {
		if (!treeData) return;
		expanded = treeIds(treeData);
		store?.writeExpanded(view, expanded);
		syncNotice();
	}

	function collapseAll(): void {
		expanded = [];
		store?.writeExpanded(view, expanded);
		syncNotice();
	}

	function toggleDependencies(): void {
		dependenciesVisible = !dependenciesVisible;
		store?.writeDependencies(view, dependenciesVisible);
		syncNotice();
	}

	async function selectItem(id: string): Promise<void> {
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

	function closeDetail(): void {
		detailRequest += 1;
		selectedId = null;
		detail = null;
		detailFailure = null;
		detailLoading = false;
	}

	function handleScroll(): void {
		if (!store || !treePane) return;
		store.writeScroll(view, treePane.scrollTop);
		syncNotice();
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
			{:else if treeData}
				<TreeView
					{view}
					{roots}
					{groups}
					{expanded}
					{dependenciesVisible}
					selectedId={selectedId}
					onToggle={toggleNode}
					onSelect={(id) => void selectItem(id)}
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
