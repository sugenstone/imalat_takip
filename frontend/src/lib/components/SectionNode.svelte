<script lang="ts">
	import type { Section } from '$lib/api/types';
	import SectionNode from './SectionNode.svelte';

	let {
		node,
		childrenOf,
		expanded,
		onToggle,
		onAction
	}: {
		node: Section;
		childrenOf: (id: string) => Section[];
		expanded: Set<string>;
		onToggle: (id: string) => void;
		onAction: (action: string, node: Section) => void;
	} = $props();

	const children = $derived(childrenOf(node.id));
	const isOpen = $derived(expanded.has(node.id));
	let menuOpen = $state(false);

	function closeMenu() {
		menuOpen = false;
	}
</script>

<div class="flex items-center gap-1 rounded-lg px-1 py-0.5 hover:bg-slate-50">
	<button
		type="button"
		class="flex size-11 shrink-0 items-center justify-center rounded-md text-slate-400 disabled:opacity-30"
		onclick={() => onToggle(node.id)}
		disabled={children.length === 0}
		aria-label={isOpen ? 'Kapat' : 'Aç'}
	>
		<svg
			class="size-4 transition-transform {isOpen ? 'rotate-90' : ''}"
			fill="none"
			viewBox="0 0 24 24"
			stroke="currentColor"
			stroke-width="2"
		>
			<path stroke-linecap="round" stroke-linejoin="round" d="m9 5 7 7-7 7" />
		</svg>
	</button>

	<button
		type="button"
		class="flex min-h-11 flex-1 items-center truncate py-1 text-left text-sm font-medium
			{node.archived ? 'text-slate-400 line-through' : 'text-slate-800'}"
		onclick={() => (children.length > 0 ? onToggle(node.id) : undefined)}
	>
		{node.name}
		{#if children.length > 0}
			<span class="ml-2 shrink-0 text-xs font-normal text-slate-400">{children.length}</span>
		{/if}
	</button>

	<div class="relative">
		<button
			type="button"
			class="flex size-11 items-center justify-center rounded-md text-slate-400 hover:bg-slate-100 hover:text-slate-600"
			onclick={() => (menuOpen = !menuOpen)}
			aria-label="İşlemler"
		>
			<svg class="size-5" fill="currentColor" viewBox="0 0 24 24">
				<path d="M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2Zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2Zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2Z" />
			</svg>
		</button>

		{#if menuOpen}
			<button type="button" class="fixed inset-0 z-40 cursor-default" onclick={closeMenu} aria-label="Menüyü kapat"></button>
			<div class="absolute right-0 z-50 mt-1 w-52 overflow-hidden rounded-xl bg-white py-1 shadow-lg ring-1 ring-slate-200">
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('workitem', node); }}>İş Kalemi Ekle</button>
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('add', node); }}>Alt Bölüm Ekle</button>
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('serial', node); }}>Seri Oluştur</button>
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('rename', node); }}>Yeniden Adlandır</button>
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('clone', node); }}>Kopyala</button>
				<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm hover:bg-slate-50"
					onclick={() => { closeMenu(); onAction('move', node); }}>Taşı</button>
				{#if !node.archived}
					<button type="button" class="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm text-red-600 hover:bg-red-50"
						onclick={() => { closeMenu(); onAction('archive', node); }}>Arşivle</button>
				{/if}
			</div>
		{/if}
	</div>
</div>

{#if isOpen && children.length > 0}
	<div class="ml-6 border-l border-slate-200 pl-1">
		{#each children as child (child.id)}
			<SectionNode node={child} {childrenOf} {expanded} {onToggle} {onAction} />
		{/each}
	</div>
{/if}
