<script lang="ts">
    import AuthProviderButtons from '$lib/components/AuthProviderButtons.svelte';
</script>

<svelte:head>
    <title>Login — Rungu</title>
</svelte:head>

<div class="mx-auto max-w-sm py-16">
    <div class="text-center">
        <h1 class="text-2xl font-bold text-gray-900">Welcome back</h1>
        <p class="mt-2 text-sm text-gray-500">Sign in to share your feedback</p>
    </div>

    <div class="mt-8">
        <AuthProviderButtons />
    </div>
</div>
