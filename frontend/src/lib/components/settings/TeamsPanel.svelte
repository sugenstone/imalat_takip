<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Member, Team } from '$lib/api/types';
	import { parseJsonArray } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';

	let {
		wid,
		teams,
		members,
		reload
	}: {
		wid: string;
		teams: Team[];
		members: Member[];
		reload: () => Promise<void>;
	} = $props();

	let createOpen = $state(false);
	let editTeam = $state<Team | null>(null);
	let teamName = $state('');
	let teamDesc = $state('');
	let busy = $state(false);
	let error = $state('');

	function memberName(uid: string): string {
		return members.find((m) => m.user_id === uid)?.name ?? uid;
	}

	function openCreate() {
		error = '';
		teamName = '';
		teamDesc = '';
		createOpen = true;
	}

	function openEdit(t: Team) {
		error = '';
		teamName = t.name;
		teamDesc = t.description ?? '';
		editTeam = t;
	}

	async function saveCreate(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/teams`, { name: teamName, description: teamDesc });
			createOpen = false;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	async function saveEdit(e: SubmitEvent) {
		e.preventDefault();
		if (!editTeam) return;
		error = '';
		busy = true;
		try {
			await api.patch(`/workspaces/${wid}/teams/${editTeam.id}`, {
				name: teamName,
				description: teamDesc
			});
			editTeam = null;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function toggleTeamMember(team: Team, uid: string, inTeam: boolean) {
		error = '';
		try {
			if (inTeam) {
				await api.del(`/workspaces/${wid}/teams/${team.id}/members/${uid}`);
			} else {
				await api.post(`/workspaces/${wid}/teams/${team.id}/members`, { user_id: uid });
			}
			await reload();
			// duzenleme aciksa tabloyu guncelle
			const fresh = teams.find((t) => t.id === team.id);
			if (fresh && editTeam?.id === team.id) editTeam = fresh;
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'İşlem başarısız';
		}
	}

	async function archiveTeam(t: Team) {
		if (!confirm(`${t.name} arşivlensin mi?`)) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/teams/${t.id}`);
			if (editTeam?.id === t.id) editTeam = null;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Arşivlenemedi';
		}
	}
</script>

<div class="mb-3 flex items-center justify-between">
	<h2 class="font-semibold">Takımlar ({teams.length})</h2>
	<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openCreate}>+ Ekle</button>
</div>

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}

{#if teams.length === 0}
	<div class="card">
		<EmptyState
			icon="users"
			title="Henüz takım yok"
			description="Montaj Ekibi, Kalite Ekibi gibi takımlar kurun; süreç atamalarında kullanılacak."
			actionLabel="Takım Oluştur"
			onaction={openCreate}
		/>
	</div>
{:else}
	<div class="card divide-y divide-slate-100 !p-0">
		{#each teams as t (t.id)}
			<div class="flex items-center justify-between gap-2 px-4 py-3">
				<button type="button" class="min-w-0 flex-1 text-left" onclick={() => openEdit(t)}>
					<p class="truncate text-sm font-medium">{t.name}</p>
					<p class="truncate text-xs text-slate-500">{t.member_count} üye</p>
				</button>
				<button
					type="button"
					class="flex size-11 items-center justify-center rounded-lg text-red-500 hover:bg-red-50"
					onclick={() => archiveTeam(t)}
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

<Sheet bind:open={createOpen} title="Yeni Takım">
	<form class="space-y-4" onsubmit={saveCreate}>
		<div>
			<label class="label" for="team-name">Takım Adı</label>
			<input id="team-name" class="input" bind:value={teamName} placeholder="Örn. Montaj Ekibi" required maxlength={60} />
		</div>
		<div>
			<label class="label" for="team-desc">Açıklama</label>
			<input id="team-desc" class="input" bind:value={teamDesc} placeholder="Opsiyonel" maxlength={200} />
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (createOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !teamName.trim()}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>

<Sheet open={editTeam !== null} title={editTeam ? editTeam.name : ''} onclose={() => (editTeam = null)}>
	{#if editTeam}
		{@const teamMemberIds = parseJsonArray(editTeam.member_ids)}
		<form class="space-y-4" onsubmit={saveEdit}>
			<div>
				<label class="label" for="edit-team-name">Takım Adı</label>
				<input id="edit-team-name" class="input" bind:value={teamName} required maxlength={60} />
			</div>
			<div>
				<label class="label" for="edit-team-desc">Açıklama</label>
				<input id="edit-team-desc" class="input" bind:value={teamDesc} maxlength={200} />
			</div>

			<div>
				<p class="label">Üyeler</p>
				<div class="max-h-64 space-y-1 overflow-y-auto rounded-lg border border-slate-200 p-2">
					{#each members as m (m.user_id)}
						{@const inTeam = teamMemberIds.includes(m.user_id)}
						<label class="flex min-h-11 items-center justify-between gap-3 rounded-md px-2 text-sm hover:bg-slate-50">
							<span class="truncate">{m.name}</span>
							<input
								type="checkbox"
								class="size-4 shrink-0 accent-indigo-600"
								checked={inTeam}
								onchange={() => toggleTeamMember(editTeam!, m.user_id, inTeam)}
							/>
						</label>
					{/each}
				</div>
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
