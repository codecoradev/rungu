<script lang="ts">
    import { Button } from '$lib/components/ui/button';
    import Sun from '@lucide/svelte/icons/sun';
    import Moon from '@lucide/svelte/icons/moon';
    import Monitor from '@lucide/svelte/icons/monitor';
    import { browser } from '$app/environment';
    import {
        getStoredTheme,
        applyTheme,
        setTheme,
        cycleTheme,
        watchSystemTheme,
        type Theme,
    } from '$lib/theme';

    // Read the persisted theme once on the client; 'system' during SSR so the
    // server-rendered markup matches the pre-hydration FOUC script output.
    let theme = $state<Theme>(browser ? getStoredTheme() : 'system');

    function cycle() {
        theme = cycleTheme(theme);
        setTheme(theme);
    }

    // Apply on mount and whenever the user changes modes.
    $effect(() => {
        applyTheme(theme);
    });

    // When following the OS, react to live changes without a reload.
    $effect(() => {
        if (theme !== 'system') return;
        const stop = watchSystemTheme(() => applyTheme('system'));
        return stop;
    });
</script>

<Button variant="ghost" size="icon-sm" onclick={cycle} title={`Theme: ${theme}`} aria-label={`Switch theme (current: ${theme})`}>
    {#if theme === 'light'}
        <Sun class="size-4" aria-hidden="true" />
    {:else if theme === 'dark'}
        <Moon class="size-4" aria-hidden="true" />
    {:else}
        <Monitor class="size-4" aria-hidden="true" />
    {/if}
</Button>
