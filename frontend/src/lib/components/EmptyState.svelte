<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';

	let {
		icon = 'inbox',
		title,
		description = '',
		actionLabel = '',
		onaction
	}: {
		icon?: string;
		title: string;
		description?: string;
		actionLabel?: string;
		onaction?: () => void;
	} = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 py-12 text-center">
	<div class="mb-3 flex size-16 items-center justify-center rounded-2xl bg-slate-100 text-slate-400">
		<Icon name={icon} size={30} />
	</div>
	<h3 class="text-base font-semibold text-slate-800">{title}</h3>
	{#if description}
		<p class="mt-1 max-w-sm text-sm text-slate-500">{description}</p>
	{/if}
	{#if actionLabel && onaction}
		<button type="button" class="btn-primary mt-4" onclick={onaction}>{actionLabel}</button>
	{/if}
</div>
