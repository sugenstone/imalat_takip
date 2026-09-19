<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { AttributeDefinition, WorkType, WorkflowTemplate } from '$lib/api/types';
	import { ATTRIBUTE_DATA_TYPES } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';

	let {
		wid,
		workTypes,
		reload
	}: {
		wid: string;
		workTypes: WorkType[];
		reload: () => Promise<void>;
	} = $props();

	let createOpen = $state(false);
	let editType = $state<WorkType | null>(null);
	let tName = $state('');
	let tDesc = $state('');
	let busy = $state(false);
	let error = $state('');

	// Varsayilan akis (gercek senaryo paketi)
	let workflows = $state<WorkflowTemplate[]>([]);
	let tDefaultFlow = $state('');

	$effect(() => {
		void (async () => {
			try {
				const wf = await api.get<WorkflowTemplate[]>('/workspaces/{wid}/workflows'.replace('{wid}', wid));
				workflows = wf.filter((w) => w.published_version);
			} catch {
				workflows = [];
			}
		})();
	});

	// Ozellik editoru
	let attrSheet = $state(false);
	let attrFormOpen = $state(false);
	let editingAttr = $state<AttributeDefinition | null>(null);
	let attrDefs = $state<AttributeDefinition[]>([]);
	let aName = $state('');
	let aType = $state('text');
	let aUnit = $state('');
	let aRequired = $state(false);
	let aFilterable = $state(false);
	let aDefault = $state('');
	let aOptions = $state('');

	function openCreate() {
		error = '';
		tName = '';
		tDesc = '';
		createOpen = true;
	}

	function openEdit(t: WorkType) {
		error = '';
		tName = t.name;
		tDesc = t.description ?? '';
		tDefaultFlow = '';
		editType = t;
	}

	async function saveType(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			if (editType) {
				await api.patch(`/workspaces/${wid}/work-types/${editType.id}`, {
					name: tName,
					description: tDesc || null,
					default_workflow_template_id: tDefaultFlow || null
				});
				editType = null;
			} else {
				await api.post(`/workspaces/${wid}/work-types`, {
					name: tName,
					description: tDesc || null
				});
				createOpen = false;
			}
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function archiveType(t: WorkType) {
		if (!confirm(`${t.name} arşivlensin mi? Bu tipe bağlı iş kalemleri etkilenmez.`)) return;
		error = '';
		try {
			await api.post(`/workspaces/${wid}/work-types/${t.id}/archive`, undefined);
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Arşivlenemedi';
		}
	}

	// --- Ozellik yonetimi ---
	async function openAttrs(t: WorkType) {
		error = '';
		editType = t;
		try {
			attrDefs = await api.get<AttributeDefinition[]>(
				`/workspaces/${wid}/work-types/${t.id}/attributes`
			);
			attrSheet = true;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yüklenemedi';
		}
	}

	function openAttrEditor(d: AttributeDefinition | null) {
		error = '';
		editingAttr = d;
		attrFormOpen = true;
		if (d) {
			aName = d.name;
			aType = d.data_type;
			aUnit = '';
			aRequired = d.is_required;
			aFilterable = d.is_filterable;
			aDefault = d.default_value ?? '';
			aOptions = d.options.join('\n');
		} else {
			aName = '';
			aType = 'text';
			aUnit = '';
			aRequired = false;
			aFilterable = false;
			aDefault = '';
			aOptions = '';
		}
	}

	async function saveAttr(e: SubmitEvent) {
		e.preventDefault();
		if (!editType) return;
		error = '';
		busy = true;
		const options = aOptions
			.split('\n')
			.map((s) => s.trim())
			.filter(Boolean);
		try {
			if (editingAttr) {
				await api.patch(
					`/workspaces/${wid}/work-types/${editType.id}/attributes/${editingAttr.id}`,
					{
						name: aName,
						unit: aUnit || null,
						is_required: aRequired,
						is_filterable: aFilterable,
						default_value: aDefault || null,
						options
					}
				);
			} else {
				await api.post(`/workspaces/${wid}/work-types/${editType.id}/attributes`, {
					name: aName,
					data_type: aType,
					unit: aUnit || null,
					is_required: aRequired,
					is_filterable: aFilterable,
					default_value: aDefault || null,
					options
				});
			}
			editingAttr = null;
			attrFormOpen = false;
			attrDefs = await api.get<AttributeDefinition[]>(
				`/workspaces/${wid}/work-types/${editType.id}/attributes`
			);
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function archiveAttr(d: AttributeDefinition) {
		if (!editType || !confirm(`${d.name} özelliği arşivlensin mi?`)) return;
		error = '';
		try {
			await api.post(
				`/workspaces/${wid}/work-types/${editType.id}/attributes/${d.id}/archive`,
				undefined
			);
			attrDefs = await api.get<AttributeDefinition[]>(
				`/workspaces/${wid}/work-types/${editType.id}/attributes`
			);
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Arşivlenemedi';
		}
	}

	const typeLabel = (v: string) => ATTRIBUTE_DATA_TYPES.find((t) => t.value === v)?.label ?? v;
</script>

<div class="mb-3 flex items-center justify-between">
	<h2 class="font-semibold">İş Tipleri ({workTypes.length})</h2>
	<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openCreate}>+ Ekle</button>
</div>

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}

{#if workTypes.length === 0}
	<div class="card">
		<EmptyState
			icon="package"
			title="Henüz iş tipi yok"
			description="Montaj, Boya, Kalite Kontrol gibi tipler tanımlayın; her tipe özel özellik şeması ekleyebilirsiniz."
			actionLabel="İş Tipi Oluştur"
			onaction={openCreate}
		/>
	</div>
{:else}
	<div class="card divide-y divide-slate-100 !p-0">
		{#each workTypes as t (t.id)}
			<div class="flex items-center gap-2 px-4 py-3">
				<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openEdit(t)}>
					<p class="truncate text-sm font-medium">{t.name}</p>
					<p class="truncate text-xs text-slate-500">{t.attribute_count} özellik</p>
				</button>
				<button
					type="button"
					class="flex min-h-11 items-center rounded-lg px-2 text-xs font-medium text-indigo-600 hover:bg-indigo-50"
					onclick={() => openAttrs(t)}
				>
					Özellikler
				</button>
				<button
					type="button"
					class="flex size-11 items-center justify-center rounded-lg text-red-500 hover:bg-red-50"
					onclick={() => archiveType(t)}
					aria-label="Arşivle"
				>
					<svg class="size-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
						<path stroke-linecap="round" stroke-linejoin="round" d="m20 7-1-3H5L4 7m16 0v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7m16 0H4m5 5h6" />
					</svg>
				</button>
			</div>
		{/each}
	</div>
{/if}

<!-- Tip olustur/duzenle -->
<Sheet bind:open={createOpen} title="Yeni İş Tipi">
	<form class="space-y-4" onsubmit={saveType}>
		<div>
			<label class="label" for="wt-name">Tip Adı</label>
			<input id="wt-name" class="input" bind:value={tName} placeholder="Örn. Montaj" required maxlength={60} />
		</div>
		<div>
			<label class="label" for="wt-desc">Açıklama</label>
			<input id="wt-desc" class="input" bind:value={tDesc} maxlength={200} />
		</div>
		<div>
			<label class="label" for="wt-flow">Varsayılan Süreç Grubu (otomatik atama)</label>
			<select id="wt-flow" class="input" bind:value={tDefaultFlow}>
				<option value="">Yok</option>
				{#each workflows as w (w.id)}
					<option value={w.id}>{w.name} (v{w.published_version})</option>
				{/each}
			</select>
			<p class="mt-1.5 text-xs text-slate-500">
				Bu tipte iş kalemi oluşturulduğunda (auto ile) akış otomatik atanır.
			</p>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (createOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !tName.trim()}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>

<!-- Tip duzenle -->
<Sheet open={editType !== null && !attrSheet} title={editType ? `Tip: ${editType.name}` : ''} onclose={() => (editType = null)}>
	{#if editType}
		<form class="space-y-4" onsubmit={saveType}>
			<div>
				<label class="label" for="et-name">Tip Adı</label>
				<input id="et-name" class="input" bind:value={tName} required maxlength={60} />
			</div>
			<div>
				<label class="label" for="et-desc">Açıklama</label>
				<input id="et-desc" class="input" bind:value={tDesc} maxlength={200} />
			</div>
			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
			<div class="flex gap-2">
				<button type="submit" class="btn-primary flex-1" disabled={busy}>
					{busy ? 'Kaydediliyor…' : 'Kaydet'}
				</button>
				<button
					type="button"
					class="btn-secondary"
					onclick={() => void openAttrs(editType!)}
				>
					Özellikler
				</button>
			</div>
		</form>
	{/if}
</Sheet>

<!-- Ozellik yonetimi -->
<Sheet bind:open={attrSheet} title={editType ? `Özellikler — ${editType.name}` : ''} onclose={() => { attrSheet = false; attrFormOpen = false; editingAttr = null; }}>
	{#if !attrFormOpen}
		<div class="space-y-3">
			{#if attrDefs.length === 0}
				<p class="py-4 text-center text-sm text-slate-500">Bu tipe henüz özellik tanımı yok.</p>
			{:else}
				<div class="divide-y divide-slate-100 rounded-xl border border-slate-200">
					{#each attrDefs as d (d.id)}
						<div class="flex items-center gap-2 px-3 py-2.5">
							<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openAttrEditor(d)}>
								<p class="truncate text-sm font-medium">
									{d.name}
									{#if d.is_required}<span class="text-red-500">*</span>{/if}
								</p>
								<p class="truncate text-xs text-slate-500">
									{typeLabel(d.data_type)}
									{#if d.is_filterable} · filtrelenabilir{/if}
									{#if d.options.length > 0} · {d.options.length} seçenek{/if}
								</p>
							</button>
							<button
								type="button"
								class="flex size-11 shrink-0 items-center justify-center rounded-lg text-red-400 hover:bg-red-50"
								onclick={() => archiveAttr(d)}
								aria-label="Özelliği arşivle"
							>
								<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
									<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
								</svg>
							</button>
						</div>
					{/each}
				</div>
			{/if}
			<button type="button" class="btn-primary w-full" onclick={() => openAttrEditor(null)}>
				+ Yeni Özellik
			</button>
		</div>
	{:else}
		<form class="space-y-4" onsubmit={saveAttr}>
			<div>
				<label class="label" for="a-name">Özellik Adı</label>
				<input id="a-name" class="input" bind:value={aName} placeholder="Örn. Metraj" required maxlength={60} />
			</div>
			<div>
				<label class="label" for="a-type">Veri Tipi</label>
				<select id="a-type" class="input" bind:value={aType} disabled={editingAttr !== null}>
					{#each ATTRIBUTE_DATA_TYPES as t (t.value)}
						<option value={t.value}>{t.label}</option>
					{/each}
				</select>
				{#if editingAttr}
					<p class="mt-1.5 text-xs text-amber-600">
						Veri tipi değiştirilemez (kayıtlı veri bütünlüğü). Gerekirse yeni özellik ekleyin.
					</p>
				{/if}
			</div>
			{#if aType === 'select' || aType === 'multiselect'}
				<div>
					<label class="label" for="a-options">Seçenekler (her satır bir seçenek)</label>
					<textarea id="a-options" class="input min-h-28" rows="4" bind:value={aOptions} placeholder={'Laminat\nGranit\nMernet'}></textarea>
				</div>
			{/if}
			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="label" for="a-unit">Birim (opsiyonel)</label>
					<input id="a-unit" class="input" bind:value={aUnit} placeholder="m2, kg, adet…" maxlength={10} />
				</div>
				<div>
					<label class="label" for="a-default">Varsayılan Değer</label>
					<input id="a-default" class="input" bind:value={aDefault} placeholder="Boş bırakılabilir" />
				</div>
			</div>
			<div class="space-y-2">
				<label class="flex min-h-11 items-center gap-3 text-sm text-slate-700">
					<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={aRequired} />
					Zorunlu alan
				</label>
				<label class="flex min-h-11 items-center gap-3 text-sm text-slate-700">
					<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={aFilterable} />
					Listede filtrelenebilir
				</label>
			</div>

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}

			<div class="flex gap-2">
				<button type="button" class="btn-secondary flex-1" onclick={() => { editingAttr = null; attrFormOpen = false; }}>Geri</button>
				<button type="submit" class="btn-primary flex-1" disabled={busy || !aName.trim()}>
					{busy ? 'Kaydediliyor…' : 'Kaydet'}
				</button>
			</div>
		</form>
	{/if}
</Sheet>
