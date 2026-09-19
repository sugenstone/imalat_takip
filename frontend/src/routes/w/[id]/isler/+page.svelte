<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Section, WorkItem, WorkType, AttributeDefinition, WorkflowTemplate } from '$lib/api/types';
	import { statusLabel, statusBadgeClass, priorityLabel, priorityBadgeClass, WORK_ITEM_STATUSES, WORK_ITEM_PRIORITIES } from '$lib/api/types';
	import { page } from '$app/state';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import AttributeField from '$lib/components/AttributeField.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import SectionPicker from '$lib/components/SectionPicker.svelte';
	import { ws } from '$lib/stores/ws.svelte';

	// my-tasks surec karti (tasks.rs MyTasksOut ile ayni sekil)
	interface TaskRow {
		id: string;
		step_name: string;
		status: string;
		item_id: string;
		item_name: string;
		item_status: string;
		workflow_name: string;
		assignments: string | null;
		approval_note: string | null;
		requested_by_name: string | null;
	}
	interface MyTasks {
		ready: TaskRow[];
		in_progress: TaskRow[];
		approvals: TaskRow[];
	}

	let items = $state<WorkItem[]>([]);
	let sections = $state<Section[]>([]);
	let workTypes = $state<WorkType[]>([]);
	let workflows = $state<WorkflowTemplate[]>([]);
	let loading = $state(true);

	// Ana gorunum: iscinin ekranindan yonetim listesine
	let view = $state<'gorevlerim' | 'onaylar' | 'tumu'>('gorevlerim');
	let tasks = $state<MyTasks>({ ready: [], in_progress: [], approvals: [] });
	let tasksLoading = $state(true);

	// Hizli aksiyon durumu
	let busyTask = $state('');
	let taskError = $state('');

	// Reddet sheet'i
	let rejectTarget = $state<TaskRow | null>(null);
	let rejectNote = $state('');

	// Filtreler (yalniz 'tumu' gorunumunde)
	let fSearch = $state('');
	let fSection = $state('');
	let fSubtree = $state(true);
	let fType = $state('');
	let fStatus = $state('');
	let filterOpen = $state(false);

	// Bolum secici (filtre)
	let pickerOpen = $state(false);
	let pickerValue = $state('');
	let pickerValueName = $state('');

	const wid = $derived(page.params.id ?? '');

	$effect(() => {
		wid;
		void loadMeta();
		void loadTasks();
		void load();
	});

	async function loadMeta() {
		try {
			const [s, t, w] = await Promise.all([
				api.get<Section[]>(`/workspaces/${wid}/sections`),
				api.get<WorkType[]>(`/workspaces/${wid}/work-types`),
				api.get<WorkflowTemplate[]>(`/workspaces/${wid}/workflows`)
			]);
			sections = s;
			workTypes = t;
			workflows = w.filter((x) => x.published_version);
		} catch {
			/* layout hatayi gosterir */
		}
	}

	async function loadTasks() {
		tasksLoading = true;
		try {
			tasks = await api.get<MyTasks>(`/workspaces/${wid}/my-tasks`);
		} catch {
			tasks = { ready: [], in_progress: [], approvals: [] };
		} finally {
			tasksLoading = false;
		}
	}

	async function load() {
		loading = true;
		const p = new URLSearchParams();
		if (view === 'tumu') {
			if (fSearch.trim()) p.set('search', fSearch.trim());
			if (fSection) {
				p.set('section_id', fSection);
				if (fSubtree) p.set('subtree', 'true');
			}
			if (fType) p.set('work_type_id', fType);
			if (fStatus) p.set('status', fStatus);
		}
		try {
			items = await api.get<WorkItem[]>(`/workspaces/${wid}/work-items?${p.toString()}`);
		} catch {
			items = [];
		} finally {
			loading = false;
		}
	}

	function resetFilters() {
		fSearch = '';
		fSection = '';
		pickerValueName = '';
		fType = '';
		fStatus = '';
		void load();
	}

	const activeFilters = $derived(
		(fSearch ? 1 : 0) + (fSection ? 1 : 0) + (fType ? 1 : 0) + (fStatus ? 1 : 0)
	);

	// --- Gorev aksiyonlari: tek dokunus, sayfada kal ---
	async function taskAction(t: TaskRow, action: 'start' | 'submit' | 'approve') {
		busyTask = t.id;
		taskError = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${t.id}/${action}`, {});
			await loadTasks();
		} catch (err) {
			taskError = err instanceof ApiError ? err.message : 'İşlem başarısız';
		} finally {
			busyTask = '';
		}
	}

	async function submitReject(e: SubmitEvent) {
		e.preventDefault();
		if (!rejectTarget || !rejectNote.trim()) return;
		taskError = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${rejectTarget.id}/reject`, {
				note: rejectNote.trim()
			});
			rejectTarget = null;
			rejectNote = '';
			await loadTasks();
		} catch (err) {
			taskError = err instanceof ApiError ? err.message : 'Reddedilemedi';
		}
	}

	function parseAssignments(t: TaskRow): { name: string }[] {
		if (!t.assignments) return [];
		try {
			const arr = JSON.parse(t.assignments);
			return Array.isArray(arr) ? arr : [];
		} catch {
			return [];
		}
	}

	const taskCount = $derived(
		tasks.ready.length + tasks.in_progress.length + tasks.approvals.length
	);

	// Segment gorunurlugu role gore: isci yalniz Gorevlerim (+yetkisi varsa Onaylar) gorur
	const showApprovals = $derived(ws.can('process.approve'));
	const showAll = $derived(ws.can('work_item.create'));
	const segmentCount = $derived(1 + (showApprovals ? 1 : 0) + (showAll ? 1 : 0));

	// Yetki sonrasi gelisse saklanan gorunume dusme
	$effect(() => {
		if (view === 'onaylar' && !showApprovals) view = 'gorevlerim';
		if (view === 'tumu' && !showAll) view = 'gorevlerim';
	});

	// --- Olusturma sheet'leri ---
	let createOpen = $state(false);
	let bulkOpen = $state(false);
	let cSection = $state('');
	let cSectionName = $state('');
	let cType = $state('');
	let cWorkflow = $state('');
	let cName = $state('');
	let cDesc = $state('');
	let cPriority = $state('medium');
	let cStatus = $state('draft');
	let cPlannedStart = $state('');
	let cPlannedEnd = $state('');
	let cAttrs = $state<Record<string, unknown>>({});
	let defs = $state<AttributeDefinition[]>([]);
	let busy = $state(false);
	let error = $state('');

	let cPickerOpen = $state(false);
	let bPickerOpen = $state(false);

	let bTemplate = $state('İş Kalemi {n}');
	let bStart = $state(1);
	let bEnd = $state(5);

	// Hafif toast geri bildirimi (alert yerine)
	let toast = $state('');
	let toastTimer: ReturnType<typeof setTimeout> | undefined;
	function showToast(msg: string) {
		toast = msg;
		clearTimeout(toastTimer);
		toastTimer = setTimeout(() => (toast = ''), 4000);
	}

	// Bolumlere dagitim
	let distOpen = $state(false);
	let dSection = $state('');
	let dSectionName = $state('');
	let dTarget = $state('leaves');
	let dType = $state('');
	let dWorkflow = $state('');
	let dName = $state('{parent} İş Kalemi');
	let dPickerOpen = $state(false);

	async function openCreate(sectionPreset?: string) {
		error = '';
		cSection = sectionPreset ?? '';
		cSectionName = '';
		cType = ''; // varsayilan Tipsiz — kullanici bilincli secsin (otomatik akis riski)
		cWorkflow = '';
		cName = '';
		cDesc = '';
		cPriority = 'medium';
		cStatus = 'draft';
		cPlannedStart = '';
		cPlannedEnd = '';
		cAttrs = {};
		await loadDefs();
		createOpen = true;
	}

	async function loadDefs() {
		if (!cType) {
			defs = [];
			return;
		}
		try {
			defs = await api.get<AttributeDefinition[]>(
				`/workspaces/${wid}/work-types/${cType}/attributes`
			);
			const initial: Record<string, unknown> = {};
			for (const d of defs) {
				if (d.default_value !== null && d.default_value !== undefined && d.default_value !== '') {
					try {
						initial[d.id] = JSON.parse(d.default_value);
					} catch {
						initial[d.id] = d.default_value;
					}
				} else if (d.data_type === 'multiselect') {
					initial[d.id] = [];
				}
			}
			cAttrs = initial;
		} catch {
			defs = [];
		}
	}

	async function submitCreate(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const attributes = defs.map((d) => ({
				definition_id: d.id,
				value: cAttrs[d.id] ?? null
			}));
			await api.post(`/workspaces/${wid}/work-items`, {
				section_id: cSection,
				work_type_id: cType || null,
				workflow_template_id: cWorkflow || null,
				name: cName,
				description: cDesc || null,
				priority: cPriority,
				status: cStatus,
				planned_start: cPlannedStart || null,
				planned_end: cPlannedEnd || null,
				attributes
			});
			createOpen = false;
			await load();
			showToast('İş kalemi oluşturuldu');
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	function openBulk() {
		error = '';
		cSection = sections[0]?.id ?? '';
		cSectionName = '';
		cType = ''; // varsayilan Tipsiz
		bTemplate = 'İş Kalemi {n}';
		bStart = 1;
		bEnd = 5;
		bulkOpen = true;
	}

	async function submitBulk(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const res = await api.post<{ created: number }>(`/workspaces/${wid}/work-items/bulk`, {
				section_id: cSection,
				work_type_id: cType || null,
				template: bTemplate,
				start: bStart,
				end: bEnd
			});
			bulkOpen = false;
			await load();
			showToast(`${res.created} iş kalemi oluşturuldu`);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	function openDistribute() {
		error = '';
		dSection = sections[0]?.id ?? '';
		dSectionName = '';
		dType = ''; // varsayilan Tipsiz — yanlis otomatik akis riskini kaldirir
		dWorkflow = '';
		dName = '{parent} İş Kalemi';
		distOpen = true;
	}

	async function submitDistribute(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const res = await api.post<{ created: number; auto_assigned: number }>(
				`/workspaces/${wid}/work-items/bulk-distribute`,
				{
					parent_section_id: dSection,
					target: dTarget,
					work_type_id: dType || null,
					name: dName,
					workflow_template_id: dWorkflow || null
				}
			);
			distOpen = false;
			await load();
			showToast(
				`${res.created} iş kalemi oluşturuldu${res.auto_assigned > 0 ? ` · ${res.auto_assigned} akış atandı` : ''}`
			);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Dağıtılamadı';
		} finally {
			busy = false;
		}
	}

	// Bolum menusunden "Is Kalemi Ekle" ile gelinebilir (?section=...)
	$effect(() => {
		const sp = page.url.searchParams;
		if (sp.get('yeni-is') === '1') {
			const sid = sp.get('section') ?? '';
			void openCreate(sid);
		}
	});

	// Filtre picker'i kapaninca secilen bolumu uygula
	$effect(() => {
		if (!pickerOpen && pickerValue !== fSection) {
			fSection = pickerValue;
			void load();
		}
	});
</script>

<svelte:head><title>İşler</title></svelte:head>

<!-- Ana segment: Gorevlerim | Onaylar | Tumu (rol bazli) -->
{#if segmentCount > 1}
	<div class="mb-3 grid gap-1 rounded-xl bg-white p-1 shadow-sm ring-1 ring-slate-200 {segmentCount === 2 ? 'grid-cols-2' : 'grid-cols-3'}">
		<button
			type="button"
			class="flex min-h-11 items-center justify-center gap-1.5 rounded-lg px-1 text-xs font-medium transition-colors
				{view === 'gorevlerim' ? 'bg-indigo-600 text-white' : 'text-slate-600 hover:bg-slate-100'}"
			onclick={() => {
				view = 'gorevlerim';
				void loadTasks();
			}}
		>
			<Icon name="zap" size={14} />
			Görevlerim
			{#if tasks.ready.length + tasks.in_progress.length > 0}
				<span class="flex min-w-5 items-center justify-center rounded-full px-1 text-[11px] {view === 'gorevlerim' ? 'bg-white/25' : 'bg-indigo-100 text-indigo-700'}">
					{tasks.ready.length + tasks.in_progress.length}
				</span>
			{/if}
		</button>
		{#if showApprovals}
			<button
				type="button"
				class="flex min-h-11 items-center justify-center gap-1.5 rounded-lg px-1 text-xs font-medium transition-colors
					{view === 'onaylar' ? 'bg-violet-600 text-white' : 'text-slate-600 hover:bg-slate-100'}"
				onclick={() => {
					view = 'onaylar';
					void loadTasks();
				}}
			>
				<Icon name="check-circle" size={14} />
				Onaylar
				{#if tasks.approvals.length > 0}
					<span class="flex min-w-5 items-center justify-center rounded-full px-1 text-[11px] {view === 'onaylar' ? 'bg-white/25' : 'bg-violet-100 text-violet-700'}">
						{tasks.approvals.length}
					</span>
				{/if}
			</button>
		{/if}
		{#if showAll}
			<button
				type="button"
				class="flex min-h-11 items-center justify-center gap-1.5 rounded-lg px-1 text-xs font-medium transition-colors
					{view === 'tumu' ? 'bg-slate-900 text-white' : 'text-slate-600 hover:bg-slate-100'}"
				onclick={() => {
					view = 'tumu';
					void load();
				}}
			>
				<Icon name="clipboard" size={14} />
				Tümü
			</button>
		{/if}
	</div>
{:else}
	<h2 class="mb-3 flex items-center gap-2 text-base font-bold">
		<Icon name="zap" size={18} class="text-indigo-600" /> Görevlerim
	</h2>
{/if}

{#if taskError}
	<p class="form-error" role="alert">{taskError}</p>
{/if}

{#if view === 'gorevlerim' || view === 'onaylar'}
	{#if tasksLoading}
		<div class="flex justify-center py-16">
			<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
		</div>
	{:else if view === 'gorevlerim'}
		<!-- Gorevlerim: devam eden + hazir surec kartlari -->
		{#if tasks.in_progress.length > 0}
			<h3 class="mb-2 flex items-center gap-1.5 text-xs font-semibold tracking-wide text-slate-500 uppercase">
				<Icon name="activity" size={13} class="text-amber-500" />
				Devam Eden ({tasks.in_progress.length})
			</h3>
			<div class="mb-5 space-y-2">
				{#each tasks.in_progress as t (t.id)}
					{@const assigns = parseAssignments(t)}
					<div class="rounded-2xl border border-amber-200 bg-white p-3 shadow-sm">
						<div class="flex items-start justify-between gap-2">
							<a href={`/w/${wid}/isler/${t.item_id}`} class="min-w-0 flex-1">
								<p class="truncate text-sm font-bold">{t.step_name}</p>
								<p class="truncate text-xs text-slate-500">{t.item_name} · {t.workflow_name}</p>
								{#if assigns.length > 0}
									<div class="mt-1.5 flex flex-wrap gap-1">
										{#each assigns as a, i (i)}
											<span class="badge bg-indigo-50 text-indigo-700">{a.name}</span>
										{/each}
									</div>
								{/if}
							</a>
							<button
								type="button"
								class="btn-primary shrink-0 !min-h-10 !px-4 !text-xs !bg-emerald-600 hover:!bg-emerald-700"
								disabled={busyTask === t.id}
								onclick={() => taskAction(t, 'submit')}
							>
								<Icon name="check" size={14} />
								{busyTask === t.id ? '…' : 'Tamamla'}
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}

		{#if tasks.ready.length > 0}
			<h3 class="mb-2 flex items-center gap-1.5 text-xs font-semibold tracking-wide text-slate-500 uppercase">
				<Icon name="zap" size={13} class="text-blue-500" />
				Hazır ({tasks.ready.length})
			</h3>
			<div class="space-y-2">
				{#each tasks.ready as t (t.id)}
					{@const assigns = parseAssignments(t)}
					<div class="rounded-2xl border border-blue-200 bg-white p-3 shadow-sm">
						<div class="flex items-start justify-between gap-2">
							<a href={`/w/${wid}/isler/${t.item_id}`} class="min-w-0 flex-1">
								<p class="truncate text-sm font-bold">{t.step_name}</p>
								<p class="truncate text-xs text-slate-500">{t.item_name} · {t.workflow_name}</p>
								{#if assigns.length > 0}
									<div class="mt-1.5 flex flex-wrap gap-1">
										{#each assigns as a, i (i)}
											<span class="badge bg-indigo-50 text-indigo-700">{a.name}</span>
										{/each}
									</div>
								{/if}
							</a>
							<button
								type="button"
								class="btn-primary shrink-0 !min-h-10 !px-4 !text-xs"
								disabled={busyTask === t.id}
								onclick={() => taskAction(t, 'start')}
							>
								<Icon name="play" size={14} />
								{busyTask === t.id ? '…' : 'Başlat'}
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}

		{#if tasks.ready.length === 0 && tasks.in_progress.length === 0}
			<div class="card">
				<EmptyState
					icon="inbox"
					title="Şu an göreviniz yok"
					description="Size atanan işler burada görünür. Yönetici size görev verdiğinde listelenir."
				/>
			</div>
		{/if}
	{:else}
		<!-- Onaylar -->
		{#if tasks.approvals.length === 0}
			<div class="card">
				<EmptyState
					icon="check-circle"
					title="Onay bekleyen iş yok"
					description="Onaylamanız gereken işler burada görünür."
				/>
			</div>
		{:else}
			<div class="space-y-2">
				{#each tasks.approvals as t (t.id)}
					<div class="rounded-2xl border border-violet-200 bg-white p-3 shadow-sm">
						<a href={`/w/${wid}/isler/${t.item_id}`} class="block">
							<p class="truncate text-sm font-bold">{t.step_name}</p>
							<p class="truncate text-xs text-slate-500">{t.item_name} · {t.workflow_name}</p>
							{#if t.requested_by_name}
								<p class="mt-1 truncate text-xs text-slate-400">
									{t.requested_by_name} gönderdi{#if t.approval_note} — “{t.approval_note}”{/if}
								</p>
							{/if}
						</a>
						<div class="mt-2.5 flex gap-2">
							<button
								type="button"
								class="btn-primary flex-1 !min-h-10 !text-xs !bg-emerald-600 hover:!bg-emerald-700"
								disabled={busyTask === t.id}
								onclick={() => taskAction(t, 'approve')}
							>
								<Icon name="check" size={14} />
								{busyTask === t.id ? '…' : 'Onayla'}
							</button>
							<button
								type="button"
								class="btn-danger !min-h-10 !px-4 !text-xs"
								onclick={() => {
									rejectTarget = t;
									rejectNote = '';
								}}
							>
								Reddet
							</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
{:else if loading}
	<div class="flex justify-center py-16">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else}
	<!-- Tumu: klasik is kalemi listesi -->
	<div class="flex items-center justify-between pb-2">
		<h2 class="font-semibold">İş Kalemleri ({items.length})</h2>
		<div class="flex gap-2">
			<button
				type="button"
				class="btn-secondary !min-h-9 !px-3 !text-xs"
				onclick={() => (filterOpen = !filterOpen)}
			>
				<Icon name="filter" size={13} />
				Filtre{activeFilters > 0 ? ` (${activeFilters})` : ''}
			</button>
			<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openBulk}>Seri</button>
			<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openDistribute}>Dağıt</button>
		</div>
	</div>

	{#if filterOpen}
		<div class="card mb-3 space-y-3">
			<div>
				<label class="label" for="f-search">Ara</label>
				<input
					id="f-search"
					class="input"
					type="search"
					placeholder="İsim veya açıklama…"
					bind:value={fSearch}
					onchange={load}
				/>
			</div>
			<div>
				<span class="label">Bölüm</span>
				<button
					type="button"
					class="input flex items-center justify-between text-left"
					onclick={() => {
						pickerValue = fSection;
						pickerOpen = true;
					}}
				>
					<span class={fSection ? '' : 'text-slate-400'}>
						{fSection ? pickerValueName || 'Seçili bölüm' : 'Tüm bölümler'}
					</span>
					<Icon name="chevron-right" size={15} class="shrink-0 text-slate-400" />
				</button>
			</div>
			<div class="grid grid-cols-2 gap-3">
				<div>
					<label class="label" for="f-type">İş Tipi</label>
					<select id="f-type" class="input" bind:value={fType} onchange={load}>
						<option value="">Tümü</option>
						{#each workTypes as t (t.id)}
							<option value={t.id}>{t.name}</option>
						{/each}
					</select>
				</div>
				<div>
					<label class="label" for="f-status">Durum</label>
					<select id="f-status" class="input" bind:value={fStatus} onchange={load}>
						<option value="">Tümü</option>
						{#each WORK_ITEM_STATUSES as s (s.value)}
							<option value={s.value}>{s.label}</option>
						{/each}
					</select>
				</div>
			</div>
			<label class="flex min-h-11 items-center gap-2 text-sm text-slate-600">
				<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={fSubtree} onchange={load} />
				Alt bölümler dahil
			</label>
			<button type="button" class="text-sm text-indigo-600" onclick={resetFilters}>Filtreleri temizle</button>
		</div>
	{/if}

	{#if items.length === 0}
		<div class="card">
			<EmptyState
				icon="clipboard"
				title="İş kalemi yok"
				description="Bölümlerinize iş kalemleri ekleyin: montaj, boya, kontrol, sevkiyat…"
				actionLabel="İş Kalemi Ekle"
				onaction={() => openCreate()}
			/>
		</div>
	{:else}
		<div class="card divide-y divide-slate-100 !p-0">
			{#each items as it (it.id)}
				<a
					href={`/w/${wid}/isler/${it.id}`}
					class="flex items-center justify-between gap-3 px-4 py-3 hover:bg-slate-50"
				>
					<div class="min-w-0 flex-1">
						<div class="flex flex-wrap items-center gap-1.5">
							<span class="truncate font-medium">{it.name}</span>
							<span class="badge {statusBadgeClass(it.status)}">{statusLabel(it.status)}</span>
							{#if it.priority !== 'medium'}
								<span class="badge {priorityBadgeClass(it.priority)}">{priorityLabel(it.priority)}</span>
							{/if}
						</div>
						<p class="mt-0.5 truncate text-xs text-slate-500">
							{it.work_type_name ?? 'Tipsiz'} · {it.section_name}
						</p>
					</div>
					<Icon name="chevron-right" size={16} class="shrink-0 text-slate-400" />
				</a>
			{/each}
		</div>
	{/if}

	<button
		type="button"
		class="btn-primary fixed inset-x-4 bottom-24 z-30 shadow-lg md:static md:inset-auto md:mt-4 md:w-auto"
		onclick={() => openCreate()}
	>
		+ Yeni İş Kalemi
	</button>
{/if}

<!-- Reddet sheet'i (sebep zorunlu) -->
<Sheet open={rejectTarget !== null} title={rejectTarget ? `Reddet — ${rejectTarget.step_name}` : ''} onclose={() => (rejectTarget = null)}>
	{#if rejectTarget}
		<form class="space-y-4" onsubmit={submitReject}>
			<div>
				<label class="label" for="task-reject-note">Red Sebebi (zorunlu)</label>
				<textarea
					id="task-reject-note"
					class="input min-h-24"
					rows="3"
					bind:value={rejectNote}
					required
					placeholder="Örn. ölçüler hatalı, yeniden yapılmalı"
				></textarea>
			</div>
			{#if taskError}
				<p class="form-error" role="alert">{taskError}</p>
			{/if}
			<div class="flex gap-2">
				<button type="button" class="btn-secondary flex-1" onclick={() => (rejectTarget = null)}>İptal</button>
				<button type="submit" class="btn-danger flex-1" disabled={!rejectNote.trim()}>Reddet</button>
			</div>
		</form>
	{/if}
</Sheet>

<!-- Tek olusturma -->
<Sheet open={createOpen} title="Yeni İş Kalemi" onclose={() => (createOpen = false)}>
	<form class="space-y-4" onsubmit={submitCreate}>
		<div>
			<span class="label">Bölüm</span>
			<button
				type="button"
				class="input flex items-center justify-between text-left"
				onclick={() => (cPickerOpen = true)}
			>
				<span class={cSection ? '' : 'text-slate-400'}>
					{cSection ? cSectionName || 'Seçili bölüm' : '— Seçin —'}
				</span>
				<Icon name="chevron-right" size={15} class="shrink-0 text-slate-400" />
			</button>
		</div>
		<div>
			<label class="label" for="c-type">İş Tipi</label>
			<select id="c-type" class="input" bind:value={cType} onchange={() => void loadDefs()}>
				<option value="">Tipsiz</option>
				{#each workTypes as t (t.id)}
					<option value={t.id}>{t.name}</option>
				{/each}
			</select>
		</div>
		<div>
			<label class="label" for="c-wf">Süreç Grubu (opsiyonel)</label>
			<select id="c-wf" class="input" bind:value={cWorkflow}>
				<option value="">Akış olmadan oluştur</option>
				{#each workflows as g (g.id)}
					<option value={g.id}>{g.name} · {g.published_node_count} adım</option>
				{/each}
			</select>
			{#if cWorkflow}
				<p class="mt-1.5 text-xs text-indigo-600">Grubun adımları ve sorumluları otomatik eklenecek.</p>
			{/if}
		</div>
		<div>
			<label class="label" for="c-name">Ad</label>
			<input id="c-name" class="input" bind:value={cName} required maxlength={120} placeholder="Örn. Tezgah 1" />
		</div>

		{#if defs.length > 0}
			<div class="rounded-xl bg-slate-50 p-3">
				<p class="mb-3 text-xs font-semibold tracking-wide text-slate-500 uppercase">Özellikler</p>
				<div class="space-y-3">
					{#each defs as d (d.id)}
						<AttributeField def={d} bind:value={cAttrs[d.id]} />
					{/each}
				</div>
			</div>
		{/if}

		<div class="grid grid-cols-2 gap-3">
			<div>
				<label class="label" for="c-priority">Öncelik</label>
				<select id="c-priority" class="input" bind:value={cPriority}>
					{#each WORK_ITEM_PRIORITIES as p (p.value)}
						<option value={p.value}>{p.label}</option>
					{/each}
				</select>
			</div>
			<div>
				<label class="label" for="c-status">Durum</label>
				<select id="c-status" class="input" bind:value={cStatus}>
					{#each WORK_ITEM_STATUSES as s (s.value)}
						<option value={s.value}>{s.label}</option>
					{/each}
				</select>
			</div>
		</div>
		<div class="grid grid-cols-2 gap-3">
			<div>
				<label class="label" for="c-ps">Planlanan Başlangıç</label>
				<input id="c-ps" class="input" type="date" bind:value={cPlannedStart} />
			</div>
			<div>
				<label class="label" for="c-pe">Planlanan Bitiş</label>
				<input id="c-pe" class="input" type="date" bind:value={cPlannedEnd} />
			</div>
		</div>
		<div>
			<label class="label" for="c-desc">Açıklama</label>
			<textarea id="c-desc" class="input min-h-20" rows="2" bind:value={cDesc}></textarea>
		</div>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}

		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (createOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !cSection || !cName.trim()}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>

<!-- Seri olusturma -->
<Sheet open={bulkOpen} title="Seri İş Kalemi Oluştur" onclose={() => (bulkOpen = false)}>
	<form class="space-y-4" onsubmit={submitBulk}>
		<div>
			<span class="label">Bölüm</span>
			<button
				type="button"
				class="input flex items-center justify-between text-left"
				onclick={() => (bPickerOpen = true)}
			>
				<span class={cSection ? '' : 'text-slate-400'}>
					{cSection ? cSectionName || 'Seçili bölüm' : '— Seçin —'}
				</span>
				<Icon name="chevron-right" size={15} class="shrink-0 text-slate-400" />
			</button>
		</div>
		<div>
			<label class="label" for="b-type">İş Tipi</label>
			<select id="b-type" class="input" bind:value={cType}>
				<option value="">Tipsiz</option>
				{#each workTypes as t (t.id)}
					<option value={t.id}>{t.name}</option>
				{/each}
			</select>
			<p class="mt-1.5 text-xs text-slate-500">Özellikler tip tanımındaki varsayılan değerlerle doldurulur.</p>
		</div>
		<div>
			<label class="label" for="b-template">İsim Şablonu</label>
			<input id="b-template" class="input" bind:value={bTemplate} placeholder={'İş Kalemi {n}'} required />
		</div>
		<div class="grid grid-cols-2 gap-3">
			<div>
				<label class="label" for="b-start">Başlangıç</label>
				<input id="b-start" class="input" type="number" bind:value={bStart} min="0" required />
			</div>
			<div>
				<label class="label" for="b-end">Bitiş</label>
				<input id="b-end" class="input" type="number" bind:value={bEnd} min={bStart} required />
			</div>
		</div>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}

		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (bulkOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !cSection}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>

<!-- Bolumlere dagit sheet'i -->
<Sheet open={distOpen} title="Bölümlere Dağıt" onclose={() => (distOpen = false)}>
	<form class="space-y-4" onsubmit={submitDistribute}>
		<p class="rounded-lg bg-indigo-50 px-3 py-2 text-xs text-indigo-800">
			Seçilen bölümün altındaki hedef bölümlere (ör. her daireye) birer iş kalemi oluşturur.
			Süreç grubu seçerseniz adımlar ve sorumlular otomatik eklenir.
		</p>
		<div>
			<span class="label">Kök Bölüm</span>
			<button
				type="button"
				class="input flex items-center justify-between text-left"
				onclick={() => (dPickerOpen = true)}
			>
				<span class={dSection ? '' : 'text-slate-400'}>
					{dSection ? dSectionName || 'Seçili bölüm' : '— Seçin —'}
				</span>
				<Icon name="chevron-right" size={15} class="shrink-0 text-slate-400" />
			</button>
		</div>
		<div>
			<label class="label" for="d-target">Hedef</label>
			<select id="d-target" class="input" bind:value={dTarget}>
				<option value="leaves">En alt bölümler (yapraklar — daireler)</option>
				<option value="children">Doğrudan alt bölümler</option>
			</select>
		</div>
		<div>
			<label class="label" for="d-wf">Süreç Grubu</label>
			<select id="d-wf" class="input" bind:value={dWorkflow}>
				<option value="">Akış olmadan dağıt</option>
				{#each workflows as g (g.id)}
					<option value={g.id}>{g.name} · {g.published_node_count} adım</option>
				{/each}
			</select>
			{#if dWorkflow}
				<p class="mt-1.5 text-xs text-indigo-600">Her iş kalemi grubun adımları ve sorumlularıyla oluşturulacak.</p>
			{/if}
		</div>
		<div>
			<label class="label" for="d-type">İş Tipi (opsiyonel)</label>
			<select id="d-type" class="input" bind:value={dType}>
				<option value="">Tipsiz</option>
				{#each workTypes as t (t.id)}
					<option value={t.id}>{t.name}</option>
				{/each}
			</select>
		</div>
		<div>
			<label class="label" for="d-name">İsim</label>
			<input id="d-name" class="input" bind:value={dName} required maxlength={160} />
			<p class="mt-1.5 text-xs text-slate-500">
				<code class="rounded bg-slate-100 px-1">{'{parent}'}</code> hedef bölüm adı,
				<code class="rounded bg-slate-100 px-1">{'{n}'}</code> sıra numarası olur.
			</p>
		</div>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (distOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !dSection || !dName.trim()}>
				{busy ? 'Dağıtılıyor…' : 'Dağıt'}
			</button>
		</div>
	</form>
</Sheet>

<!-- Toast geri bildirimi -->
{#if toast}
	<div class="fixed inset-x-4 bottom-24 z-50 mx-auto max-w-sm md:bottom-8" role="status">
		<div class="flex items-center gap-2.5 rounded-xl bg-slate-900 px-4 py-3 text-sm font-medium text-white shadow-lg">
			<Icon name="check-circle" size={17} class="shrink-0 text-emerald-400" />
			<span class="min-w-0 flex-1">{toast}</span>
			<button type="button" class="shrink-0 text-slate-400 hover:text-white" onclick={() => (toast = '')} aria-label="Kapat">
				<Icon name="x" size={15} />
			</button>
		</div>
	</div>
{/if}

<!-- Bolum seciciler -->
<SectionPicker bind:open={cPickerOpen} title="Bölüm Seç" {wid} bind:value={cSection} bind:valueName={cSectionName} />
<SectionPicker bind:open={bPickerOpen} title="Seri — Bölüm Seç" {wid} bind:value={cSection} bind:valueName={cSectionName} />
<SectionPicker bind:open={dPickerOpen} title="Kök Bölüm Seç" {wid} bind:value={dSection} bind:valueName={dSectionName} />
<SectionPicker
	bind:open={pickerOpen}
	title="Filtre — Bölüm"
	{wid}
	bind:value={pickerValue}
	bind:valueName={pickerValueName}
	allowRoot={true}
	rootLabel="Tüm bölümler"
/>
