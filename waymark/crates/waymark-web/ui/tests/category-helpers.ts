/**
 * Category-control test helpers.
 *
 * The picker is a closed popover until the reader opens it, so a test that
 * wants an option must open the control first. Keeping that step in one place
 * makes every test exercise the same entry point the reader uses, instead of
 * reaching into a popover that is not on screen.
 */
import { fireEvent, screen, within } from '@testing-library/svelte';

/** Open the picker if it is closed, and return its popover. */
export async function openCategories(): Promise<HTMLElement> {
	const open = screen.queryByTestId('category-popover');
	if (open !== null) return open;
	await fireEvent.click(screen.getByTestId('category-trigger'));
	return screen.findByTestId('category-popover');
}

/** Click one category option, opening the popover first. */
export async function clickCategory(key: string): Promise<void> {
	const popover = await openCategories();
	await fireEvent.click(within(popover).getByTestId(`category-option-${key}`));
}

/** One option, with the popover open so it is on screen. */
export async function categoryOption(key: string): Promise<HTMLElement> {
	const popover = await openCategories();
	return within(popover).getByTestId(`category-option-${key}`);
}

/** Click the named popover action (`Select all categories` / `Clear all categories`). */
export async function clickCategoryAction(name: string): Promise<void> {
	const popover = await openCategories();
	await fireEvent.click(within(popover).getByRole('button', { name }));
}

/** The selected state of one option, opening the popover first. */
export async function pressed(key: string): Promise<string | null> {
	return (await categoryOption(key)).getAttribute('aria-pressed');
}
