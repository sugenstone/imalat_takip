<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { WorkflowInstance, WorkflowTemplate, Member, Team, ProcessInstance } from '$lib/api/types';
	import { processStatus, parseAssignments, parseApproval } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import CommentsPanel from '$lib/components/CommentsPanel.svelte';
	import { ws } from '$lib/stores/ws.svelte';

	let {
		wid,
		itemId,
		onAssigned
	}: {
		wid: string;
		itemId: string;
		onAssigned?: () => void;
	} = $props();

	// Rol bazli aksiyon gorunurlugu: isci yalniz saha aksiyonlari gorur
	const canRework = $derived(ws.can('process.rework'));
	const canAssignFlow = $derived(ws.can('workflow.assign'));
	const canQuickFlow = $derived(ws.can('workflow.create'));
	const canAssignProcess = $derived(ws.can('process.assign'));

	let instance = $state<WorkflowInstance | null>(null);
	let templates = $state<WorkflowTemplate[]>([]);
	let members = $state<Member[]>([]);
	let teams = $state<Team[]>([]);
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');

	// Atama sheet'i
	let assignSheetOpen = $state(false);
	let assignTarget = $state<ProcessInstance | null>(null);
	let assignTab = $state<'uyeler' | 'takimlar'>('uyeler');

	// Red sheet'i
	let rejectSheetOpen = $state(false);
	let rejectTarget = $state<ProcessInstance | null>(null);
	let rejectNote = $state('');

	// Hizli akis olusturma (quick-flow)
	let quickOpen = $state(false);
	let qfName = $state('');
	let qfSteps = $state<{ name: string; approval: boolean }[]>([]);
	let qfInput = $state('');

	// Surec yorumlari (Faz 8)
	let commentsSheetOpen = $state(false);
	let commentsTarget = $state<ProcessInstance | null>(null);

	// Rework sheet'i (Faz 7)
	let reworkSheetOpen = $state(false);
	let reworkRestart = $state('');
	let reworkReason = $state('');

	$effect(() => {
		itemId;
		void load();
	});

	async function load() {
		loading = true;
		error = '';
		try {
			const [inst, tpls, m, t] = await Promise.all([
				api.get<WorkflowInstance | null>(`/workspaces/${wid}/work-items/${itemId}/workflow`),
				api.get<WorkflowTemplate[]>(`/workspaces/${wid}/workflows`),
				api.get<Member[]>(`/workspaces/${wid}/members`),
				api.get<Team[]>(`/workspaces/${wid}/teams`)
			]);
			instance = inst;
			templates = tpls.filter((t2) => t2.published_version);
			members = m;
			teams = t;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yüklenemedi';
		} finally {
			loading = false;
		}
	}

	async function assign(t: WorkflowTemplate) {
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/work-items/${itemId}/workflow`, {
				template_id: t.id
			});
			onAssigned?.();
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Atanamadı';
		} finally {
			busy = false;
		}
	}

	async function start(pid: string) {
		error = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${pid}/start`, {});
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Başlatılamadı';
		}
	}

	async function submit(pid: string) {
		error = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${pid}/submit`, {});
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Tamamlanamadı';
		}
	}

	async function approve(pid: string) {
		error = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${pid}/approve`, {});
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Onaylanamadı';
		}
	}

	async function reject(e: SubmitEvent) {
		e.preventDefault();
		if (!rejectTarget || !rejectNote.trim()) return;
		error = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${rejectTarget.id}/reject`, {
				note: rejectNote.trim()
			});
			rejectSheetOpen = false;
			rejectNote = '';
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Reddedilemedi';
		}
	}

	// Rework (Faz 7)
	function openRework() {
		error = '';
		reworkRestart = '';
		reworkReason = '';
		reworkSheetOpen = true;
	}

	async function submitRework(e: SubmitEvent) {
		e.preventDefault();
		if (!reworkRestart || !reworkReason.trim()) return;
		error = '';
		try {
			await api.post(`/workspaces/${wid}/work-items/${itemId}/rework`, {
				restart_from_process_id: reworkRestart,
				reason: reworkReason.trim()
			});
			reworkSheetOpen = false;
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Rework başlatılamadı';
		}
	}

	async function cancelFlow() {
		if (!confirm('Akış iptal edilsin mi? Tamamlanmış adımlar geçmişte korunur.')) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/work-items/${itemId}/workflow`);
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'İptal edilemedi';
		}
	}

	// --- Atama islemleri ---
	function openAssign(p: ProcessInstance) {
		error = '';
		assignTarget = p;
		assignTab = 'uyeler';
		assignSheetOpen = true;
	}

	async function addAssignment(type: 'user' | 'team', id: string) {
		if (!assignTarget) return;
		error = '';
		try {
			await api.post(`/workspaces/${wid}/process-instances/${assignTarget.id}/assignments`, {
				assignee_type: type,
				assignee_id: id
			});
			await load();
			// assignTarget'i guncel surecle tazele
			const fresh = instance?.processes.find((p) => p.id === assignTarget!.id);
			if (fresh) assignTarget = fresh;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Atanamadı';
		}
	}

	async function removeAssignment(aid: string) {
		if (!assignTarget) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/process-instances/${assignTarget.id}/assignments/${aid}`);
			await load();
			const fresh = instance?.processes.find((p) => p.id === assignTarget!.id);
			if (fresh) assignTarget = fresh;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaldırılamadı';
		}
	}

	// --- Hizli akis ---
	function openQuick() {
		error = '';
		qfName = '';
		qfSteps = [];
		qfInput = '';
		quickOpen = true;
	}

	function qfAdd() {
		const n = qfInput.trim();
		if (!n || qfSteps.length >= 20) return;
		qfSteps = [...qfSteps, { name: n, approval: false }];
		qfInput = '';
	}

	function qfRemove(i: number) {
		qfSteps = qfSteps.filter((_, idx) => idx !== i);
	}

	function qfMoveUp(i: number) {
		if (i === 0) return;
		const next = [...qfSteps];
		[next[i - 1], next[i]] = [next[i], next[i - 1]];
		qfSteps = next;
	}

	function qfToggleApproval(i: number) {
		const next = [...qfSteps];
		next[i] = { ...next[i], approval: !next[i].approval };
		qfSteps = next;
	}

	async function submitQuick(e: SubmitEvent) {
		e.preventDefault();
		if (!qfName.trim() || qfSteps.length === 0) return;
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/quick-flow`, {
				name: qfName.trim(),
				steps: qfSteps.map((s2) => ({
					name: s2.name,
					requires_approval: s2.approval
				})),
				assign_to_item_id: itemId
			});
			quickOpen = false;
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	function predNames(p: ProcessInstance): string[] {
		if (!instance || !p.predecessor_ids) return [];
		try {
			const ids: string[] = JSON.parse(p.predecessor_ids);
			return ids
				.map((id) => instance!.processes.find((x) => x.node_id === id)?.name)
				.filter((n): n is string => Boolean(n));
		} catch {
			return [];
		}
	}

	const progress = $derived(
		instance && instance.total > 0 ? Math.round((instance.approved / instance.total) * 100) : 0
	);

	const hasFailed = $derived(
		instance?.processes.some((p) => p.status === 'failed') ?? false
	);
</script>

{#if loading}
	<div class="flex justify-center py-12">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if instance}
	<!-- Aktif atama -->
	<div class="card mb-3">
		<div class="flex items-center justify-between gap-2">
			<div class="min-w-0">
				<p class="truncate font-semibold">{instance.template_name}</p>
				<p class="text-xs text-slate-500">
					v{instance.version_number}
					{#if instance.status === 'active'}
						· {instance.approved}/{instance.total} adım tamamlandı
					{:else if instance.status === 'completed'}
						· Tamamlandı
					{:else}
						· iptal edildi
					{/if}
					{#if instance.rework_cycles.length > 0}
						· {instance.rework_cycles.length} rework
					{/if}
				</p>
			</div>
			<div class="flex shrink-0 items-center gap-2">
				{#if hasFailed && canRework}
					<button
						type="button"
						class="rounded-lg bg-amber-50 px-2.5 py-1.5 text-xs font-semibold text-amber-700 hover:bg-amber-100"
						onclick={openRework}
					>
						Rework
					</button>
				{/if}
				{#if instance.status === 'active' && canAssignFlow}
					<button type="button" class="text-xs font-medium text-red-600" onclick={cancelFlow}>
						İptal
					</button>
				{/if}
			</div>
		</div>
		<div class="mt-3 h-2 overflow-hidden rounded-full bg-slate-100">
			<div
				class="h-full rounded-full transition-all {instance.status === 'cancelled' ? 'bg-slate-300' : 'bg-emerald-500'}"
				style="width: {progress}%"
			></div>
		</div>
	</div>

	<!-- Surec kartlari -->
	<div class="space-y-2">
		{#each instance.processes as p, i (p.id)}
			{@const assigns = parseAssignments(p)}
			{@const approval = parseApproval(p)}
			<div
				class="rounded-xl border bg-white p-3 shadow-sm
				{p.status === 'ready' ? 'border-blue-200' : p.status === 'in_progress' ? 'border-amber-200' : p.status === 'submitted' ? 'border-violet-200' : p.status === 'failed' ? 'border-red-200' : 'border-slate-200'}"
			>
				<div class="flex items-center gap-2">
					<span
						class="flex size-7 shrink-0 items-center justify-center rounded-full text-xs font-bold
						{p.status === 'approved' ? 'bg-emerald-100 text-emerald-700' : p.status === 'failed' ? 'bg-red-100 text-red-700' : 'bg-indigo-50 text-indigo-700'}"
					>
						{#if p.status === 'approved'}<Icon name="check" size={13} />{:else if p.status === 'failed'}<Icon name="x" size={13} />{:else}{i + 1}{/if}
					</span>
					<p class="min-w-0 flex-1 truncate text-sm font-medium">{p.name}</p>
					<button
						type="button"
						class="flex size-9 shrink-0 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-100"
						onclick={() => { commentsTarget = p; commentsSheetOpen = true; }}
						aria-label="Yorumlar"
					>
						<Icon name="message" size={16} />
					</button>
					{#if p.attempt_count > 1}
						<span class="badge shrink-0 bg-amber-50 text-amber-700" title="Rework ile yeniden yapılan deneme">
							Deneme {p.attempt_count}
						</span>
					{/if}
					<span class="badge shrink-0 {processStatus(p.status).badge}">
						{processStatus(p.status).label}
					</span>
				</div>

				{#if predNames(p).length > 0}
					<p class="mt-1.5 pl-9 text-xs text-slate-400">Önce: {predNames(p).join(', ')}</p>
				{/if}

				<!-- Atamalar -->
				<div class="mt-2 flex flex-wrap items-center gap-1.5 pl-9">
					{#each assigns as a (a.id)}
						<span class="badge {a.type === 'user' ? 'bg-indigo-50 text-indigo-700' : 'bg-teal-50 text-teal-700'}">
							{a.name}
						</span>
					{/each}
					{#if instance.status === 'active' && p.status !== 'approved' && p.status !== 'cancelled' && canAssignProcess}
						<button type="button" class="text-xs font-medium text-indigo-600" onclick={() => openAssign(p)}>
							{assigns.length === 0 ? '+ ata' : 'düzenle'}
						</button>
					{/if}
				</div>

				<!-- Onay bilgisi -->
				{#if approval}
					<div class="mt-2 ml-9 rounded-lg px-2 py-1.5 text-xs
						{approval.decision === 'pending' ? 'bg-violet-50 text-violet-700' : approval.decision === 'approved' ? 'bg-emerald-50 text-emerald-700' : 'bg-red-50 text-red-700'}">
						{#if approval.decision === 'pending'}
							{approval.requested_by_name ?? '?'} onay için gönderdi
						{:else}
							{approval.decided_by_name ?? '?'} tarafından
							{approval.decision === 'approved' ? 'onaylandı' : 'reddedildi'}
						{/if}
						{#if approval.note}— “{approval.note}”{/if}
					</div>
				{/if}

				<!-- Aksiyonlar -->
				{#if instance.status === 'active'}
					{#if p.status === 'ready'}
						<button type="button" class="btn-primary mt-2 ml-9 w-auto !min-h-9 !px-4 !text-xs" onclick={() => start(p.id)}>
							Başlat
						</button>
					{:else if p.status === 'in_progress'}
						<button type="button" class="btn-primary mt-2 ml-9 w-auto !min-h-9 !px-4 !text-xs !bg-emerald-600 hover:!bg-emerald-700" onclick={() => submit(p.id)}>
							Tamamla
						</button>
					{:else if p.status === 'submitted'}
						<div class="mt-2 ml-9 flex gap-2">
							<button type="button" class="btn-primary !min-h-9 !px-4 !text-xs !bg-emerald-600 hover:!bg-emerald-700"
								onclick={() => approve(p.id)}>
								Onayla
							</button>
							<button type="button" class="btn-danger !min-h-9 !px-4 !text-xs"
								onclick={() => { rejectTarget = p; rejectNote = ''; rejectSheetOpen = true; }}>
								Reddet
							</button>
						</div>
					{/if}
				{/if}
			</div>
		{/each}
	</div>

	{#if instance.status !== 'active'}
		<p class="mt-4 text-center text-xs text-slate-500">
			Bu akış {instance.status === 'completed' ? 'tamamlandı' : 'iptal edildi'} — geçmiş korunur.
		</p>
	{/if}

	<!-- Rework gecmisi (Faz 7) -->
	{#if instance.rework_cycles.length > 0}
		<div class="card mt-4">
			<h3 class="mb-2 text-sm font-semibold">Rework Geçmişi</h3>
			<ul class="space-y-2">
				{#each instance.rework_cycles as rc (rc.id)}
					<li class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
						<p>
							<strong>{rc.restart_from_name}</strong> aşamasından yeniden başlatıldı
							<span class="text-amber-600">({rc.triggered_by_name} başarısız olmuştu)</span>
						</p>
						<p class="mt-0.5">“{rc.reason}” — {rc.created_by_name}</p>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
{:else if !canAssignFlow && !canQuickFlow}
	<!-- Isci: akis atanmamis — yonetim aksiyonu yok, sade bilgi -->
	<div class="card text-center">
		<p class="text-sm text-slate-600">Bu işe henüz iş sırası atanmamış.</p>
		<p class="mt-1 text-xs text-slate-400">Yönetici atadığında adımlar burada görünür.</p>
	</div>
{:else}
	<!-- Atama bekleniyor -->
	<div class="card">
		{#if canQuickFlow}
			<button
				type="button"
				class="flex w-full items-center gap-3 rounded-xl bg-indigo-600 px-4 py-4 text-left text-white shadow-sm transition-all hover:bg-indigo-700 active:bg-indigo-800"
				onclick={openQuick}
			>
				<div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-white/15">
					<Icon name="zap" size={20} />
				</div>
				<div class="min-w-0 flex-1">
					<p class="text-sm font-bold">Hızlı Akış Oluştur</p>
					<p class="text-xs text-indigo-200">Adımları yaz, tek hamlede oluştur ve bu iş kalemine ata</p>
				</div>
				<Icon name="chevron-right" size={18} class="shrink-0 text-indigo-200" />
			</button>
		{/if}

		{#if canAssignFlow && templates.length > 0}
			<div class="my-4 flex items-center gap-3">
				<div class="h-px flex-1 bg-slate-200"></div>
				<span class="text-xs text-slate-400">ya da mevcut akıştan seç</span>
				<div class="h-px flex-1 bg-slate-200"></div>
			</div>
		{/if}
		<p class="text-sm text-slate-600">Bu iş kalemine akış atanmamış.</p>
		{#if canAssignFlow && templates.length === 0}
			<p class="mt-2 text-sm text-slate-400">
				Yayınlanmış akış yok. Önce <strong>Ayarlar › Akışlar</strong>'dan bir akış oluşturup yayınlayın.
			</p>
		{:else if canAssignFlow}
			<div class="mt-3 divide-y divide-slate-100">
				{#each templates as t (t.id)}
					<button
						type="button"
						class="flex w-full items-center justify-between gap-2 py-3 text-left disabled:opacity-50"
						onclick={() => assign(t)}
						disabled={busy}
					>
						<div class="min-w-0">
							<p class="truncate text-sm font-medium">{t.name}</p>
							<p class="text-xs text-slate-500">v{t.published_version} · {t.published_node_count} adım</p>
						</div>
						<span class="shrink-0 text-sm font-medium text-indigo-600">Ata</span>
					</button>
				{/each}
			</div>
		{/if}
	</div>
{/if}

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}

<!-- Atama sheet'i -->
<Sheet open={assignSheetOpen} title={assignTarget ? `Atamalar — ${assignTarget.name}` : ''} onclose={() => (assignSheetOpen = false)}>
	{#if assignTarget}
		{@const current = parseAssignments(assignTarget)}
		<div class="space-y-3">
			{#if current.length > 0}
				<div class="flex flex-wrap gap-1.5">
					{#each current as a (a.id)}
						<button type="button" class="badge min-h-9 px-3 {a.type === 'user' ? 'bg-indigo-50 text-indigo-700' : 'bg-teal-50 text-teal-700'}"
							onclick={() => removeAssignment(a.id)} title="Kaldır">
							{a.name}
						</button>
					{/each}
				</div>
			{/if}

			<div class="grid grid-cols-2 gap-1 rounded-xl bg-slate-100 p-1">
				<button type="button" class="flex min-h-10 items-center justify-center rounded-lg text-sm font-medium
					{assignTab === 'uyeler' ? 'bg-white shadow-sm' : 'text-slate-500'}"
					onclick={() => (assignTab = 'uyeler')}>Üyeler</button>
				<button type="button" class="flex min-h-10 items-center justify-center rounded-lg text-sm font-medium
					{assignTab === 'takimlar' ? 'bg-white shadow-sm' : 'text-slate-500'}"
					onclick={() => (assignTab = 'takimlar')}>Takımlar</button>
			</div>

			{#if assignTab === 'uyeler'}
				<div class="divide-y divide-slate-100 rounded-xl border border-slate-200">
					{#each members as m (m.user_id)}
						<button type="button" class="flex min-h-11 w-full items-center justify-between px-3 text-sm hover:bg-slate-50"
							onclick={() => addAssignment('user', m.user_id)}>
							<span class="truncate">{m.name}</span>
							<span class="text-xs text-slate-400">{m.role_name}</span>
						</button>
					{/each}
				</div>
			{:else}
				<div class="divide-y divide-slate-100 rounded-xl border border-slate-200">
					{#each teams as t (t.id)}
						<button type="button" class="flex min-h-11 w-full items-center justify-between px-3 text-sm hover:bg-slate-50"
							onclick={() => addAssignment('team', t.id)}>
							<span class="truncate">{t.name}</span>
							<span class="text-xs text-slate-400">{t.member_count} üye</span>
						</button>
					{:else}
						<p class="p-3 text-sm text-slate-400">Takım yok</p>
					{/each}
				</div>
			{/if}

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
		</div>
	{/if}
</Sheet>

<!-- Red sheet'i -->
<Sheet open={rejectSheetOpen} title={rejectTarget ? `Reddet — ${rejectTarget.name}` : ''} onclose={() => (rejectSheetOpen = false)}>
	<form class="space-y-4" onsubmit={reject}>
		<div>
			<label class="label" for="reject-note">Red Sebebi (zorunlu)</label>
			<textarea id="reject-note" class="input min-h-24" rows="3" bind:value={rejectNote} required placeholder="Örn. yüzey hatalı, yeniden yapılmalı"></textarea>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (rejectSheetOpen = false)}>İptal</button>
			<button type="submit" class="btn-danger flex-1" disabled={!rejectNote.trim()}>
				Reddet
			</button>
		</div>
	</form>
</Sheet>

<!-- Rework sheet'i (Faz 7) -->
<Sheet open={reworkSheetOpen} title="Rework Başlat" onclose={() => (reworkSheetOpen = false)}>
	<form class="space-y-4" onsubmit={submitRework}>
		<p class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
			Başarısız adım geri alınmaz; seçilen adımdan itibaren <strong>yeni deneme zinciri</strong> oluşturulur.
			Seçilen adım ve sonraki adımlar yeniden yapılır, önceki onaylanmış adımlar korunur.
		</p>
		<div>
			<label class="label" for="rework-restart">Yeniden Başlanacak Adım</label>
			<select id="rework-restart" class="input" bind:value={reworkRestart} required>
				<option value="">— Seçin —</option>
				{#if instance}
					{#each instance.processes as p (p.id)}
						<option value={p.id}>{p.name}</option>
					{/each}
				{/if}
			</select>
			<p class="mt-1.5 text-xs text-slate-500">
				Seçilen adım, başarısız adıma giden yolda olmalıdır.
			</p>
		</div>
		<div>
			<label class="label" for="rework-reason">Sebep (zorunlu)</label>
			<textarea id="rework-reason" class="input min-h-24" rows="3" bind:value={reworkReason} required placeholder="Örn. ölçüler hatalı kesildi"></textarea>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (reworkSheetOpen = false)}>Vazgeç</button>
			<button type="submit" class="btn-primary flex-1 !bg-amber-600 hover:!bg-amber-700" disabled={!reworkRestart || !reworkReason.trim()}>
				Rework'u Başlat
			</button>
		</div>
	</form>
</Sheet>

<!-- Surec yorumlari sheet'i (Faz 8) -->
<Sheet open={commentsSheetOpen} title={commentsTarget ? `Yorumlar — ${commentsTarget.name}` : ''} onclose={() => (commentsSheetOpen = false)}>
	{#if commentsTarget}
		<CommentsPanel {wid} entityType="process_instance" entityId={commentsTarget.id} />
	{/if}
</Sheet>


<!-- Hizli akis olusturma sheet'i -->
<Sheet bind:open={quickOpen} title="Hızlı Akış Oluştur" onclose={() => (quickOpen = false)}>
	<form class="space-y-4" onsubmit={submitQuick}>
		<div>
			<label class="label" for="qf-name">Akış Adı</label>
			<input id="qf-name" class="input" bind:value={qfName} placeholder="Örn. Tezgah Akışı" required maxlength={80} />
		</div>

		<div>
			<label class="label" for="qf-step">Adımlar (sırayla ekleyin)</label>
			<div class="flex gap-2">
				<input
					id="qf-step"
					class="input flex-1"
					bind:value={qfInput}
					placeholder="Örn. Kesim"
					onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); qfAdd(); } }}
				/>
				<button type="button" class="btn-secondary shrink-0 !px-4" onclick={qfAdd} disabled={!qfInput.trim() || qfSteps.length >= 20}>
					<Icon name="plus" size={16} />
				</button>
			</div>

			{#if qfSteps.length > 0}
				<div class="mt-3 space-y-1.5">
					{#each qfSteps as st, i (i)}
						<div class="flex items-center gap-2 rounded-xl border border-slate-200 bg-white px-3 py-2.5">
							<span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-indigo-50 text-[11px] font-bold text-indigo-700">{i + 1}</span>
							<span class="min-w-0 flex-1 truncate text-sm font-medium">{st.name}</span>
							<button
								type="button"
								class="flex size-9 shrink-0 items-center justify-center rounded-lg transition-colors {st.approval ? 'bg-violet-100 text-violet-600' : 'text-slate-300 hover:bg-slate-100 hover:text-slate-500'}"
								onclick={() => qfToggleApproval(i)}
								title={st.approval ? 'Onay gerekli (kapat)' : 'Onay gerekli yap'}
							>
								<Icon name="check-circle" size={15} />
							</button>
							<button
								type="button"
								class="flex size-9 shrink-0 items-center justify-center rounded-lg text-slate-300 hover:bg-slate-100 hover:text-slate-600 disabled:opacity-30"
								onclick={() => qfMoveUp(i)}
								disabled={i === 0}
								aria-label="Yukarı taşı"
							>
								<Icon name="arrow-up" size={14} />
							</button>
							<button
								type="button"
								class="flex size-9 shrink-0 items-center justify-center rounded-lg text-slate-300 hover:bg-red-50 hover:text-red-500"
								onclick={() => qfRemove(i)}
								aria-label="Sil"
							>
								<Icon name="x" size={14} />
							</button>
						</div>
					{/each}
					<p class="pt-1 text-xs text-slate-400">
						Adımlar sırayla bağlanır: {qfSteps.map((x) => x.name).join(' › ')}
					</p>
				</div>
			{:else}
				<p class="mt-2 text-xs text-slate-400">En az 1 adım ekleyin. Örn: Kesim, İmalat, Sevkiyat</p>
			{/if}
		</div>

		<p class="rounded-lg bg-slate-50 px-3 py-2 text-xs text-slate-500">
			Yayınlanır ve bu iş kalemine otomatik atanır. Paralel dallar ve varsayılan atanan için
			Ayarlar → Akışlar'daki gelişmiş düzenleyiciyi kullanın.
		</p>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}

		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (quickOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !qfName.trim() || qfSteps.length === 0}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur ve Ata'}
			</button>
		</div>
	</form>
</Sheet>
