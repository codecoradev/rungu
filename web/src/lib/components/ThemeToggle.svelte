<script lang="ts">
    import { Button } from '$lib/components/ui/button';

    let {
        theme = $bindable('system'),
    }: { theme?: 'light' | 'dark' | 'system' } = $props();

    // Read saved theme on mount (not during SSR)
    import { onMount } from 'svelte';
    onMount(() => {
        try {
            const saved = localStorage.getItem('rungu-theme');
            if (saved === 'light' || saved === 'dark' || saved === 'system') {
                theme = saved;
            }
        } catch {}
    });

    function cycle() {
        const order: Array<'light' | 'dark' | 'system'> = ['light', 'dark', 'system'];
        const idx = order.indexOf(theme);
        theme = order[(idx + 1) % order.length];
        applyTheme();
    }

    function applyTheme() {
        const root = document.documentElement;
        const prefersLight = window.matchMedia('(prefers-color-scheme: light)').matches;

        const isLight = theme === 'light' || (theme === 'system' && prefersLight);

        if (isLight) {
            root.classList.add('light');
        } else {
            root.classList.remove('light');
        }

        try {
            localStorage.setItem('rungu-theme', theme);
        } catch {
            // localStorage unavailable
        }
    }

    $effect(() => {
        applyTheme();
    });
</script>

<Button variant="ghost" size="icon-sm" onclick={cycle} title={`Theme: ${theme}`}>
    {#if theme === 'light'}
        <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
            <circle cx="10" cy="10" r="4" />
            <path d="M10 1a1 1 0 011 1v1a1 1 0 11-2 0V2a1 1 0 011-1zm0 16a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zm9-8a1 1 0 010 2h-1a1 1 0 110-2h1zM3 10a1 1 0 010 2H2a1 1 0 110-2h1zm12.07-6.07a1 1 0 010 1.41l-.71.71a1 1 0 11-1.41-1.41l.71-.71a1 1 0 011.41 0zM6.34 14.66a1 1 0 010 1.41l-.71.71a1 1 0 11-1.41-1.41l.71-.71a1 1 0 011.41 0zm9.19 1.41a1 1 0 01-1.41 0l-.71-.71a1 1 0 011.41-1.41l.71.71a1 1 0 010 1.41zM6.34 5.76a1 1 0 01-1.41 0l-.71-.71a1 1 0 011.41-1.41l.71.71a1 1 0 010 1.41z" />
        </svg>
    {:else if theme === 'dark'}
        <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
            <path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z" />
        </svg>
    {:else}
        <svg class="size-4" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
            <path d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4 8a4 4 0 11-8 0 4 4 0 018 0zm-.464 4.95l.707.707a1 1 0 001.414-1.414l-.707-.707a1 1 0 00-1.414 1.414zm2.12-10.607a1 1 0 010 1.414l-.706.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 011.414 0zM17 11a1 1 0 100-2h-1a1 1 0 100 2h1zm-7 4a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zM5.05 6.464A1 1 0 106.465 5.05l-.708-.707a1 1 0 00-1.414 1.414l.707.707zm1.414 8.486l-.707.707a1 1 0 01-1.414-1.414l.707-.707a1 1 0 011.414 1.414zM4 11a1 1 0 100-2H3a1 1 0 000 2h1z" />
        </svg>
    {/if}
</Button>
