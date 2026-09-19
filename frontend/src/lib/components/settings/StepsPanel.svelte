<script lang="ts">
	// Adim Havuzu: yeniden kullanilabilir surec adimlari (ad + varsayilan sorumlu + onay).
	// Bir kez tanimla, surec gruplarinda defalarca kullan.
	import { api, ApiError } from '$lib/api/client';
	import type { StepDefinition, Member, Team, Role } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let {
		wid,
		steps,
		members,
		teams,
		roles,
		reload
	}: {
		wid: string;
		steps: StepDefinition[];
		members: Member[];
		teams: Team[];
		roles: Role[];
		reload: () => void;
	} = $props();

	let sheetOpen = $state(false);
	let editId = $state<string | null>(null);
	let name = $state('');
	let desc = $state('');
	let assigneeType = $state<'' | 'user' | 'team'>('');
	let assigneeId = $state('');
	let approval = $state(false);
	let approverRole = $state('');
	let busy = $state(false);
	let error = $state('');

	const memberName = (uid: string) => members.find((m) => m.user_id === uid)?.name ?? '?';
	const teamName = (tid: string) => teams.find((t) => t.id === tid)?.name ?? '?';

	function assigneeLabel(s: StepDefinition): string | null {
		if (!s.default_assignee_type || !s.default_assignee_id) return null;
		return s.default_assignee_type === 'user'
			? memberName(s.default_assignee_id)
			: teamName(s.default_assignee_id);
	}

	function openCreate() {
		error = '';
		editId = null;
		name = '';
		desc = '';
		assigneeType = '';
		assigneeId = '';
		approval = false;
		approverRole = '';
		sheetOpen = true;
	}

	function openEdit(s: StepDefinition) {
		error = '';
		editId = s.id;
		name = s.name;
		desc = s.description ?? '';
		assigneeType = (s.default_assignee_type as 'user' | 'team' | null) ?? '';
		assigneeId = s.default_assignee_id ?? '';
		approval = s.requires_approval;
		approverRole = s.approver_role_id ?? '';
		sheetOpen = true;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		const body = {
			name: name.trim(),
			description: desc.trim() || null,
			default_assignee:
				assigneeType && assigneeId ? { type: assigneeType, id: assigneeId } : null,
			requires_approval: approval,
			approver_role_id: approval && approverRole ? approverRole : null
		};
		try {
			if (editId) {
				await api.patch(`/workspaces/${wid}/steps/${editId}`, body);
			} else {
				await api.post(`/workspaces/${wid}/steps`, body);
			}
			sheetOpen = false;
			reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function archive(s: StepDefinition) {
		if (!confirm(`"${s.name}" adımı arşivlensin mi? Grup kurulumunda artık seçilmez.`)) return;
		try {
			await api.post(`/workspaces/${wid}/steps/${s.id}/archive`, undefined);
			reload();
		} catch {
			/* sessiz */
		}
	}
</script>

<div class="mb-3 rounded-xl bg-indigo-50 px-4 py-3 text-xs text-indigo-900">
	Adımlar burada <strong>bir kez</strong> tanımlanır (varsayılan sorumlu + onay kuralı ile).
	Süreç Grupları bu adımlardan seçilerek kurulur — her yeni iş için yeniden yazmak gerekmez.
</div>

{#if steps.length === 0}
	<div class="card">
		<EmptyState
			icon="list"
			title="Adım havuzu boş"
			description="Sık yaptığınız işleri adım olarak tanımlayın: Kesim, Montaj, Boya, Kontrol…"
			actionLabel="İlk Adımı Ekle"
			onaction={openCreate}
		/>
	</div>
{:else}
	<div class="card divide-y divide-slate-100 !p-0">
		{#each steps as s (s.id)}
			<div class="flex items-center gap-3 px-4 py-3">
				<div class="flex size-9 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-slate-500">
					<Icon name="wrench" size={17} />
				</div>
				<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openEdit(s)}>
					<p class="truncate text-sm font-semibold">{s.name}</p>
					<div class="mt-0.5 flex flex-wrap items-center gap-1.5">
						{#if assigneeLabel(s)}
							<span class="badge bg-indigo-50 text-indigo-700">
								<Icon name="user" size={11} class="mr-0.5" />
								{assigneeLabel(s)}
							</span>
						{:else}
							<span class="badge bg-slate-100 text-slate-500">Sorumlu yok</span>
						{/if}
						{#if s.requires_approval}
							<span class="badge bg-violet-50 text-violet-700">Onaylı</span>
						{/if}
					</div>
				</button>
				<button
					type="button"
					class="flex size-9 shrink-0 items-center justify-center rounded-lg text-slate-400 hover:bg-red-50 hover:text-red-600"
					onclick={() => archive(s)}
					aria-label="Arşivle"
				>
					<Icon name="trash" size={16} />
				</button>
			</div>
		{/each}
	</div>
{/if}

<button type="button" class="btn-primary mt-4 w-full" onclick={openCreate}>
	+ Yeni Adım
</button>

<Sheet open={sheetOpen} title={editId ? 'Adımı Düzenle' : 'Yeni Adım'} onclose={() => (sheetOpen = false)}>
	<form class="space-y-4" onsubmit={submit}>
		<div>
			<label class="label" for="step-name">Adım Adı</label>
			<input id="step-name" class="input" bind:value={name} required maxlength={80} placeholder="Örn. Kesim, Montaj, Boya" />
		</div>
		<div>
			<label class="label" for="step-desc">Açıklama (opsiyonel)</label>
			<input id="step-desc" class="input" bind:value={desc} maxlength={200} />
		</div>
		<div class="rounded-xl bg-slate-50 p-3">
			<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">Varsayılan Sorumlu</p>
			<p class="mb-2 text-xs text-slate-500">Bu adım bir işe eklendiğinde otomatik bu kişiye/takıma görev düşer.</p>
			<div class="grid grid-cols-2 gap-2">
				<select class="input" bind:value={assigneeType}>
					<option value="">Yok</option>
					<option value="user">Üye</option>
					<option value="team">Takım</option>
				</select>
				<select class="input" bind:value={assigneeId} disabled={!assigneeType}>
					<option value="">— Seçin —</option>
					{#if assigneeType === 'user'}
						{#each members as m (m.user_id)}
							<option value={m.user_id}>{m.name}</option>
						{/each}
					{:else if assigneeType === 'team'}
						{#each teams as t (t.id)}
							<option value={t.id}>{t.name}</option>
						{/each}
					{/if}
				</select>
			</div>
		</div>
		<label class="flex min-h-11 items-center gap-3 rounded-lg bg-slate-50 px-3 text-sm font-medium text-slate-700">
			<input type="checkbox" class="size-4 accent-indigo-600" bind:checked={approval} />
			Tamamlanınca onay gerektirir
		</label>
		{#if approval}
			<div>
				<label class="label" for="step-role">Onay Rolü (boş = yetkisi olan herkes)</label>
				<select id="step-role" class="input" bind:value={approverRole}>
					<option value="">Yetkisi olan herkes</option>
					{#each roles as r (r.id)}
						{#if r.name !== 'Owner'}
							<option value={r.id}>{r.name}</option>
						{/if}
					{/each}
				</select>
			</div>
		{/if}

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (sheetOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !name.trim()}>
				{busy ? 'Kaydediliyor…' : 'Kaydet'}
			</button>
		</div>
	</form>
</Sheet>
