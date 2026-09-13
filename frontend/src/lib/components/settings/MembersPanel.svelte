<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Member, Role, Section } from '$lib/api/types';
	import { parseJsonArray } from '$lib/api/types';
	import type { WorkspaceInvite } from '$lib/api/types';
	import Icon from '$lib/components/Icon.svelte';
	import Sheet from '$lib/components/Sheet.svelte';

	let {
		wid,
		members,
		roles,
		sections,
		reload
	}: {
		wid: string;
		members: Member[];
		roles: Role[];
		sections: Section[];
		reload: () => Promise<void>;
	} = $props();

	let addOpen = $state(false);
	let editMember = $state<Member | null>(null);
	let addEmail = $state('');
	let addRole = $state('');
	let editRole = $state('');
	let scopeSelection = $state<Set<string>>(new Set());
	let busy = $state(false);
	let error = $state('');

	// Davet sistemi
	let invites = $state<WorkspaceInvite[]>([]);
	let inviteOpen = $state(false);
	let invEmail = $state('');
	let invRole = $state('');

	$effect(() => {
		void loadInvites();
	});

	async function loadInvites() {
		try {
			invites = await api.get<WorkspaceInvite[]>(`/workspaces/${wid}/invites`);
		} catch {
			invites = [];
		}
	}

	async function sendInvite(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/invites`, {
				email: invEmail.trim(),
				role_id: invRole
			});
			inviteOpen = false;
			invEmail = '';
			alert('Davet e-postası gönderildi');
			await loadInvites();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Davet gönderilemedi';
		} finally {
			busy = false;
		}
	}

	async function cancelInvite(inv: WorkspaceInvite) {
		if (!confirm(`${inv.email} daveti iptal edilsin mi?`)) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/invites/${inv.id}`);
			await loadInvites();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'İptal edilemedi';
		}
	}

	const activeSections = $derived(sections.filter((s) => !s.archived));
	const roleOptions = $derived(roles.filter((r) => r.name !== 'Owner'));

	function openAdd() {
		error = '';
		addEmail = '';
		addRole = roleOptions[0]?.id ?? '';
		addOpen = true;
	}

	function openEdit(m: Member) {
		error = '';
		editRole = m.role_id;
		scopeSelection = new Set(parseJsonArray(m.scope_section_ids));
		editMember = m;
	}

	async function addMember(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/members`, { email: addEmail, role_id: addRole });
			addOpen = false;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Üye eklenemedi';
		} finally {
			busy = false;
		}
	}

	async function saveMember(e: SubmitEvent) {
		e.preventDefault();
		if (!editMember) return;
		error = '';
		busy = true;
		try {
			if (editRole !== editMember.role_id) {
				await api.patch(`/workspaces/${wid}/members/${editMember.id}`, { role_id: editRole });
			}
			await api.put(`/workspaces/${wid}/members/${editMember.id}/scopes`, {
				section_ids: [...scopeSelection]
			});
			editMember = null;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function removeMember(m: Member) {
		if (!confirm(`${m.name} workspace'ten çıkarılsın mı?`)) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/members/${m.id}`);
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Çıkarılamadı';
		}
	}

	function toggleScope(id: string) {
		const next = new Set(scopeSelection);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		scopeSelection = next;
	}

	function sectionName(id: string): string {
		return activeSections.find((s) => s.id === id)?.name ?? id;
	}
</script>

<div class="mb-3 flex items-center justify-between">
	<h2 class="font-semibold">Üyeler ({members.length})</h2>
	<div class="flex gap-2">
		<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openAdd}>+ Ekle</button>
		<button type="button" class="btn-primary !min-h-9 !px-3 !text-xs" onclick={() => { error = ''; invEmail = ''; invRole = roleOptions[0]?.id ?? ''; inviteOpen = true; }}>
			<Icon name="mail" size={14} /> Davet Et
		</button>
	</div>
</div>

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}

<div class="card divide-y divide-slate-100 !p-0">
	{#each members as m (m.id)}
		<button
			type="button"
			class="flex w-full items-center justify-between gap-2 px-4 py-3 text-left hover:bg-slate-50"
			onclick={() => openEdit(m)}
		>
			<div class="min-w-0">
				<p class="truncate text-sm font-medium">{m.name}</p>
				<p class="truncate text-xs text-slate-500">
					{m.email}
					{#if parseJsonArray(m.scope_section_ids).length > 0}
						· kapsam: {parseJsonArray(m.scope_section_ids).length} bölüm
					{/if}
				</p>
			</div>
			<span class="badge shrink-0 {m.role_name === 'Owner' ? 'bg-amber-50 text-amber-700' : 'bg-slate-100 text-slate-600'}">
				{m.role_name}
			</span>
		</button>
	{/each}
</div>

<Sheet bind:open={addOpen} title="Üye Ekle">
	<form class="space-y-4" onsubmit={addMember}>
		<div>
			<label class="label" for="member-email">E-posta (kayıtlı kullanıcı)</label>
			<input id="member-email" class="input" type="email" bind:value={addEmail} placeholder="uye@sirket.com" required />
			<p class="mt-1.5 text-xs text-slate-500">Kullanıcı henüz kayıt olmadıysa önce kayıt olmalı.</p>
		</div>
		<div>
			<label class="label" for="member-role">Rol</label>
			<select id="member-role" class="input" bind:value={addRole}>
				{#each roleOptions as r (r.id)}
					<option value={r.id}>{r.name}</option>
				{/each}
			</select>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (addOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !addEmail || !addRole}>
				{busy ? 'Ekleniyor…' : 'Ekle'}
			</button>
		</div>
	</form>
</Sheet>

<Sheet open={editMember !== null} title={editMember ? editMember.name : ''} onclose={() => (editMember = null)}>
	{#if editMember}
		<form class="space-y-4" onsubmit={saveMember}>
			{#if editMember.role_name !== 'Owner'}
				<div>
					<label class="label" for="edit-role">Rol</label>
					<select id="edit-role" class="input" bind:value={editRole}>
						{#each roleOptions as r (r.id)}
							<option value={r.id}>{r.name}</option>
						{/each}
					</select>
				</div>

				<div>
					<p class="label">Erişim Kapsamı</p>
					<p class="mb-2 text-xs text-slate-500">
						Hiç seçim yapılmazsa tüm workspace'e erişir. Seçilen bölümler ve alt bölümleri görünür olur.
					</p>
					{#if activeSections.length === 0}
						<p class="text-xs text-slate-400">Bölüm yok — kapsam sınırı yapılandırılamaz.</p>
					{:else}
						<div class="max-h-56 space-y-1 overflow-y-auto rounded-lg border border-slate-200 p-2">
							{#each activeSections as s (s.id)}
								<label class="flex min-h-11 items-center gap-3 rounded-md px-2 text-sm hover:bg-slate-50" style="padding-left: {0.5 + s.depth * 1.25}rem">
									<input
										type="checkbox"
										class="size-4 shrink-0 accent-indigo-600"
										checked={scopeSelection.has(s.id)}
										onchange={() => toggleScope(s.id)}
									/>
									<span class="truncate">{s.name}</span>
								</label>
							{/each}
						</div>
					{/if}
					{#if scopeSelection.size > 0}
						<p class="mt-2 text-xs text-indigo-600">
							{scopeSelection.size} bölüm seçildi: {[...scopeSelection].slice(0, 3).map(sectionName).join(', ')}{scopeSelection.size > 3 ? '…' : ''}
						</p>
					{/if}
				</div>
			{:else}
				<p class="text-sm text-slate-500">Owner tüm workspace'e her zaman tam erişir.</p>
			{/if}

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}

			<div class="flex gap-2">
				<button type="submit" class="btn-primary flex-1" disabled={busy}>
					{busy ? 'Kaydediliyor…' : 'Kaydet'}
				</button>
				{#if editMember.role_name !== 'Owner'}
					<button
						type="button"
						class="btn-danger"
						disabled={busy}
						onclick={() => removeMember(editMember!)}
					>
						Çıkar
					</button>
				{/if}
			</div>
		</form>
	{/if}
</Sheet>


<!-- Bekleyen davetler -->
{#if invites.length > 0}
	<h3 class="mt-5 mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-slate-500">
		<Icon name="mail" size={14} /> Davetler ({invites.filter((i) => i.status === 'pending').length} bekliyor)
	</h3>
	<div class="card divide-y divide-slate-100 !p-0">
		{#each invites as inv (inv.id)}
			<div class="flex items-center gap-3 px-4 py-3">
				<div class="flex size-9 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-slate-500">
					<Icon name="mail" size={16} />
				</div>
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium">{inv.email}</p>
					<p class="truncate text-xs text-slate-500">
						{inv.role_name} · {inv.invited_by_name} · {new Date(inv.created_at).toLocaleDateString('tr-TR')}
					</p>
				</div>
				<span class="badge shrink-0 {inv.status === 'pending' ? 'bg-amber-50 text-amber-700' : inv.status === 'accepted' ? 'bg-emerald-50 text-emerald-700' : 'bg-slate-100 text-slate-500'}">
					{inv.status === 'pending' ? 'Bekliyor' : inv.status === 'accepted' ? 'Kabul edildi' : 'İptal'}
				</span>
				{#if inv.status === 'pending'}
					<button
						type="button"
						class="flex size-11 shrink-0 items-center justify-center rounded-lg text-red-400 hover:bg-red-50"
						onclick={() => cancelInvite(inv)}
						aria-label="İptal"
					>
						<Icon name="x" size={16} />
					</button>
				{/if}
			</div>
		{/each}
	</div>
{/if}

<!-- Davet sheet'i -->
<Sheet bind:open={inviteOpen} title="E-posta ile Davet Et" onclose={() => (inviteOpen = false)}>
	<form class="space-y-4" onsubmit={sendInvite}>
		<p class="rounded-lg bg-indigo-50 px-3 py-2 text-xs text-indigo-800">
			Kayıtlı olmayan kişiye bile davet gönderilebilir. Kişi e-postadaki bağlantıdan
			hesabını oluşturur ve otomatik olarak bu workspace'e katılır.
		</p>
		<div>
			<label class="label" for="inv-email">E-posta Adresi</label>
			<input id="inv-email" class="input" type="email" bind:value={invEmail} placeholder="uye@sirket.com" required />
		</div>
		<div>
			<label class="label" for="inv-role">Rol</label>
			<select id="inv-role" class="input" bind:value={invRole}>
				{#each roleOptions as r (r.id)}
					<option value={r.id}>{r.name}</option>
				{/each}
			</select>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (inviteOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !invEmail.trim() || !invRole}>
				{busy ? 'Gönderiliyor…' : 'Davet Gönder'}
			</button>
		</div>
	</form>
</Sheet>
