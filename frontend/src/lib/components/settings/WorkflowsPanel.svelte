<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { WorkflowTemplate, WorkflowDraft, WorkflowNode, WorkflowDependency, WorkflowVersion, Role, Member, Team, StepDefinition } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let {
		wid,
		workflows,
		steps,
		reload
	}: {
		wid: string;
		workflows: WorkflowTemplate[];
		steps: StepDefinition[];
		reload: () => Promise<void>;
	} = $props();

	let editTemplate = $state<WorkflowTemplate | null>(null);
	let wfName = $state('');
	let wfDesc = $state('');
	let busy = $state(false);
	let error = $state('');

	// Yeni Grup: havuzdan adim secimi (sirali)
	let groupOpen = $state(false);
	let groupName = $state('');
	let groupSearch = $state('');
	let selected = $state<StepDefinition[]>([]);

	const availableSteps = $derived(
		steps.filter(
			(s) =>
				!selected.some((x) => x.id === s.id) &&
				(groupSearch.trim() === '' || s.name.toLowerCase().includes(groupSearch.trim().toLowerCase()))
		)
	);

	function openGroup() {
		error = '';
		groupName = '';
		groupSearch = '';
		selected = [];
		groupOpen = true;
	}

	function addStep(s: StepDefinition) {
		selected = [...selected, s];
	}

	function removeStep(i: number) {
		selected = selected.filter((_, idx) => idx !== i);
	}

	function moveStep(i: number, dir: -1 | 1) {
		const j = i + dir;
		if (j < 0 || j >= selected.length) return;
		const next = [...selected];
		[next[i], next[j]] = [next[j], next[i]];
		selected = next;
	}

	async function submitGroup(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/step-groups`, {
				name: groupName.trim(),
				step_ids: selected.map((s) => s.id)
			});
			groupOpen = false;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Grup oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	// Editor durumu
	let editorOpen = $state(false);
	let draft = $state<WorkflowDraft | null>(null);
	let versions = $state<WorkflowVersion[]>([]);
	let nodeSheetOpen = $state(false);
	let editingNode = $state<WorkflowNode | null>(null);
	let nodeName = $state('');
	let nodeDesc = $state('');
	// Onay kurali (Faz 6)
	let approvalRequired = $state(false);
	let approverRole = $state('');
	let wfRoles = $state<Role[]>([]);

	// Varsayilan atanan (adim bazli otomatik atama)
	let defAssigneeType = $state(''); // '' | user | team
	let defAssigneeId = $state('');
	let wfMembers = $state<Member[]>([]);
	let wfTeams = $state<Team[]>([]);
	let depsSheetOpen = $state(false);
	let depsNode = $state<WorkflowNode | null>(null);
	let depSelection = $state<Set<string>>(new Set());
	let versionsOpen = $state(false);

	// Onay rolu secimi icin rolleri yukle
	$effect(() => {
		wid;
		void (async () => {
			try {
				wfRoles = await api.get<Role[]>(`/workspaces/${wid}/roles`);
			} catch {
				wfRoles = [];
			}
		})();
	});

	// Varsayilan atanan secimi icin uyeler/takimlar
	$effect(() => {
		wid;
		void (async () => {
			try {
				const [m, t] = await Promise.all([
					api.get<Member[]>(`/workspaces/${wid}/members`),
					api.get<Team[]>(`/workspaces/${wid}/teams`)
				]);
				wfMembers = m;
				wfTeams = t;
			} catch {
				wfMembers = [];
				wfTeams = [];
			}
		})();
	});

	function openEditTemplate(t: WorkflowTemplate) {
		error = '';
		wfName = t.name;
		wfDesc = t.description ?? '';
		editTemplate = t;
	}

	async function saveTemplate(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			if (editTemplate) {
				await api.patch(`/workspaces/${wid}/workflows/${editTemplate.id}`, {
					name: wfName,
					description: wfDesc || null
				});
				editTemplate = null;
			}
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function archiveTemplate(t: WorkflowTemplate) {
		if (!confirm(`${t.name} akışı arşivlensin mi? Yayınlanmış versiyonlar korunur.`)) return;
		error = '';
		try {
			await api.post(`/workspaces/${wid}/workflows/${t.id}/archive`, undefined);
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Arşivlenemedi';
		}
	}

	// --- Editor ---
	async function openEditor(t: WorkflowTemplate) {
		error = '';
		editTemplate = t;
		try {
			draft = await api.get<WorkflowDraft>(`/workspaces/${wid}/workflows/${t.id}/draft`);
			editorOpen = true;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yüklenemedi';
		}
	}

	async function refreshDraft() {
		if (!editTemplate) return;
		draft = await api.get<WorkflowDraft>(`/workspaces/${wid}/workflows/${editTemplate.id}/draft`);
	}

	function nodeName_(id: string): string {
		return draft?.nodes.find((n) => n.id === id)?.name ?? '?';
	}

	/** Bir node'un onceki adimlari (predecessor'lari) */
	function predecessorsOf(nid: string): WorkflowDependency[] {
		return draft?.dependencies.filter((d) => d.successor_node_id === nid) ?? [];
	}

	function openNodeEditor(n: WorkflowNode | null) {
		error = '';
		editingNode = n;
		nodeName = n?.name ?? '';
		nodeDesc = n?.description ?? '';
		if (!n) {
			defAssigneeType = '';
			defAssigneeId = '';
		}
		// Onay kuralini yukle
		const rule = n?.approval_rule ? parseApprovalRule(n) : null;
		approvalRequired = rule?.required ?? false;
		approverRole = rule?.approver_role_id ?? '';
		// Varsayilan atanan yukle
		defAssigneeType = n?.default_assignee_type ?? '';
		defAssigneeId = n?.default_assignee_id ?? '';
		nodeSheetOpen = true;
	}

	function parseApprovalRule(n: WorkflowNode): { required: boolean; approver_role_id?: string } | null {
		try {
			return JSON.parse(n.approval_rule as string);
		} catch {
			return null;
		}
	}

	async function saveNode(e: SubmitEvent) {
		e.preventDefault();
		if (!editTemplate) return;
		error = '';
		busy = true;
		try {
			if (editingNode) {
				await api.patch(
					`/workspaces/${wid}/workflows/${editTemplate.id}/draft/nodes/${editingNode.id}`,
					{
						name: nodeName,
						description: nodeDesc || null,
						approval_rule: {
							required: approvalRequired,
							approver_role_id: approvalRequired ? approverRole || null : null
						},
						default_assignee: defAssigneeType
							? { type: defAssigneeType, id: defAssigneeId }
							: null
					}
				);
			} else {
				const created = await api.post<WorkflowNode>(
					`/workspaces/${wid}/workflows/${editTemplate.id}/draft/nodes`,
					{
						name: nodeName,
						description: nodeDesc || null,
						approval_rule: {
							required: approvalRequired,
							approver_role_id: approvalRequired ? approverRole || null : null
						}
					}
				);
				// Yeni node'da kural/atanan set edildiyse hemen patch'le (POST bunlari almiyor)
				if ((approvalRequired || defAssigneeType) && created?.id) {
					await api.patch(
						`/workspaces/${wid}/workflows/${editTemplate.id}/draft/nodes/${created.id}`,
						{
							...(approvalRequired
								? { approval_rule: { required: true, approver_role_id: approverRole || null } }
								: {}),
							...(defAssigneeType
								? { default_assignee: { type: defAssigneeType, id: defAssigneeId } }
								: {})
						}
					);
				}
			}
			nodeSheetOpen = false;
			await refreshDraft();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function moveNode(n: WorkflowNode, up: boolean) {
		if (!editTemplate || !up) return; // MVP: sadece yukari tasi
		try {
			await api.patch(`/workspaces/${wid}/workflows/${editTemplate.id}/draft/nodes/${n.id}`, {
				move_up: true
			});
			await refreshDraft();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Taşınamadı';
		}
	}

	async function deleteNode(n: WorkflowNode) {
		if (!editTemplate || !confirm(`${n.name} adımı silinsin mi? Bağımlılıkları da kaldırılır.`)) return;
		try {
			await api.del(`/workspaces/${wid}/workflows/${editTemplate.id}/draft/nodes/${n.id}`);
			await refreshDraft();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Silinemedi';
		}
	}

	// --- Bagimlilik editoru ---
	function openDepsEditor(n: WorkflowNode) {
		error = '';
		depsNode = n;
		depSelection = new Set(predecessorsOf(n.id).map((d) => d.predecessor_node_id));
		depsSheetOpen = true;
	}

	function toggleDep(nid: string) {
		const next = new Set(depSelection);
		if (next.has(nid)) {
			next.delete(nid);
		} else {
			next.add(nid);
		}
		depSelection = next;
	}

	async function saveDeps() {
		if (!editTemplate || !depsNode || !draft) return;
		error = '';
		busy = true;
		try {
			const current = new Set(predecessorsOf(depsNode.id).map((d) => d.predecessor_node_id));
			// Eklenecekler
			for (const nid of depSelection) {
				if (!current.has(nid)) {
					await api.post(`/workspaces/${wid}/workflows/${editTemplate.id}/draft/dependencies`, {
						predecessor_node_id: nid,
						successor_node_id: depsNode.id
					});
				}
			}
			// Cikarilacaklar
			for (const d of draft.dependencies.filter((x) => x.successor_node_id === depsNode!.id)) {
				if (!depSelection.has(d.predecessor_node_id)) {
					await api.del(`/workspaces/${wid}/workflows/${editTemplate.id}/draft/dependencies/${d.id}`);
				}
			}
			depsSheetOpen = false;
			await refreshDraft();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi (dongu?)';
		} finally {
			busy = false;
		}
	}

	async function publish() {
		if (!editTemplate || !draft) return;
		if (!confirm(`v${draft.version_number} taslağı yayınlanacak. Onaylıyor musunuz?`)) return;
		error = '';
		try {
			const res = await api.post<{ published_version: number }>(
				`/workspaces/${wid}/workflows/${editTemplate.id}/publish`,
				undefined
			);
			await refreshDraft();
			await reload();
			alert(`v${res.published_version} yayınlandı`);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yayınlanamadı';
		}
	}

	async function openVersions() {
		if (!editTemplate) return;
		try {
			versions = await api.get<WorkflowVersion[]>(
				`/workspaces/${wid}/workflows/${editTemplate.id}/versions`
			);
			versionsOpen = true;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yüklenemedi';
		}
	}

	const statusLabel = (s: string) =>
		s === 'published' ? 'Yayında' : s === 'draft' ? 'Taslak' : s === 'retired' ? 'Pasif' : s;
</script>

<div class="mb-3 flex items-center justify-between">
	<h2 class="font-semibold">Süreç Grupları ({workflows.length})</h2>
	<button type="button" class="btn-primary !min-h-9 !px-3 !text-xs" onclick={openGroup}>+ Yeni Grup</button>
</div>

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}

{#if workflows.length === 0}
	<div class="card">
		<EmptyState
			icon="workflow"
			title="Henüz süreç grubu yok"
			description="Adım Havuzu'ndan adım seçip isimli bir grup kurun: Kesim › Montaj › Kontrol. Grubu bir işe eklediğinizde adımlar ve sorumlular tek seferde gelir."
			actionLabel="Grup Oluştur"
			onaction={openGroup}
		/>
	</div>
{:else}
	<div class="card divide-y divide-slate-100 !p-0">
		{#each workflows as t (t.id)}
			<div class="flex items-center gap-2 px-4 py-3">
				<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openEditor(t)}>
					<p class="truncate text-sm font-medium">{t.name}</p>
					<p class="truncate text-xs text-slate-500">
						{#if t.published_version}
							v{t.published_version} yayında · {t.published_node_count} adım
						{:else}
							Henüz yayınlanmadı
						{/if}
						{#if t.draft_node_count && t.draft_node_count > 0}
							· taslakta {t.draft_node_count} adım
						{/if}
					</p>
				</button>
				<button
					type="button"
					class="flex min-h-11 items-center rounded-lg px-2 text-xs font-medium text-indigo-600 hover:bg-indigo-50"
					onclick={() => openEditTemplate(t)}
				>
					Düzenle
				</button>
				<button
					type="button"
					class="flex size-11 items-center justify-center rounded-lg text-red-500 hover:bg-red-50"
					onclick={() => archiveTemplate(t)}
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

<!-- Yeni Surec Grubu: havuzdan adim sec -->
<Sheet bind:open={groupOpen} title="Yeni Süreç Grubu">
	<form class="space-y-4" onsubmit={submitGroup}>
		<div>
			<label class="label" for="grp-name">Grup Adı</label>
			<input id="grp-name" class="input" bind:value={groupName} placeholder="Örn. Mutfak Tezgahı, Standart Üretim" required maxlength={80} />
		</div>

		{#if selected.length > 0}
			<div class="rounded-xl border border-indigo-200 bg-indigo-50/50 p-3">
				<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-indigo-700">Sıra ({selected.length} adım)</p>
				<div class="space-y-1.5">
					{#each selected as s, i (s.id)}
						<div class="flex items-center gap-2 rounded-lg bg-white px-3 py-2 shadow-sm">
							<span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-indigo-100 text-[11px] font-bold text-indigo-700">{i + 1}</span>
							<div class="min-w-0 flex-1">
								<p class="truncate text-sm font-medium">{s.name}</p>
								{#if s.default_assignee_type}
									<p class="truncate text-[11px] text-slate-400">sorumlu tanımlı{#if s.requires_approval} · onaylı{/if}</p>
								{:else if s.requires_approval}
									<p class="truncate text-[11px] text-slate-400">onaylı</p>
								{/if}
							</div>
							<button type="button" class="flex size-8 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-100 disabled:opacity-30" onclick={() => moveStep(i, -1)} disabled={i === 0} aria-label="Yukarı">
								<Icon name="chevron-up" size={15} />
							</button>
							<button type="button" class="flex size-8 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-100 disabled:opacity-30" onclick={() => moveStep(i, 1)} disabled={i === selected.length - 1} aria-label="Aşağı">
								<Icon name="chevron-down" size={15} />
							</button>
							<button type="button" class="flex size-8 items-center justify-center rounded-lg text-slate-400 hover:bg-red-50 hover:text-red-600" onclick={() => removeStep(i)} aria-label="Çıkar">
								<Icon name="x" size={15} />
							</button>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<div>
			<span class="label">Adım Havuzu'ndan ekle</span>
			{#if steps.length === 0}
				<p class="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
					Adım havuzu boş. Önce <strong>Adımlar</strong> sekmesinde adım tanımlayın.
				</p>
			{:else}
				<div class="relative mb-2">
					<Icon name="search" size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-slate-400" />
					<input class="input !pl-9" placeholder="Adım ara…" bind:value={groupSearch} />
				</div>
				<div class="max-h-56 space-y-1 overflow-y-auto">
					{#each availableSteps as s (s.id)}
						<button
							type="button"
							class="flex w-full items-center gap-2.5 rounded-lg bg-white px-3 py-2 text-left ring-1 ring-slate-200 hover:ring-indigo-300"
							onclick={() => addStep(s)}
						>
							<Icon name="plus" size={15} class="shrink-0 text-indigo-600" />
							<div class="min-w-0 flex-1">
								<p class="truncate text-sm font-medium">{s.name}</p>
								{#if s.default_assignee_type || s.requires_approval}
									<p class="truncate text-[11px] text-slate-400">
										{#if s.default_assignee_type}varsayılan sorumlu{/if}
										{#if s.default_assignee_type && s.requires_approval} · {/if}
										{#if s.requires_approval}onaylı{/if}
									</p>
								{/if}
							</div>
						</button>
					{:else}
						<p class="py-3 text-center text-xs text-slate-400">Eklenecek adım kalmadı</p>
					{/each}
				</div>
			{/if}
		</div>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (groupOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !groupName.trim() || selected.length === 0}>
				{busy ? 'Oluşturuluyor…' : 'Grubu Oluştur'}
			</button>
		</div>
	</form>
</Sheet>

<Sheet open={editTemplate !== null && !editorOpen} title={editTemplate ? `Süreç Grubu: ${editTemplate.name}` : ''} onclose={() => (editTemplate = null)}>
	{#if editTemplate}
		<form class="space-y-4" onsubmit={saveTemplate}>
			<div>
				<label class="label" for="et-wf-name">Grup Adı</label>
				<input id="et-wf-name" class="input" bind:value={wfName} required maxlength={80} />
			</div>
			<div>
				<label class="label" for="et-wf-desc">Açıklama</label>
				<input id="et-wf-desc" class="input" bind:value={wfDesc} maxlength={200} />
			</div>
			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
			<button type="submit" class="btn-primary w-full" disabled={busy}>
				{busy ? 'Kaydediliyor…' : 'Kaydet'}
			</button>
		</form>
	{/if}
</Sheet>

<!-- EDITOR: draft adimlari -->
<Sheet bind:open={editorOpen} title={editTemplate ? `${editTemplate.name} — Gelişmiş Düzenle (Taslak v${draft?.version_number ?? '?'})` : 'Süreç Grubu Editörü'} onclose={() => (editorOpen = false)}>
	<div class="space-y-3">
		<p class="text-xs text-slate-500">
			Adımları ekleyin, her adımın <strong>önceki adımlarını</strong> seçerek akışı kurun.
			Bir adıma birden çok önceki adım bağlanabilir (paralel dallanma). Yayınlanan versiyon bir daha değişmez.
		</p>

		{#if draft && draft.nodes.length === 0}
			<p class="py-4 text-center text-sm text-slate-500">Bu taslakta henüz adım yok.</p>
		{:else if draft}
			<div class="space-y-2">
				{#each draft.nodes as n, i (n.id)}
					<div class="rounded-xl border border-slate-200 bg-white p-3">
						<div class="flex items-center gap-2">
							<span class="flex size-7 shrink-0 items-center justify-center rounded-full bg-indigo-50 text-xs font-bold text-indigo-700">
								{i + 1}
							</span>
							<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openNodeEditor(n)}>
								<p class="truncate text-sm font-medium">{n.name}</p>
							</button>
							<button type="button" class="flex size-9 items-center justify-center rounded-lg text-slate-400 hover:bg-slate-100 disabled:opacity-30"
								onclick={() => moveNode(n, i > 0)} disabled={i === 0} aria-label="Yukarı taşı">
								<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
									<path stroke-linecap="round" stroke-linejoin="round" d="m5 15 7-7 7 7" />
								</svg>
							</button>
							<button type="button" class="flex size-9 items-center justify-center rounded-lg text-red-400 hover:bg-red-50"
								onclick={() => deleteNode(n)} aria-label="Sil">
								<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
									<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
								</svg>
							</button>
						</div>
						<button type="button" class="mt-2 flex w-full flex-wrap items-center gap-1.5 rounded-lg bg-slate-50 px-2 py-2 text-left" onclick={() => openDepsEditor(n)}>
							<span class="text-xs text-slate-500">Önce:</span>
							{#if predecessorsOf(n.id).length === 0}
								<span class="text-xs italic text-slate-400">başlangıç adımı — düzenle</span>
							{:else}
								{#each predecessorsOf(n.id) as d (d.id)}
									<span class="badge bg-indigo-50 text-indigo-700">{nodeName_(d.predecessor_node_id)}</span>
								{/each}
								<span class="ml-auto text-xs text-indigo-600">düzenle</span>
							{/if}
						</button>
					</div>
				{/each}
			</div>
		{/if}

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}

		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => openNodeEditor(null)}>+ Adım Ekle</button>
			<button type="button" class="btn-primary flex-1" onclick={publish} disabled={!draft || draft.nodes.length === 0}>
				Yayınla
			</button>
		</div>
		<button type="button" class="w-full text-sm text-indigo-600" onclick={openVersions}>
			Versiyon geçmişini göster
		</button>
	</div>
</Sheet>

<!-- Node editor -->
<Sheet bind:open={nodeSheetOpen} title={editingNode ? 'Adımı Düzenle' : 'Yeni Adım'} onclose={() => (nodeSheetOpen = false)}>
	<form class="space-y-4" onsubmit={saveNode}>
		<div>
			<label class="label" for="n-name">Adım Adı</label>
			<input id="n-name" class="input" bind:value={nodeName} placeholder="Örn. Kesim" required maxlength={80} />
		</div>
		<div>
			<label class="label" for="n-desc">Açıklama</label>
			<input id="n-desc" class="input" bind:value={nodeDesc} maxlength={200} />
		</div>

		<!-- Onay kurali (Faz 6) -->
		<div class="rounded-xl bg-slate-50 p-3">
			<label class="flex min-h-11 items-center gap-3 text-sm font-medium text-slate-700">
				<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={approvalRequired} />
				Tamamlanınca onay gerektirir
			</label>
			<p class="mb-1 mt-1 px-7 text-xs text-slate-500">
				Açıksa adım "Tamamla" sonrası onay bekler; seçilen rol onaylayıp akışı sürdürür.
			</p>
			{#if approvalRequired}
				<div class="px-7">
					<label class="label" for="n-approver-role">Onay Rolü (boşsa onay yetkisi olan herkes)</label>
					<select id="n-approver-role" class="input" bind:value={approverRole}>
						<option value="">Onay yetkisi olan herkes</option>
						{#each wfRoles.filter((r) => r.name !== 'Owner') as r (r.id)}
							<option value={r.id}>{r.name}</option>
						{/each}
					</select>
				</div>
			{/if}

			<!-- Varsayilan atanan -->
			<div class="mt-3 border-t border-slate-200 pt-3">
				<p class="mb-1 block text-xs font-semibold uppercase tracking-wide text-slate-500">
					Varsayılan Atanan
				</p>
				<p class="mb-2 text-xs text-slate-500">
					Bu adımı içeren her iş kaleminde otomatik atanır.
				</p>
				<div class="grid grid-cols-2 gap-2">
					<select class="input" aria-label="Atanan tipi" bind:value={defAssigneeType} onchange={() => (defAssigneeId = '')}>
						<option value="">Yok</option>
						<option value="user">Üye</option>
						<option value="team">Takım</option>
					</select>
					<select class="input" bind:value={defAssigneeId} disabled={!defAssigneeType}>
						<option value="">— Seçin —</option>
						{#if defAssigneeType === 'user'}
							{#each wfMembers as m (m.user_id)}
								<option value={m.user_id}>{m.name}</option>
							{/each}
						{:else if defAssigneeType === 'team'}
							{#each wfTeams as t (t.id)}
								<option value={t.id}>{t.name}</option>
							{/each}
						{/if}
					</select>
				</div>
			</div>
		</div>

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (nodeSheetOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !nodeName.trim()}>
				{busy ? 'Kaydediliyor…' : 'Kaydet'}
			</button>
		</div>
	</form>
</Sheet>

<!-- Bagimlilik editoru: onceki adimlar -->
<Sheet open={depsSheetOpen} title={depsNode ? `Önceki Adımlar — ${depsNode.name}` : ''} onclose={() => (depsSheetOpen = false)}>
	{#if depsNode && draft}
		<div class="space-y-3">
			<p class="text-xs text-slate-500">
				<strong>{depsNode.name}</strong> hangi adımlar tamamlandığında başlayabilir? (Dongu oluşturan seçimler reddedilir.)
			</p>
			<div class="space-y-1">
				{#each draft.nodes.filter((n) => n.id !== depsNode!.id) as n (n.id)}
					<label class="flex min-h-11 items-center gap-3 rounded-lg border border-slate-200 px-3 text-sm hover:bg-slate-50">
						<input
							type="checkbox"
							class="size-4 shrink-0 accent-indigo-600"
							checked={depSelection.has(n.id)}
							onchange={() => toggleDep(n.id)}
						/>
						<span class="flex-1 truncate">{n.name}</span>
					</label>
				{/each}
			</div>
			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}
			<div class="flex gap-2">
				<button type="button" class="btn-secondary flex-1" onclick={() => (depsSheetOpen = false)}>İptal</button>
				<button type="button" class="btn-primary flex-1" onclick={saveDeps} disabled={busy}>
					{busy ? 'Kaydediliyor…' : 'Kaydet'}
				</button>
			</div>
		</div>
	{/if}
</Sheet>

<!-- Versiyon gecmisi -->
<Sheet bind:open={versionsOpen} title="Versiyon Geçmişi" onclose={() => (versionsOpen = false)}>
	<div class="space-y-2">
		{#each versions as v (v.id)}
			<div class="flex items-center justify-between rounded-lg border border-slate-200 px-3 py-2.5">
				<div>
					<p class="text-sm font-medium">
						v{v.version_number}
						<span class="badge {v.status === 'published' ? 'bg-emerald-50 text-emerald-700' : 'bg-slate-100 text-slate-500'}">
							{statusLabel(v.status)}
						</span>
					</p>
					<p class="text-xs text-slate-500">
						{v.node_count} adım
						{#if v.published_at}· {new Date(v.published_at).toLocaleDateString('tr-TR')}{/if}
					</p>
				</div>
			</div>
		{/each}
		<p class="pt-1 text-center text-xs text-slate-400">Yayınlanan versiyonlar değiştirilemez (immutable).</p>
	</div>
</Sheet>
