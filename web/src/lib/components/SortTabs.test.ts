import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import SortTabs from './SortTabs.svelte';

const options = [
    { value: 'newest', label: 'Newest' },
    { value: 'trending', label: 'Trending' },
    { value: 'most_votes', label: 'Most Voted' },
];

describe('SortTabs', () => {
    it('exposes the options as toggle buttons with the current one pressed', () => {
        render(SortTabs, { options, value: 'trending', onchange: vi.fn() });
        expect(screen.getByRole('group', { name: 'Sort posts' })).toBeTruthy();
        expect(screen.getByRole('button', { name: 'Trending' }).getAttribute('aria-pressed')).toBe('true');
        expect(screen.getByRole('button', { name: 'Newest' }).getAttribute('aria-pressed')).toBe('false');
        expect(screen.queryAllByRole('tab')).toHaveLength(0);
    });

    it('reports the chosen value from the buttons', async () => {
        const onchange = vi.fn();
        render(SortTabs, { options, value: 'newest', onchange });
        await fireEvent.click(screen.getByRole('button', { name: 'Most Voted' }));
        expect(onchange).toHaveBeenCalledWith('most_votes');
    });

    it('offers the same options in the small-screen select', async () => {
        const onchange = vi.fn();
        render(SortTabs, { options, value: 'newest', onchange });
        const select = screen.getByRole('combobox', { name: 'Sort' }) as HTMLSelectElement;
        expect(select.value).toBe('newest');
        expect(Array.from(select.options).map((o) => o.value)).toEqual(['newest', 'trending', 'most_votes']);
        await fireEvent.change(select, { target: { value: 'trending' } });
        expect(onchange).toHaveBeenCalledWith('trending');
    });
});
