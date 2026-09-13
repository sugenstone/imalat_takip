<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { WorkspaceSummary } from '$lib/api/types';
	import { auth } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import Sheet from '$lib/components/Sheet.svelte';

	let workspaces = $state<WorkspaceSummary[]>([]);
	let loading = $state(true);
	let createOpen = $state(false);
	let newName = $state('');
	let error = $state('');
	let busy = $state(false);

	$effect(() => {
		void load();
	});

	async function load() {
		loading = true;
		try {
			workspaces = await api.get<WorkspaceSummary[]>('/workspaces');
		} catch {
			workspaces = [];
		} finally {
			loading = false;
		}
	}

	async function createWorkspace(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const ws = await api.post<WorkspaceSummary>('/workspaces', { name: newName });
			createOpen = false;
			newName = '';
			await goto(`/w/${ws.id}`);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Workspace oluşturulamadı';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Workspaces</title></svelte:head>

<div class="mx-auto min-dvh w-full max-w-3xl px-4 pb-24 pt-safe sm:px-6">
	<header class="flex items-center justify-between py-4">
		<div>
			<h1 class="text-xl font-bold">Workspaces</h1>
			<p class="text-sm text-slate-500">{auth.user?.name}</p>
		</div>
		<button
			type="button"
			class="btn-secondary"
			onclick={async () => {
				await auth.logout();
				await goto('/giris');
			}}
		>
			Çıkış
		</button>
	</header>

	{#if loading}
		<div class="flex justify-center py-16">
			<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
		</div>
	{:else if workspaces.length === 0}
		<div class="card mt-8">
			<EmptyState
				icon="folder"
				title="Henüz workspace yok"
				description="İlk workspace'inizi oluşturarak bölüm ağacınızı kurmaya başlayın."
				actionLabel="Workspace Oluştur"
				onaction={() => (createOpen = true)}
			/>
		</div>
	{:else}
		<div class="mt-4 grid gap-3 sm:grid-cols-2">
			{#each workspaces as ws (ws.id)}
				<button
					type="button"
					class="card flex min-h-11 items-center justify-between text-left transition-shadow hover:shadow-md active:shadow-sm"
					onclick={() => goto(`/w/${ws.id}`)}
				>
					<div class="min-w-0">
						<div class="flex items-center gap-2">
							<span class="truncate font-semibold">{ws.name}</span>
							{#if ws.status === 'archived'}
								<span class="badge bg-slate-100 text-slate-600">Arşiv</span>
							{/if}
						</div>
						<p class="truncate text-sm text-slate-500">{ws.role_name ?? '-'}</p>
					</div>
					<svg class="size-5 shrink-0 text-slate-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
						<path stroke-linecap="round" stroke-linejoin="round" d="m9 5 7 7-7 7" />
					</svg>
				</button>
			{/each}
		</div>
	{/if}

	<!-- Mobil: altta sabit ekle butonu -->
	<button
		type="button"
		class="btn-primary fixed inset-x-4 bottom-4 z-40 shadow-lg sm:static sm:inset-auto sm:mt-6 sm:w-auto"
		onclick={() => (createOpen = true)}
	>
		+ Yeni Workspace
	</button>
</div>

<Sheet bind:open={createOpen} title="Yeni Workspace">
	<form class="space-y-4" onsubmit={createWorkspace}>
		<div>
			<label class="label" for="ws-name">Workspace Adı</label>
			<input id="ws-name" class="input" bind:value={newName} placeholder="Örn. Atölye Merkez" required maxlength={80} />
		</div>
		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
		<div class="flex gap-2">
			<button type="button" class="btn-secondary flex-1" onclick={() => (createOpen = false)}>İptal</button>
			<button type="submit" class="btn-primary flex-1" disabled={busy || !newName.trim()}>
				{busy ? 'Oluşturuluyor…' : 'Oluştur'}
			</button>
		</div>
	</form>
</Sheet>
