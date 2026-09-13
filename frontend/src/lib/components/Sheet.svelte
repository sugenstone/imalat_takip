<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		open = $bindable(false),
		title,
		onclose,
		children
	}: {
		open?: boolean;
		title: string;
		onclose?: () => void;
		children: Snippet;
	} = $props();

	function close() {
		open = false;
		onclose?.();
	}
</script>

{#if open}
	<!-- Mobilde alttan acilan sheet, desktop'ta merkez modal -->
	<div class="fixed inset-0 z-50 flex items-end justify-center sm:items-center">
		<button
			type="button"
			class="absolute inset-0 bg-slate-900/50 backdrop-blur-[2px]"
			onclick={close}
			aria-label="Kapat"
		></button>
		<div
			class="relative flex max-h-[90dvh] w-full flex-col rounded-t-2xl bg-white shadow-xl sm:max-w-lg sm:rounded-2xl"
			role="dialog"
			aria-modal="true"
		>
			<div class="flex items-center justify-between border-b border-slate-200 px-4 py-3">
				<h2 class="truncate pr-2 text-base font-semibold">{title}</h2>
				<button
					type="button"
					class="flex size-11 shrink-0 items-center justify-center rounded-lg text-slate-500 hover:bg-slate-100"
					onclick={close}
					aria-label="Kapat"
				>
					<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
					</svg>
				</button>
			</div>
			<div class="overflow-y-auto overscroll-contain p-4 pb-safe">
				{@render children()}
			</div>
		</div>
	</div>
{/if}
