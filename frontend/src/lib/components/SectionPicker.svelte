<script lang="ts">
	// Agac bolum secici: arama + breadcrumb + seviye seviye inme.
	// 179 bolumluk agacta <3 dokunusla dogru kaynagi bulmayi saglar.
	import { api } from '$lib/api/client';
	import type { Section } from '$lib/api/types';
	import Icon from '$lib/components/Icon.svelte';

	let {
		open = $bindable(false),
		title = 'Bölüm Seç',
		wid,
		value = $bindable(''),
		/** Kok seviye secilebilsin mi (orn. tasima hedefi 'kok' olabilir) */
		allowRoot = false,
		rootLabel = 'Kök seviye',
		/** Secilen section'ın adı (parent forma gostermek icin) */
		valueName = $bindable(''),
		excludeIds = [] as string[]
	}: {
		open?: boolean;
		title?: string;
		wid: string;
		value?: string;
		allowRoot?: boolean;
		rootLabel?: string;
		valueName?: string;
		excludeIds?: string[];
	} = $props();

	let sections = $state<Section[]>([]);
	let loading = $state(true);
	let currentId = $state<string | null>(null);
	let search = $state('');

	$effect(() => {
		wid;
		if (open && sections.length === 0) void load();
	});

	async function load() {
		loading = true;
		try {
			sections = await api.get<Section[]>(`/workspaces/${wid}/sections`);
		} catch {
			sections = [];
		} finally {
			loading = false;
		}
	}

	const byId = $derived(new Map(sections.map((s) => [s.id, s])));

	const breadcrumb = $derived.by(() => {
		const chain: Section[] = [];
		let cur = currentId ? byId.get(currentId) : undefined;
		while (cur) {
			chain.unshift(cur);
			cur = cur.parent_id ? byId.get(cur.parent_id) : undefined;
		}
		return chain;
	});

	const currentLabel = $derived(
		currentId ? byId.get(currentId)?.name ?? '' : rootLabel
	);

	const children = $derived(
		sections
			.filter((s) => (s.parent_id ?? null) === currentId && !excludeIds.includes(s.id))
			.sort((a, b) => a.sort_order - b.sort_order)
	);

	const searchResults = $derived.by(() => {
		const q = search.trim().toLowerCase();
		if (!q) return null;
		return sections
			.filter((s) => s.name.toLowerCase().includes(q) && !excludeIds.includes(s.id))
			.slice(0, 50)
			.map((s) => ({
				section: s,
				path: fullPath(s)
			}));
	});

	function fullPath(s: Section): string {
		const parts: string[] = [];
		let cur: Section | undefined = s;
		while (cur) {
			parts.unshift(cur.name);
			cur = cur.parent_id ? byId.get(cur.parent_id) : undefined;
		}
		return parts.join(' › ');
	}

	function select(s: Section) {
		value = s.id;
		valueName = s.name;
		open = false;
		search = '';
	}

	function selectRoot() {
		value = '';
		valueName = rootLabel;
		open = false;
		search = '';
	}

	function goInto(s: Section) {
		currentId = s.id;
		search = '';
	}

	function goUpTo(id: string | null) {
		currentId = id;
		search = '';
	}

	function isSelectedHere(): boolean {
		if (!value) return allowRoot && currentId === null;
		return value === currentId;
	}

	function selectedName(): string {
		if (!value) return rootLabel;
		return byId.get(value)?.name ?? '';
	}
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex flex-col bg-slate-100">
		<!-- Ust bar -->
		<div class="bg-white shadow-sm">
			<div class="flex items-center gap-2 px-3 pt-safe pb-2">
				<button
					type="button"
					class="flex size-11 shrink-0 items-center justify-center rounded-lg text-slate-500 hover:bg-slate-100"
					onclick={() => (open = false)}
					aria-label="Kapat"
				>
					<Icon name="x" size={20} />
				</button>
				<h2 class="flex-1 truncate text-base font-semibold">{title}</h2>
			</div>
			<div class="px-3 pb-2">
				<div class="relative">
					<Icon name="search" size={16} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-slate-400" />
					<input
						class="input !pl-9"
						placeholder="Bölüm ara (örn. Daire 63)…"
						bind:value={search}
					/>
				</div>
			</div>
		</div>

		{#if loading}
			<div class="flex flex-1 items-center justify-center">
				<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
			</div>
		{:else if searchResults}
			<!-- Arama sonuclari: duz liste + tam yol -->
			<div class="flex-1 overflow-y-auto p-3">
				{#if searchResults.length === 0}
					<p class="py-10 text-center text-sm text-slate-400">Sonuç bulunamadı</p>
				{:else}
					<div class="space-y-1.5">
						{#each searchResults as r (r.section.id)}
							<button
								type="button"
								class="flex w-full items-center gap-3 rounded-xl bg-white px-4 py-3 text-left shadow-sm ring-1 ring-slate-200 transition-all hover:shadow-md {value === r.section.id ? 'ring-2 ring-indigo-500' : ''}"
								onclick={() => select(r.section)}
							>
								<div class="flex size-9 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-slate-500">
									<Icon name="folder" size={17} />
								</div>
								<div class="min-w-0 flex-1">
									<p class="truncate text-sm font-semibold">{r.section.name}</p>
									<p class="truncate text-xs text-slate-400">{r.path}</p>
								</div>
								{#if value === r.section.id}
									<Icon name="check" size={16} class="shrink-0 text-indigo-600" />
								{/if}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{:else}
			<!-- Breadcrumb -->
			{#if breadcrumb.length > 0}
				<nav class="flex items-center gap-1 overflow-x-auto border-b border-slate-200 bg-white px-3 py-2 text-sm shadow-sm">
					<button type="button" class="flex shrink-0 items-center gap-1 text-slate-500 hover:text-slate-800" onclick={() => goUpTo(null)}>
						<Icon name="building" size={14} /> Kök
					</button>
					{#each breadcrumb as b, i (b.id)}
						<span class="text-slate-300"><Icon name="chevron-right" size={13} /></span>
						{#if i === breadcrumb.length - 1}
							<span class="shrink-0 font-semibold text-slate-800">{b.name}</span>
						{:else}
							<button type="button" class="shrink-0 text-slate-500 hover:text-slate-800" onclick={() => goUpTo(b.id)}>{b.name}</button>
						{/if}
					{/each}
				</nav>
			{/if}

			<!-- Alt bolumler -->
			<div class="flex-1 overflow-y-auto p-3">
				{#if children.length === 0}
					<p class="py-8 text-center text-sm text-slate-400">Alt bölüm yok</p>
				{:else}
					<div class="grid grid-cols-2 gap-2.5 sm:grid-cols-3">
						{#each children as s (s.id)}
							<button
								type="button"
								class="group flex flex-col rounded-2xl bg-white p-3.5 text-left shadow-sm ring-1 ring-slate-200 transition-all hover:-translate-y-0.5 hover:shadow-md {value === s.id ? 'ring-2 ring-indigo-500' : ''}"
								onclick={() => goInto(s)}
							>
								<div class="mb-2 flex size-9 items-center justify-center rounded-xl bg-slate-100 text-slate-600 group-hover:bg-indigo-50 group-hover:text-indigo-600">
									<Icon name="layers" size={17} />
								</div>
								<p class="truncate text-sm font-semibold">{s.name}</p>
								<p class="mt-0.5 truncate text-[11px] text-slate-400">{s.item_count} iş kalemi</p>
							</button>
						{/each}
					</div>
				{/if}
			</div>

			<!-- Alt: sec butonu -->
			<div class="border-t border-slate-200 bg-white p-3 pb-safe shadow-lg">
				<div class="mb-2 flex items-center gap-2 px-1 text-xs text-slate-500">
					<Icon name="check-square" size={14} />
					{#if isSelectedHere()}
						Seçili: <strong>{selectedName()}</strong>
					{:else}
						Seçmek için <strong>{currentLabel}</strong> düzeyini onaylayın veya bölüme girin
					{/if}
				</div>
				{#if currentId !== null || allowRoot}
					<button
						type="button"
						class="btn-primary w-full"
						onclick={() => (currentId === null ? selectRoot() : select(byId.get(currentId!)!))}
					>
						{currentId === null ? rootLabel : `"${currentLabel}" seç`}
					</button>
				{:else}
					<button type="button" class="btn-secondary w-full" disabled>
						Bir bölüme girin veya arama kullanın
					</button>
				{/if}
			</div>
		{/if}
	</div>
{/if}
