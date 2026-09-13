<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { WorkItemDetail, Section, WorkType } from '$lib/api/types';
	import {
		statusLabel,
		statusBadgeClass,
		priorityLabel,
		priorityBadgeClass,
		WORK_ITEM_STATUSES,
		WORK_ITEM_PRIORITIES
	} from '$lib/api/types';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import AttributeField from '$lib/components/AttributeField.svelte';
	import WorkflowPanel from '$lib/components/WorkflowPanel.svelte';
	import CommentsPanel from '$lib/components/CommentsPanel.svelte';
	import AttachmentsPanel from '$lib/components/AttachmentsPanel.svelte';
	import { ws } from '$lib/stores/ws.svelte';

	let detail = $state<WorkItemDetail | null>(null);
	let sections = $state<Section[]>([]);
	let workTypes = $state<WorkType[]>([]);
	let loading = $state(true);
	let tab = $state<'genel' | 'ozellikler' | 'akis' | 'yorumlar' | 'dosyalar'>(
		ws.can('work_item.create') ? 'genel' : 'akis'
	);
	let busy = $state(false);
	let error = $state('');
	let saved = $state(false);

	// Isci duzenleme formu gormez: varsayilan sekme Akis
	// (work_item.update Worker'da ozellik girisi icin var; form kapisi create izni)
	const canManage = $derived(ws.can('work_item.create'));
	$effect(() => {
		if (!canManage && (tab === 'genel' || tab === 'ozellikler')) tab = 'akis';
	});

	// Genel form
	let fName = $state('');
	let fDesc = $state('');
	let fSection = $state('');
	let fType = $state('');
	let fPriority = $state('medium');
	let fStatus = $state('draft');
	let fStart = $state('');
	let fEnd = $state('');

	// Ozellik formu
	let attrValues = $state<Record<string, unknown>>({});

	const wid = $derived(page.params.id ?? '');
	const iid = $derived(page.params.itemId ?? '');

	$effect(() => {
		wid;
		iid;
		void load();
	});

	async function load() {
		loading = true;
		try {
			const [d, s, t] = await Promise.all([
				api.get<WorkItemDetail>(`/workspaces/${wid}/work-items/${iid}`),
				api.get<Section[]>(`/workspaces/${wid}/sections`),
				api.get<WorkType[]>(`/workspaces/${wid}/work-types`)
			]);
			detail = d;
			sections = s;
			workTypes = t;
			fName = d.name;
			fDesc = d.description ?? '';
			fSection = d.section_id;
			fType = d.work_type_id ?? '';
			fPriority = d.priority;
			fStatus = d.status;
			fStart = d.planned_start ?? '';
			fEnd = d.planned_end ?? '';
			const vals: Record<string, unknown> = {};
			for (const a of d.attributes) vals[a.definition_id] = a.value;
			attrValues = vals;
		} catch {
			detail = null;
		} finally {
			loading = false;
		}
	}

	async function saveGenel(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		saved = false;
		busy = true;
		try {
			await api.patch(`/workspaces/${wid}/work-items/${iid}`, {
				name: fName,
				description: fDesc || null,
				section_id: fSection,
				work_type_id: fType || null,
				priority: fPriority,
				status: fStatus,
				planned_start: fStart || null,
				planned_end: fEnd || null
			});
			saved = true;
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function saveAttrs(e: SubmitEvent) {
		e.preventDefault();
		if (!detail) return;
		error = '';
		saved = false;
		busy = true;
		try {
			const attributes = detail.attributes.map((a) => ({
				definition_id: a.definition_id,
				value: attrValues[a.definition_id] ?? null
			}));
			await api.patch(`/workspaces/${wid}/work-items/${iid}/attributes`, { attributes });
			saved = true;
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function archive() {
		if (!detail || !confirm(`${detail.name} arşivlensin mi?`)) return;
		try {
			await api.post(`/workspaces/${wid}/work-items/${iid}/archive`, undefined);
			await goto(`/w/${wid}/isler`);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Arşivlenemedi';
		}
	}

	const sectionLabel = (s: Section) => `${'— '.repeat(s.depth)}${s.name}`;
</script>

<svelte:head><title>{detail?.name ?? 'İş Kalemi'}</title></svelte:head>

{#if loading}
	<div class="flex justify-center py-16">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if !detail}
	<div class="card mt-4 text-center text-sm text-slate-500">İş kalemi bulunamadı veya erişiminiz yok.</div>
{:else}
	<!-- Baslik -->
	<div class="mb-4">
		<div class="flex flex-wrap items-center gap-1.5">
			<h2 class="text-lg font-bold">{detail.name}</h2>
			<span class="badge {statusBadgeClass(detail.status)}">{statusLabel(detail.status)}</span>
			<span class="badge {priorityBadgeClass(detail.priority)}">{priorityLabel(detail.priority)}</span>
		</div>
		<p class="mt-1 text-xs text-slate-500">
			{detail.work_type_name ?? 'Tipsiz'} · {detail.section_path.join(' › ')}
		</p>
	</div>

	<!-- Segment (kaydirilabilir) — duzenleme sekmeleri yalniz yetkilide -->
	<div class="mb-4 flex gap-1 overflow-x-auto rounded-xl bg-white p-1 shadow-sm ring-1 ring-slate-200">
		{#each [
			{ id: 'genel', label: 'Genel', show: canManage },
			{
				id: 'ozellikler',
				label: detail.attributes.length > 0 ? `Özellikler (${detail.attributes.length})` : 'Özellikler',
				show: canManage
			},
			{ id: 'akis', label: 'Akış', show: true },
			{ id: 'yorumlar', label: 'Yorumlar', show: true },
			{ id: 'dosyalar', label: 'Dosyalar', show: true }
		].filter((t) => t.show) as t (t.id)}
			<button
				type="button"
				class="flex min-h-11 shrink-0 items-center justify-center rounded-lg px-3 text-sm font-medium transition-colors
					{tab === t.id ? 'bg-indigo-600 text-white' : 'text-slate-600 hover:bg-slate-100'}"
				onclick={() => (tab = t.id as typeof tab)}
			>
				{t.label}
			</button>
		{/each}
	</div>

	{#if tab === 'genel'}
		<form class="card space-y-4" onsubmit={saveGenel}>
			<div>
				<label class="label" for="d-name">Ad</label>
				<input id="d-name" class="input" bind:value={fName} required maxlength={120} />
			</div>
			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="label" for="d-section">Bölüm</label>
					<select id="d-section" class="input" bind:value={fSection}>
						{#each sections as s (s.id)}
							<option value={s.id}>{sectionLabel(s)}</option>
						{/each}
					</select>
				</div>
				<div>
					<label class="label" for="d-type">İş Tipi</label>
					<select id="d-type" class="input" bind:value={fType}>
						<option value="">Tipsiz</option>
						{#each workTypes as t (t.id)}
							<option value={t.id}>{t.name}</option>
						{/each}
					</select>
				</div>
			</div>
			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="label" for="d-priority">Öncelik</label>
					<select id="d-priority" class="input" bind:value={fPriority}>
						{#each WORK_ITEM_PRIORITIES as p (p.value)}
							<option value={p.value}>{p.label}</option>
						{/each}
					</select>
				</div>
				<div>
					<label class="label" for="d-status">Durum</label>
					<select id="d-status" class="input" bind:value={fStatus}>
						{#each WORK_ITEM_STATUSES as s (s.value)}
							<option value={s.value}>{s.label}</option>
						{/each}
					</select>
				</div>
			</div>
			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="label" for="d-ps">Planlanan Başlangıç</label>
					<input id="d-ps" class="input" type="date" bind:value={fStart} />
				</div>
				<div>
					<label class="label" for="d-pe">Planlanan Bitiş</label>
					<input id="d-pe" class="input" type="date" bind:value={fEnd} />
				</div>
			</div>
			<div>
				<label class="label" for="d-desc">Açıklama</label>
				<textarea id="d-desc" class="input min-h-20" rows="3" bind:value={fDesc}></textarea>
			</div>

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
			{#if saved}
				<p class="text-sm text-emerald-600">Kaydedildi</p>
			{/if}

			<div class="flex gap-2">
				<button type="submit" class="btn-primary flex-1" disabled={busy}>
					{busy ? 'Kaydediliyor…' : 'Kaydet'}
				</button>
				<button type="button" class="btn-danger" onclick={archive}>Arşivle</button>
			</div>
		</form>
	{:else if tab === 'ozellikler'}
		<form class="card space-y-4" onsubmit={saveAttrs}>
			<div class="space-y-3">
				{#each detail.attributes as a (a.definition_id)}
					<AttributeField
						def={{ id: a.definition_id, name: a.name, data_type: a.data_type, is_required: a.is_required, options: a.options }}
						bind:value={attrValues[a.definition_id]}
					/>
				{/each}
			</div>

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
			{#if saved}
				<p class="text-sm text-emerald-600">Kaydedildi</p>
			{/if}

			<button type="submit" class="btn-primary w-full" disabled={busy}>
				{busy ? 'Kaydediliyor…' : 'Özellikleri Kaydet'}
			</button>
		</form>
	{:else if tab === 'akis'}
		<WorkflowPanel {wid} itemId={iid} onAssigned={() => void load()} />
	{:else if tab === 'yorumlar'}
		<CommentsPanel {wid} entityType="work_item" entityId={iid} />
	{:else}
		<AttachmentsPanel {wid} entityType="work_item" entityId={iid} />
	{/if}
{/if}
