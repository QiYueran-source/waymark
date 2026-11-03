import { describe, expect, it } from 'vitest';

import { allIds, countNodes, sectionsFor, summarize, visibleRoots } from '$lib/tree';
import {
	BUG,
	TASK,
	bugRoot,
	combinedTree,
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
