<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Role } from '$lib/api/types';
	import { parseJsonArray } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';

	let {
		wid,
		roles,
		reload
	}: {
		wid: string;
		roles: Role[];
		reload: () => Promise<void>;
	} = $props();

	interface PermDef {
		key: string;
		description: string;
	}

	let allPermissions = $state<PermDef[]>([]);
	let editRole = $state<Role | null>(null);
	let createOpen = $state(false);
	let roleName = $state('');
	let selectedPerms = $state<Set<string>>(new Set());
	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		void loadPermissions();
	});

	async function loadPermissions() {
		try {
			const res = await api.get<{ permissions: PermDef[] }>('/permissions');
			allPermissions = res.permissions;
		} catch {
			allPermissions = [];
		}
	}

	function openEdit(r: Role) {
		error = '';
		selectedPerms = new Set(parseJsonArray(r.permission_keys));
		roleName = r.name;
		editRole = r;
	}

	function openCreate() {
		error = '';
		roleName = '';
		selectedPerms = new Set();
		createOpen = true;
	}

	function toggle(key: string) {
		const next = new Set(selectedPerms);
		if (next.has(key)) {
			next.delete(key);
		} else {
			next.add(key);
		}
		selectedPerms = next;
	}

	async function saveEdit(e: SubmitEvent) {
		e.preventDefault();
		if (!editRole) return;
		error = '';
		busy = true;
		try {
			const body: Record<string, unknown> = {
				permission_keys: [...selectedPerms]
			};
			if (!editRole.is_system) body.name = roleName;
			await api.patch(`/workspaces/${wid}/roles/${editRole.id}`, body);
			editRole = null;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
		} finally {
			busy = false;
		}
	}

	async function saveCreate(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/roles`, {
				name: roleName,
				permission_keys: [...selectedPerms]
			});
			createOpen = false;
			await reload();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Oluşturulamadı';
		} finally {
			busy = false;
		}
	}

	const permCount = (r: Role) => parseJsonArray(r.permission_keys).length;
</script>

<div class="mb-3 flex items-center justify-between">
	<h2 class="font-semibold">Roller</h2>
	<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={openCreate}>+ Yeni Rol</button>
</div>

<div class="card divide-y divide-slate-100 !p-0">
	{#each roles as r (r.id)}
		<button
			type="button"
			class="flex w-full items-center justify-between gap-2 px-4 py-3 text-left hover:bg-slate-50"
			onclick={() => (r.name === 'Owner' ? undefined : openEdit(r))}
			disabled={r.name === 'Owner'}
		>
			<div class="min-w-0">
				<div class="flex items-center gap-2">
					<span class="truncate text-sm font-medium">{r.name}</span>
					{#if r.name === 'Owner'}
						<span class="badge bg-amber-50 text-amber-700">Sabit</span>
					{:else if r.is_system}
						<span class="badge bg-slate-100 text-slate-500">Sistem</span>
					{/if}
				</div>
				<p class="truncate text-xs text-slate-500">{permCount(r)} izin</p>
			</div>
			{#if r.name !== 'Owner'}
				<span class="text-xs text-indigo-600">Düzenle</span>
			{/if}
		</button>
	{/each}
</div>

<!-- Rol duzenleme -->
<Sheet open={editRole !== null} title={editRole ? `Rol: ${editRole.name}` : ''} onclose={() => (editRole = null)}>
	{#if editRole}
		<form class="space-y-4" onsubmit={saveEdit}>
			{#if !editRole.is_system}
				<div>
					<label class="label" for="role-name">Rol Adı</label>
					<input id="role-name" class="input" bind:value={roleName} required maxlength={40} />
				</div>
			{/if}
			<div>
				<p class="label">İzinler</p>
				<div class="max-h-72 space-y-1 overflow-y-auto rounded-lg border border-slate-200 p-2">
					{#each allPermissions as p (p.key)}
						<label class="flex min-h-11 items-center gap-3 rounded-md px-2 text-sm hover:bg-slate-50">
							<input
								type="checkbox"
								class="size-4 shrink-0 accent-indigo-600"
								checked={selectedPerms.has(p.key)}
								onchange={() => toggle(p.key)}
							/>
							<span class="flex-1 truncate">{p.description}</span>
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

<!-- Yeni rol -->
<Sheet bind:open={createOpen} title="Yeni Rol">
	<form class="space-y-4" onsubmit={saveCreate}>
		<div>
			<label class="label" for="new-role-name">Rol Adı</label>
			<input id="new-role-name" class="input" bind:value={roleName} placeholder="Örn. Kalite Ekibi" required maxlength={40} />
		</div>
		<div>
			<p class="label">İzinler</p>
			<div class="max-h-72 space-y-1 overflow-y-auto rounded-lg border border-slate-200 p-2">
				{#each allPermissions as p (p.key)}
					<label class="flex min-h-11 items-center gap-3 rounded-md px-2 text-sm hover:bg-slate-50">
						<input
							type="checkbox"
							class="size-4 shrink-0 accent-indigo-600"
							checked={selectedPerms.has(p.key)}
							onchange={() => toggle(p.key)}
						/>
						<span class="flex-1 truncate">{p.description}</span>
					</label>
				{/each}
			</div>
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (createOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !roleName.trim()}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>
