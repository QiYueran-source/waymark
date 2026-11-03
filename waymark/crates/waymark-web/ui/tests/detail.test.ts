import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import DetailPanel from '$lib/components/DetailPanel.svelte';
import { detail, failure } from './fixtures';

function renderDetail(overrides: Record<string, unknown> = {}) {
	const onSelect = vi.fn();
	const onClose = vi.fn();
	const result = render(DetailPanel, {
		id: detail.id,
		detail,
		failure: null,
		loading: false,
		onClose,
		onSelect,
		...overrides
	});
	return { ...result, onSelect, onClose };
}

function textOf(element: Element | null): string {
	return element?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

describe('item detail', () => {
	it('renders the title as text, never as markup', () => {
		renderDetail();
		const title = screen.getByTestId('detail-title');
		expect(textOf(title)).toBe('<b>Login</b> fails');
		expect(title.querySelector('b')).toBeNull();
		expect(title.children).toHaveLength(0);
	});

	it('injects HTML only from body_html', () => {
		renderDetail();
		const body = screen.getByTestId('detail-body');
		expect(body.querySelector('#body-paragraph')).not.toBeNull();
		expect(textOf(body.querySelector('em'))).toBe('markdown');

		// A custom-field value that looks like HTML stays text: names and values
		// carry no semantics, and the only {@html} sink is body_html.
		expect(document.querySelector('script')).toBeNull();
		expect(document.getElementById('body-paragraph')?.parentElement).toBe(body);
		expect(textOf(screen.getByTestId('detail-panel'))).toContain(
			'<script>window.waymarkPwned = true;</script>'
		);
	});

	it('shows category, actual status, derived state, blocked reason, and owner', () => {
		renderDetail();
		const panel = screen.getByTestId('detail-panel');
		expect(panel.textContent).toContain('Bugs');
		expect(textOf(screen.getByTestId('detail-status'))).toBe('In progress');
		expect(textOf(screen.getByTestId('detail-state'))).toBe('Blocked');
		expect(textOf(screen.getByTestId('detail-blocked-reason'))).toBe(
			'waiting on wm-task-ship (unfinished)'
		);
		expect(textOf(screen.getByTestId('detail-owner'))).toBe('Ada Lovelace');
		expect(panel.textContent).toContain('Grace Hopper');
	});

	it('shows custom fields by their stored names, without special semantics', () => {
		renderDetail();
		const names = screen.getAllByTestId('custom-field-name').map(textOf);
		expect(names).toEqual(['Severity', 'Notes']);
		expect(screen.getByTestId('detail-panel').textContent).toContain('kept as text');
	});

	it('shows relationships and lets navigation select a related item', async () => {
		const { onSelect } = renderDetail();
		expect(screen.getByTestId('detail-panel').textContent).toContain('wm-task-child');
		expect(screen.getByTestId('detail-panel').textContent).toContain('wm-task-ship');

		await fireEvent.click(screen.getByRole('button', { name: 'wm-task-ship' }));
		expect(onSelect).toHaveBeenCalledWith('wm-task-ship');
	});

	it('says None for empty relationships and custom fields', () => {
		renderDetail({
			detail: {
				...detail,
				custom_fields: {},
				relationships: {
					parent: null,
					children: [],
					dependencies: [],
					dependents: [],
					soft_links: []
				}
			}
		});
		expect(screen.getByTestId('detail-panel').textContent).toContain('None.');
		expect(screen.getAllByText('None')).toHaveLength(5);
	});

	it('closes on request', async () => {
		const { onClose } = renderDetail();
		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('reports a detail read failure with its diagnostics', () => {
		renderDetail({
			detail: null,
			failure: {
				code: 'ITEM_NOT_FOUND',
				message: 'no item with that id',
				diagnostics: ['work/bugs/wm-missing.md: no such file']
			}
		});
		const alert = screen.getByTestId('detail-failure');
		expect(alert.textContent).toContain('ITEM_NOT_FOUND');
		expect(alert.textContent).toContain('no item with that id');
		expect(alert.textContent).toContain('work/bugs/wm-missing.md: no such file');
		expect(screen.queryByTestId('detail-body')).toBeNull();
	});

	it('uses a string diagnostic verbatim and JSON for structured diagnostics', () => {
		renderDetail({
			detail: null,
			failure: {
				code: 'INCOMPLETE_PROJECTION',
				message: 'incomplete',
				diagnostics: ['plain diagnostic', { file: 'work/tasks/wm-1.md', reason: 'damaged' }]
			}
		});
		const alert = screen.getByTestId('detail-failure');
		expect(alert.textContent).toContain('plain diagnostic');
		expect(alert.textContent).toContain('"reason":"damaged"');
	});
});
