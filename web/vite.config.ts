import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
    plugins: [tailwindcss(), sveltekit()],
    server: {
        proxy: {
            '/api': 'http://localhost:3000',
            '/auth': 'http://localhost:3000'
        }
    },
    test: {
        include: ['src/**/*.{test,spec}.{js,ts}'],
        environment: 'jsdom',
        globals: true,
        setupFiles: ['src/test-setup.ts']
    },
    // Component tests (@testing-library/svelte) must resolve Svelte's browser
    // build, not the server one — otherwise mount() fails with
    // lifecycle_function_unavailable. Scoped to vitest runs only.
    resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined
});
