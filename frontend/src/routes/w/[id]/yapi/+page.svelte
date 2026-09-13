<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Section } from '$lib/api/types';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SectionNode from '$lib/components/SectionNode.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import ProgressBar from '$lib/components/ProgressBar.svelte';
	import SectionPicker from '$lib/components/SectionPicker.svelte';
	import type { WorkItem } from '$lib/api/types';
	import { statusBadgeClass, statusLabel } from '$lib/api/types';

	let sections = $state<Section[]>([]);
	let loading = $state(true);
	let showArchived = $state(false);
	let expanded = $state(new Set<string>());

	// Drill-down kart görünümü
	let viewMode = $state<'cards' | 'tree'>('cards');
	let currentId = $state<string | null>(null);
	let levelItems = $state<WorkItem[]>([]);
	let menuOpenId = $state<string | null>(null);

	const wid = $derived(page.params.id ?? '');

	$effect(() => {
		wid;
		void load();
	});

	// Seviye degisince bu bolumun is kalemlerini cek
	$effect(() => {
		currentId;
		wid;
		if (currentId) {
			void api
				.get<WorkItem[]>(`/workspaces/${wid}/work-items?section_id=${currentId}`)
				.then((r) => (levelItems = r))
				.catch(() => (levelItems = []));
		} else {
			levelItems = [];
		}
		menuOpenId = null;
	});

	async function load() {
		loading = true;
		try {
			const [secs, items] = await Promise.all([
				api.get<Section[]>(`/workspaces/${wid}/sections?include_archived=${showArchived}`),
				currentId
					? api.get<WorkItem[]>(`/workspaces/${wid}/work-items?section_id=${currentId}`)
					: Promise.resolve([] as WorkItem[])
			]);
			levelItems = items;
			sections = secs;
			if (expanded.size === 0) {
				const s = new Set<string>();
				for (const sec of sections.filter((x) => x.depth === 0 && !x.archived)) s.add(sec.id);
				expanded = s;
			}
		} catch {
			sections = [];
		} finally {
			loading = false;
		}
	}

	const visible = $derived(sections.filter((s) => showArchived || !s.archived));

	const breadcrumb = $derived.by(() => {
		const chain: Section[] = [];
		let cur = currentId ? sections.find((s) => s.id === currentId) : undefined;
		while (cur) {
			chain.unshift(cur);
			cur = cur.parent_id ? sections.find((s) => s.id === cur!.parent_id) : undefined;
		}
		return chain;
	});

	const currentChildren = $derived(
		currentId
			? visible.filter((s) => s.parent_id === currentId).sort((a, b) => a.sort_order - b.sort_order)
			: visible.filter((s) => !s.parent_id).sort((a, b) => a.sort_order - b.sort_order)
	);

	function gotoLevel(id: string | null) {
		currentId = id;
	}

	function childCount(s: Section): number {
		return visible.filter((c) => c.parent_id === s.id).length;
	}
	const roots = $derived(
		visible.filter((s) => !s.parent_id || !sections.some((p) => p.id === s.parent_id))
	);

	function childrenOf(id: string): Section[] {
		return visible.filter((s) => s.parent_id === id).sort((a, b) => a.sort_order - b.sort_order);
	}

	function toggle(id: string) {
		const next = new Set(expanded);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		expanded = next;
	}

	// --- Modal durumlari ---
	type Modal =
		| { kind: 'add'; parent: Section | null }
		| { kind: 'serial'; parent: Section | null }
		| { kind: 'rename'; node: Section }
		| { kind: 'move'; node: Section }
		| { kind: 'archive'; node: Section }
		| null;

	let modal = $state<Modal>(null);
	let name = $state('');
	let template = $state('');
	let serialStart = $state(1);
	let serialEnd = $state(10);
	// Ic ice seri (Kat x Daire)
	let nestedMode = $state(false);
	let outerTemplate = $state('Kat {n}');
	let innerTemplate = $state('Daire {n}');
	let innerCount = $state(10);
	let innerNumbering = $state('continue');
	let moveParent = $state('');
	let moveParentName = $state('');
	let movePickerOpen = $state(false);
	let busy = $state(false);
	let error = $state('');

	const modalTitle = $derived(
		!modal
			? ''
			: modal.kind === 'add'
				? modal.parent
					? `Alt Bölüm Ekle — ${modal.parent.name}`
					: 'Kök Bölüm Ekle'
				: modal.kind === 'serial'
					? `Seri Oluştur${modal.parent ? ` — ${modal.parent.name}` : ''}`
					: modal.kind === 'rename'
						? 'Yeniden Adlandır'
						: modal.kind === 'move'
							? `Taşı — ${modal.node.name}`
							: `Arşivle — ${modal.node.name}`
	);

	function openAdd(parent: Section | null) {
		error = '';
		name = '';
		modal = { kind: 'add', parent };
	}

	function onAction(action: string, node: Section) {
		error = '';
		busy = false;
		if (action === 'add') {
			openAdd(node);
		} else if (action === 'serial') {
			template = 'Daire {n}';
			serialStart = 1;
			serialEnd = 10;
			nestedMode = false;
			outerTemplate = 'Kat {n}';
			innerTemplate = 'Daire {n}';
			innerCount = 10;
			innerNumbering = 'continue';
			modal = { kind: 'serial', parent: node };
		} else if (action === 'rename') {
			name = node.name;
			modal = { kind: 'rename', node };
		} else if (action === 'clone') {
			void doClone(node);
		} else if (action === 'move') {
			moveParent = '';
			moveParentName = '';
			modal = { kind: 'move', node };
		} else if (action === 'archive') {
			modal = { kind: 'archive', node };
		} else if (action === 'workitem') {
			// Isler sayfasinda create sheet'i bu bolum onsecili acilir
			void goto(`/w/${wid}/isler?yeni-is=1&section=${node.id}`);
		}
	}

	function handleErr(err: unknown, fallback: string) {
		error = err instanceof ApiError ? err.message : fallback;
	}

	async function doClone(node: Section) {
		error = '';
		try {
			await api.post(`/workspaces/${wid}/sections/${node.id}/clone`, {});
			await load();
		} catch (err) {
			handleErr(err, 'Kopyalanamadı');
		}
	}

	async function submitModal(e: SubmitEvent) {
		e.preventDefault();
		if (!modal) return;
		error = '';
		busy = true;
		try {
			if (modal.kind === 'add') {
				await api.post(`/workspaces/${wid}/sections`, {
					name,
					parent_id: modal.parent?.id ?? null
				});
				if (modal.parent) toggle(modal.parent.id);
			} else if (modal.kind === 'serial') {
				if (nestedMode) {
					await api.post(`/workspaces/${wid}/sections/serial-nested`, {
						parent_id: modal.parent?.id ?? null,
						outer: { template: outerTemplate, start: serialStart, end: serialEnd },
						inner: { template: innerTemplate, count: innerCount, numbering: innerNumbering }
					});
				} else {
					await api.post(`/workspaces/${wid}/sections/serial`, {
						parent_id: modal.parent?.id ?? null,
						template,
						start: serialStart,
						end: serialEnd
					});
				}
				if (modal.parent) toggle(modal.parent.id);
			} else if (modal.kind === 'rename') {
				await api.patch(`/workspaces/${wid}/sections/${modal.node.id}`, { name });
			} else if (modal.kind === 'move') {
				await api.post(`/workspaces/${wid}/sections/${modal.node.id}/move`, {
					parent_id: moveParent || null
				});
				expanded = new Set();
			} else if (modal.kind === 'archive') {
				await api.post(`/workspaces/${wid}/sections/${modal.node.id}/archive`);
			}
			modal = null;
			await load();
		} catch (err) {
			handleErr(err, 'İşlem başarısız');
		} finally {
			busy = false;
		}
	}

	// Tasima hedefleri: node'un kendisi ve tum alt agaci haric (picker'a exclude olarak gecer)
	const moveExcludeIds = $derived.by(() => {
		if (modal?.kind !== 'move') return [];
		const exclude = new Set<string>([modal.node.id]);
		const collect = (id: string) => {
			for (const c of childrenOf(id)) {
				exclude.add(c.id);
				collect(c.id);
			}
		};
		collect(modal.node.id);
		return [...exclude];
	});

	function previewName(tpl: string, n: number, parentName?: string): string {
		return tpl
			.replace('{nnn}', String(n).padStart(3, '0'))
			.replace('{nn}', String(n).padStart(2, '0'))
			.replace('{n}', String(n))
			.replace('{parent}', parentName ?? '');
	}

	const ROOT_NODE: Section = {
		id: '',
		parent_id: null,
		name: '',
		sort_order: 0,
		depth: 0,
		archived: false,
		item_count: 0,
		process_total: 0,
		process_approved: 0
	};
</script>

<svelte:head><title>Yapı</title></svelte:head>

<div class="mb-3 flex items-center justify-between gap-2">
	<div class="flex items-center gap-1 rounded-xl bg-white p-1 shadow-sm ring-1 ring-slate-200">
		<button
			type="button"
			class="flex min-h-9 items-center gap-1.5 rounded-lg px-3 text-xs font-medium transition-colors {viewMode === 'cards' ? 'bg-slate-900 text-white' : 'text-slate-500 hover:bg-slate-100'}"
			onclick={() => (viewMode = 'cards')}
		>
			<Icon name="layout-grid" size={15} /> Kartlar
		</button>
		<button
			type="button"
			class="flex min-h-9 items-center gap-1.5 rounded-lg px-3 text-xs font-medium transition-colors {viewMode === 'tree' ? 'bg-slate-900 text-white' : 'text-slate-500 hover:bg-slate-100'}"
			onclick={() => (viewMode = 'tree')}
		>
			<Icon name="list" size={15} /> Ağaç
		</button>
	</div>
	<label class="flex min-h-9 items-center gap-2 text-xs text-slate-500">
		<input type="checkbox" class="size-3.5 accent-indigo-600" bind:checked={showArchived} onchange={load} />
		Arşiv
	</label>
</div>

{#if loading}
	<div class="flex justify-center py-16">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if viewMode === 'tree'}
	<div class="card py-2">
		{#each roots as root (root.id)}
			<SectionNode node={root} {childrenOf} {expanded} onToggle={toggle} {onAction} />
		{/each}
	</div>
{:else}
	<!-- Breadcrumb -->
	{#if currentId}
		<nav class="mb-3 flex items-center gap-1 overflow-x-auto rounded-xl bg-white px-3 py-2 text-sm shadow-sm ring-1 ring-slate-200">
			<button type="button" class="flex shrink-0 items-center gap-1 text-slate-500 hover:text-slate-800" onclick={() => gotoLevel(null)}>
				<Icon name="building" size={14} /> Yapı
			</button>
			{#each breadcrumb as b, i (b.id)}
				<span class="text-slate-300"><Icon name="chevron-right" size={13} /></span>
				{#if i === breadcrumb.length - 1}
					<span class="shrink-0 font-semibold text-slate-800">{b.name}</span>
				{:else}
					<button type="button" class="shrink-0 text-slate-500 hover:text-slate-800" onclick={() => gotoLevel(b.id)}>{b.name}</button>
				{/if}
			{/each}
		</nav>
	{/if}

	{#if currentChildren.length === 0 && levelItems.length === 0 && currentId === null}
		<div class="card">
			<EmptyState
				icon="building"
				title="Bölüm ağacı boş"
				description="Workspace'inize ilk bölümü ekleyin: Bina, Fabrika, Bölge... yapı tamamen size kalmış."
				actionLabel="Kök Bölüm Ekle"
				onaction={() => openAdd(null)}
			/>
		</div>
	{:else}
		<!-- Alt bolum kartlari -->
		{#if currentChildren.length > 0}
			<div class="grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-4">
				{#each currentChildren as sec (sec.id)}
					<div class="group relative flex flex-col rounded-2xl bg-white shadow-sm ring-1 ring-slate-200 transition-all hover:-translate-y-0.5 hover:shadow-md">
						<div class="absolute inset-y-0 left-0 w-1 rounded-l-2xl {sec.process_total > 0 && sec.process_approved === sec.process_total ? 'bg-emerald-500' : 'bg-indigo-500/70'}"></div>
						<button type="button" class="flex flex-1 flex-col p-3.5 pl-4 text-left" onclick={() => gotoLevel(sec.id)}>
							<div class="mb-2 flex size-9 items-center justify-center rounded-xl bg-slate-100 text-slate-600 group-hover:bg-indigo-50 group-hover:text-indigo-600">
								<Icon name={childCount(sec) > 0 ? 'layers' : 'home'} size={18} />
							</div>
							<p class="truncate text-sm font-semibold text-slate-800">{sec.name}</p>
							<p class="mt-0.5 truncate text-[11px] text-slate-400">
								{childCount(sec) > 0 ? `${childCount(sec)} alt bölüm · ` : ''}{sec.item_count} iş kalemi
							</p>
							<div class="mt-2.5">
								<ProgressBar value={sec.process_approved} total={sec.process_total} />
							</div>
						</button>
						<!-- Kart menusu -->
						<div class="absolute top-2 right-2">
							<button
								type="button"
								class="flex size-8 items-center justify-center rounded-lg bg-white/80 text-slate-400 opacity-0 backdrop-blur transition-opacity hover:bg-white hover:text-slate-700 group-hover:opacity-100 focus:opacity-100"
								onclick={() => (menuOpenId = menuOpenId === sec.id ? null : sec.id)}
								aria-label="İşlemler"
							>
								<Icon name="dots" size={16} />
							</button>
							{#if menuOpenId === sec.id}
								<button type="button" class="fixed inset-0 z-40 cursor-default" onclick={() => (menuOpenId = null)} aria-label="Kapat"></button>
								<div class="absolute right-0 z-50 mt-1 w-56 overflow-hidden rounded-xl bg-white py-1 shadow-lg ring-1 ring-slate-200">
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('workitem', sec); }}>
										<Icon name="clipboard" size={15} class="text-indigo-500" /> İş Kalemi Ekle
									</button>
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('add', sec); }}>
										<Icon name="plus" size={15} class="text-slate-500" /> Alt Bölüm Ekle
									</button>
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('serial', sec); }}>
										<Icon name="list" size={15} class="text-slate-500" /> Seri Oluştur
									</button>
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('rename', sec); }}>
										<Icon name="pencil" size={15} class="text-slate-500" /> Yeniden Adlandır
									</button>
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('clone', sec); }}>
										<Icon name="package" size={15} class="text-slate-500" /> Kopyala
									</button>
									<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] hover:bg-slate-50" onclick={() => { menuOpenId = null; onAction('move', sec); }}>
										<Icon name="arrow-right" size={15} class="text-slate-500" /> Taşı
									</button>
									{#if !sec.archived}
										<button type="button" class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-[13px] text-red-600 hover:bg-red-50" onclick={() => { menuOpenId = null; onAction('archive', sec); }}>
											<Icon name="trash" size={15} /> Arşivle
										</button>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{:else if currentId}
			<div class="mb-1 rounded-xl border border-dashed border-slate-300 py-6 text-center text-sm text-slate-400">
				Alt bölüm yok
			</div>
		{/if}

		<!-- Bu bolumun is kalemleri -->
		{#if levelItems.length > 0}
			<h3 class="mb-2 mt-5 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-slate-500">
				<Icon name="clipboard" size={14} /> İş Kalemleri ({levelItems.length})
			</h3>
			<div class="space-y-2">
				{#each levelItems as it (it.id)}
					<a
						href={'/w/' + wid + '/isler/' + it.id}
						class="flex items-center gap-3 rounded-2xl bg-white p-3 shadow-sm ring-1 ring-slate-200 transition-all hover:-translate-y-0.5 hover:shadow-md"
					>
						<div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-indigo-50 text-indigo-600">
							<Icon name="wrench" size={19} />
						</div>
						<div class="min-w-0 flex-1">
							<div class="flex items-center gap-1.5">
								<p class="truncate text-sm font-semibold text-slate-800">{it.name}</p>
								<span class="badge shrink-0 {statusBadgeClass(it.status)}">{statusLabel(it.status)}</span>
							</div>
							<p class="truncate text-[11px] text-slate-400">{it.work_type_name ?? 'Tipsiz'}</p>
							{#if it.process_total > 0}
								<div class="mt-1.5 max-w-56">
									<ProgressBar value={it.process_approved} total={it.process_total} />
								</div>
							{/if}
						</div>
						<Icon name="chevron-right" size={16} class="shrink-0 text-slate-300" />
					</a>
				{/each}
			</div>
		{/if}
	{/if}
{/if}

{#if viewMode === 'cards' && !loading}
	<button type="button" class="btn-secondary mt-4 w-full" onclick={() => openAdd(currentId ? { ...ROOT_NODE, id: currentId, name: breadcrumb[breadcrumb.length - 1]?.name ?? '' } : null)}>
		+ {currentId ? 'Bu seviyeye bölüm ekle' : 'Kök Bölüm Ekle'}
	</button>
{/if}

<Sheet open={modal !== null} title={modalTitle} onclose={() => (modal = null)}>
	<!-- Sheet close butonu modal'i kapatsin diye onclose bagladik; Sheet bind kullanmiyoruz -->
	{#if modal}
		<form class="space-y-4" onsubmit={submitModal}>
			{#if modal.kind === 'add'}
				<div>
					<label class="label" for="sec-name">Bölüm Adı</label>
					<input id="sec-name" class="input" bind:value={name} placeholder="Örn. Blok A, Hat 2, Bölge Doğu" required maxlength={120} />
				</div>
			{:else if modal.kind === 'serial'}
				<label class="flex min-h-11 items-center gap-3 rounded-lg bg-slate-50 px-3 text-sm font-medium text-slate-700">
					<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={nestedMode} />
					İç içe seri (Kat × Daire)
				</label>

				{#if nestedMode}
					<div class="rounded-xl bg-slate-50 p-3">
						<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">Dış Seviye</p>
						<div class="space-y-3">
							<div>
								<label class="label" for="outer-template">Şablon</label>
								<input id="outer-template" class="input" bind:value={outerTemplate} placeholder={'Kat {n}'} required />
							</div>
							<div class="grid grid-cols-2 gap-3">
								<div>
									<label class="label" for="serial-start">Başlangıç</label>
									<input id="serial-start" class="input" type="number" bind:value={serialStart} min="0" required />
								</div>
								<div>
									<label class="label" for="serial-end">Bitiş</label>
									<input id="serial-end" class="input" type="number" bind:value={serialEnd} min={serialStart} required />
								</div>
							</div>
						</div>
					</div>
					<div class="rounded-xl bg-slate-50 p-3">
						<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">İç Seviye (her dış öğenin altında)</p>
						<div class="space-y-3">
							<div>
								<label class="label" for="inner-template">Şablon</label>
								<input id="inner-template" class="input" bind:value={innerTemplate} placeholder={'Daire {n}'} required />
							</div>
							<div class="grid grid-cols-2 gap-3">
								<div>
									<label class="label" for="inner-count">Adet (her seviyede)</label>
									<input id="inner-count" class="input" type="number" bind:value={innerCount} min="1" max="500" required />
								</div>
								<div>
									<label class="label" for="inner-numbering">Numaralandırma</label>
									<select id="inner-numbering" class="input" bind:value={innerNumbering}>
										<option value="continue">Devam eden (1-10, 11-20…)</option>
										<option value="restart">Her seviyede baştan (1-10, 1-10…)</option>
									</select>
								</div>
							</div>
						</div>
					</div>
					<p class="text-xs text-slate-500">
						Önizleme: <strong>{previewName(outerTemplate, serialStart, modal.parent?.name)}</strong> ›
						<strong>
							{previewName(innerTemplate, innerNumbering === 'continue' ? 1 : 1, previewName(outerTemplate, serialStart, modal.parent?.name))} …
							{previewName(innerTemplate, Number(innerCount), '')}
						</strong>
					</p>
				{:else}
					<div>
						<label class="label" for="sec-template">İsim Şablonu</label>
						<input id="sec-template" class="input" bind:value={template} placeholder={'Daire {nn}'} required />
						<p class="mt-1.5 text-xs text-slate-500">
							Tokenlar: <code class="rounded bg-slate-100 px-1">{'{n}'}</code>,
							<code class="rounded bg-slate-100 px-1">{'{nn}'}</code>,
							<code class="rounded bg-slate-100 px-1">{'{nnn}'}</code>,
							<code class="rounded bg-slate-100 px-1">{'{parent}'}</code>
						</p>
					</div>
					<div class="grid grid-cols-2 gap-3">
						<div>
							<label class="label" for="serial-start">Başlangıç</label>
							<input id="serial-start" class="input" type="number" bind:value={serialStart} min="0" required />
						</div>
						<div>
							<label class="label" for="serial-end">Bitiş</label>
							<input id="serial-end" class="input" type="number" bind:value={serialEnd} min={serialStart} required />
						</div>
					</div>
					<p class="text-xs text-slate-500">
						Önizleme: <strong>{previewName(template, serialStart, modal.parent?.name)}</strong> … <strong>{previewName(template, serialEnd, modal.parent?.name)}</strong>
					</p>
				{/if}
			{:else if modal.kind === 'rename'}
				<div>
					<label class="label" for="sec-rename">Yeni Ad</label>
					<input id="sec-rename" class="input" bind:value={name} required maxlength={120} />
				</div>
			{:else if modal.kind === 'move'}
				<div>
					<span class="label">Hedef Üst Bölüm</span>
					<button
						type="button"
						class="input flex items-center justify-between text-left"
						onclick={() => (movePickerOpen = true)}
					>
						<span class={moveParent ? '' : 'text-slate-400'}>
							{moveParent ? moveParentName || 'Seçili bölüm' : '— Kök seviye —'}
						</span>
						<Icon name="chevron-right" size={15} class="shrink-0 text-slate-400" />
					</button>
					<p class="mt-1.5 text-xs text-slate-500">{modal.node.name} seçilen bölümün altına taşınacak.</p>
				</div>
			{:else if modal.kind === 'archive'}
				<p class="text-sm text-slate-600">
					<strong>{modal.node.name}</strong> ve tüm alt bölümleri arşivlenecek. Geçmiş kayıtlar korunur.
				</p>
			{/if}

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}

			<div class="flex gap-2">
				<button type="button" class="btn-secondary flex-1" onclick={() => (modal = null)}>İptal</button>
				<button
					type="submit"
					class="btn-primary flex-1 {modal?.kind === 'archive' ? 'bg-red-600 hover:bg-red-700' : ''}"
					disabled={busy}
				>
					{busy ? 'İşleniyor…' : modal.kind === 'archive' ? 'Arşivle' : 'Kaydet'}
				</button>
			</div>
		</form>
	{/if}
</Sheet>

<!-- Tasima hedef secici: node + alt agaci haric, kok secilebilir -->
{#if modal?.kind === 'move'}
	<SectionPicker
		bind:open={movePickerOpen}
		title="Hedef Üst Bölüm"
		{wid}
		bind:value={moveParent}
		bind:valueName={moveParentName}
		allowRoot={true}
		rootLabel="Kök seviye"
		excludeIds={moveExcludeIds}
	/>
{/if}
