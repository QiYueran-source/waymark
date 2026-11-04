import { describe, expect, it } from 'vitest';

import {
	allIds,
	categoryScopeKeys,
	countNodes,
	filterGroups,
	resolveCategoryScope,
	sectionsFor,
	summarize,
	summarizeCounts,
	visibleRoots
} from '$lib/tree';
import {
	BUG,
	DOCS,
	TASK,
	bugRoot,
	combinedTree,
	deepGroupedTree,
	emptyTree,
	groupedTree,
	taskRoot,
	unresolvedTree
} from './fixtures';

describe('view sections', () => {
	it('models the combined view as its own section, not a category', () => {
		const sections = sectionsFor(combinedTree);
		expect(sections).toHaveLength(1);
		expect(sections[0].kind).toBe('combined');
		// The combined section has no category and no key to collide with a real one.
		expect('category' in sections[0]).toBe(false);
		expect('key' in sections[0]).toBe(false);
		expect(sections[0].roots).toBe(combinedTree.roots);
	});

	it('keeps the real categories of the grouped view', () => {
		const sections = sectionsFor(groupedTree);
		expect(sections.map((section) => section.kind)).toEqual(['category', 'category']);
		expect(
			sections.map((section) => (section.kind === 'category' ? section.category.key : null))
		).toEqual(['bug', 'task']);
		expect(
			sections.every((section) => section.kind === 'category' && section.category.key !== '*')
		).toBe(true);
	});

	it('lists every displayed root, in section order', () => {
		expect(visibleRoots(combinedTree)).toEqual([bugRoot, taskRoot]);
		expect(visibleRoots(groupedTree)).toEqual([bugRoot, taskRoot]);
		expect(allIds(visibleRoots(groupedTree))).toEqual([
			'wm-bug-login',
			'wm-task-child',
			'wm-task-ship'
		]);
	});
});

describe('tree counts and summary', () => {
	it('counts nested items and blocked items', () => {
		expect(countNodes([bugRoot, taskRoot])).toEqual({ items: 3, blocked: 1 });
		expect(countNodes([])).toEqual({ items: 0, blocked: 0 });
	});

	it('summarizes displayed work, held-up work, and unresolved items', () => {
		expect(summarize(combinedTree)).toBe('3 items · 1 blocked');
		expect(summarize(emptyTree)).toBe('0 items · 0 blocked');
		expect(summarize(unresolvedTree)).toBe('2 items · 1 blocked · 1 unresolved');
	});

	it('formats counts on their own, so a filtered view can reuse the wording', () => {
		expect(summarizeCounts({ items: 1, blocked: 0 }, 0)).toBe('1 item · 0 blocked');
		expect(summarizeCounts({ items: 0, blocked: 0 }, 2)).toBe('0 items · 0 blocked · 2 unresolved');
	});

	it('uses the singular for one item', () => {
		const oneItem = {
			view: 'combined' as const,
			roots: [{ ...taskRoot, children: [] }],
			unresolved: [],
			categories: [BUG, TASK]
		};
		expect(summarize(oneItem)).toBe('1 item · 0 blocked');
	});
});

describe('category scope', () => {
	// The payload's `categories` is every configured category, in configured
	// order, whether or not it holds active work; `groups` is only those with
	// roots. The scope is resolved from the former so an empty category still
	// has a control.
	const configured = [BUG, DOCS, TASK];

	it('selects every configured category when nothing is stored', () => {
		expect(resolveCategoryScope(configured, null)).toEqual(new Set(['bug', 'docs', 'task']));
	});

	it('keeps a stored subset, including a category with no active work', () => {
		expect(resolveCategoryScope(configured, ['docs'])).toEqual(new Set(['docs']));
		expect(resolveCategoryScope(configured, ['bug', 'task'])).toEqual(new Set(['bug', 'task']));
	});

	it('ignores stored keys for categories this project no longer configures', () => {
		expect(resolveCategoryScope(configured, ['bug', 'obsolete'])).toEqual(new Set(['bug']));
		expect(resolveCategoryScope(configured, ['bug', 'bug'])).toEqual(new Set(['bug']));
	});

	it('treats a stored empty selection as none selected, not as "nothing stored"', () => {
		expect(resolveCategoryScope(configured, [])).toEqual(new Set());
	});

	it('stores the scope in configured order, dropping stale keys', () => {
		expect(categoryScopeKeys(configured, new Set(['task', 'bug', 'obsolete']))).toEqual([
			'bug',
			'task'
		]);
		expect(categoryScopeKeys(configured, new Set())).toEqual([]);
	});
});

describe('grouped filtering', () => {
	const selected = (...keys: string[]): Set<string> => new Set(keys);

	it('keeps only the selected category, and only its own work items', () => {
		const filtered = filterGroups(groupedTree.groups, selected('bug'));
		expect(filtered.groups.map((group) => group.category.key)).toEqual(['bug']);
		// `wm-task-child` is a task nested in a bug's subtree; only work items in
		// selected categories remain, so it is not shown under a bug-only scope.
		expect(allIds(filtered.groups.flatMap((group) => group.roots))).toEqual(['wm-bug-login']);
		expect(filtered.counts).toEqual({ items: 1, blocked: 1 });
	});

	it('keeps work items from all selected categories', () => {
		const filtered = filterGroups(groupedTree.groups, selected('bug', 'task'));
		expect(filtered.groups.map((group) => group.category.key)).toEqual(['bug', 'task']);
		expect(filtered.counts).toEqual({ items: 3, blocked: 1 });
	});

	it('keeps a required unselected parent as context instead of promoting its child', () => {
		// `wm-task-child`'s parent is a bug. A task-only scope keeps that bug as
		// temporary context: the child stays at depth 1 under its real parent.
		const filtered = filterGroups(groupedTree.groups, selected('task'));
		const bugs = filtered.groups.find((group) => group.category.key === 'bug');
		expect(bugs?.roots.map((node) => node.id)).toEqual(['wm-bug-login']);
		expect(bugs?.roots[0].children.map((node) => node.id)).toEqual(['wm-task-child']);

		// The context ancestor is presentation only: the two tasks are the
		// selected work, so the blocked bug above them is not counted.
		expect(filtered.counts).toEqual({ items: 2, blocked: 0 });
		expect(filtered.contextIds).toEqual(new Set(['wm-bug-login']));
	});

	it('keeps the whole unselected ancestor chain when several levels are out of scope', () => {
		// Docs root → Bug → Task leaf, with unrelated out-of-scope siblings at
		// both unselected levels.
		const filtered = filterGroups(deepGroupedTree.groups, selected('task'));
		const docs = filtered.groups.find((group) => group.category.key === 'docs');
		expect(docs?.roots.map((node) => node.id)).toEqual(['wm-docs-umbrella']);
		expect(docs?.roots[0].children.map((node) => node.id)).toEqual(['wm-bug-mid']);
		expect(docs?.roots[0].children[0].children.map((node) => node.id)).toEqual(['wm-task-leaf']);

		// Both unselected ancestors are context; only the task is selected work,
		// and the blocked context ancestor does not inflate the blocked count.
		expect(filtered.contextIds).toEqual(new Set(['wm-docs-umbrella', 'wm-bug-mid']));
		expect(filtered.counts).toEqual({ items: 2, blocked: 0 });
	});

	it('does not drag in out-of-scope siblings of a kept context ancestor', () => {
		const filtered = filterGroups(deepGroupedTree.groups, selected('task'));
		const ids = allIds(filtered.groups.flatMap((group) => group.roots));
		expect(ids).not.toContain('wm-docs-sibling');
		expect(ids).not.toContain('wm-bug-sibling');
		expect(filtered.contextIds.has('wm-docs-sibling')).toBe(false);
		expect(filtered.contextIds.has('wm-bug-sibling')).toBe(false);
	});

	it('keeps in-scope descendants under a context ancestor', () => {
		const filtered = filterGroups(groupedTree.groups, selected('task'));
		expect(allIds(filtered.groups.flatMap((group) => group.roots))).toEqual([
			'wm-bug-login',
			'wm-task-child',
			'wm-task-ship'
		]);
	});

	it('keeps a context ancestor when its own category is unselected but holds selected work', () => {
		// Both Bugs and Tasks are in scope; the Docs umbrella only has to keep the
		// chain, while the Bugs in the chain are real selected work.
		const filtered = filterGroups(deepGroupedTree.groups, selected('bug', 'task'));
		expect(filtered.groups.map((group) => group.category.key)).toEqual(['docs', 'task']);
		expect(filtered.contextIds).toEqual(new Set(['wm-docs-umbrella']));
		expect(filtered.counts).toEqual({ items: 4, blocked: 1 });
	});

	it('keeps a group only when it holds selected work or a required ancestor', () => {
		// Docs is configured but holds no active work: nothing to show.
		const filtered = filterGroups(groupedTree.groups, selected('docs'));
		expect(filtered.groups).toEqual([]);
		expect(filtered.counts).toEqual({ items: 0, blocked: 0 });
		expect(filtered.contextIds).toEqual(new Set());
	});

	it('shows nothing when no category is selected', () => {
		const filtered = filterGroups(groupedTree.groups, selected());
		expect(filtered.groups).toEqual([]);
		expect(filtered.counts).toEqual({ items: 0, blocked: 0 });
		expect(filtered.contextIds).toEqual(new Set());
	});

	it('keeps the payload order and drops a selected category with no active work', () => {
		const filtered = filterGroups(groupedTree.groups, selected('docs', 'task', 'bug'));
		expect(filtered.groups.map((group) => group.category.key)).toEqual(['bug', 'task']);
	});
});
