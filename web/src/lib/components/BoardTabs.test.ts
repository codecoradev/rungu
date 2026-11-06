import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import BoardTabs from './BoardTabs.svelte';

describe('BoardTabs', () => {
    it('links every board view and marks only the current one', () => {
        render(BoardTabs, { slug: 'demo', current: 'roadmap' });
        const nav = screen.getByRole('navigation', { name: 'Board views' });
        expect(nav).toBeTruthy();

        const feedback = screen.getByRole('link', { name: 'Feedback' });
        const roadmap = screen.getByRole('link', { name: 'Roadmap' });
        const changelog = screen.getByRole('link', { name: 'Changelog' });
        expect(feedback.getAttribute('href')).toBe('/board/demo');
        expect(roadmap.getAttribute('href')).toBe('/board/demo/roadmap');
        expect(changelog.getAttribute('href')).toBe('/board/demo/changelog');

        expect(roadmap.getAttribute('aria-current')).toBe('page');
        expect(feedback.hasAttribute('aria-current')).toBe(false);
        expect(changelog.hasAttribute('aria-current')).toBe(false);
    });

    it('does not claim ARIA tab semantics for page links', () => {
        render(BoardTabs, { slug: 'demo', current: 'feedback' });
        expect(screen.queryAllByRole('tab')).toHaveLength(0);
        expect(screen.queryAllByRole('tablist')).toHaveLength(0);
    });
});
