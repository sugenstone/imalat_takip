<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Notification } from '$lib/api/types';
	import { notificationIcon } from '$lib/api/types';
	import Sheet from '$lib/components/Sheet.svelte';

	let {
		wid,
		open = $bindable(false),
		refreshKey = 0,
		onRead
	}: {
		wid: string;
		open?: boolean;
		refreshKey?: number;
		onRead?: () => void;
	} = $props();

	let items = $state<Notification[]>([]);
	let loading = $state(true);
	let error = $state('');

	$effect(() => {
		wid;
		refreshKey;
		if (open) void load();
	});

	async function load() {
		loading = true;
		error = '';
		try {
			items = await api.get<Notification[]>(`/workspaces/${wid}/notifications`);
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Yüklenemedi';
		} finally {
			loading = false;
		}
	}

	async function markRead(n: Notification) {
		if (n.read_at) return;
		try {
			await api.post(`/workspaces/${wid}/notifications/${n.id}/read`, undefined);
			n.read_at = new Date().toISOString();
			onRead?.();
		} catch {
			/* sessiz */
		}
	}

	async function markAll() {
		try {
			await api.post(`/workspaces/${wid}/notifications/read-all`, undefined);
			await load();
			onRead?.();
		} catch {
			/* sessiz */
		}
	}

	function timeAgo(iso: string): string {
		const d = new Date(iso).getTime();
		const min = Math.floor((Date.now() - d) / 60000);
		if (min < 1) return 'şimdi';
		if (min < 60) return `${min} dk önce`;
		const h = Math.floor(min / 60);
		if (h < 24) return `${h} sa önce`;
		return new Date(iso).toLocaleDateString('tr-TR');
	}
</script>

<Sheet bind:open title="Bildirimler">
	<div class="space-y-3">
		{#if items.length > 0}
			<button type="button" class="w-full text-sm font-medium text-indigo-600" onclick={markAll}>
				Tümünü okundu işaretle
			</button>
		{/if}

		{#if loading}
			<div class="flex justify-center py-8">
				<div class="size-6 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
			</div>
		{:else if items.length === 0}
			<p class="py-8 text-center text-sm text-slate-400">Bildirim yok.</p>
		{:else}
			<div class="space-y-2">
				{#each items as n (n.id)}
					<button
						type="button"
						class="flex w-full items-start gap-3 rounded-xl border p-3 text-left transition-colors
							{n.read_at ? 'border-slate-200 bg-white' : 'border-indigo-200 bg-indigo-50/50'}"
						onclick={() => markRead(n)}
					>
						<span class="text-xl" aria-hidden="true">{notificationIcon(n.type)}</span>
						<span class="min-w-0 flex-1">
							<span class="flex items-center justify-between gap-2">
								<span class="truncate text-sm font-semibold">{n.title}</span>
								<span class="shrink-0 text-xs text-slate-400">{timeAgo(n.created_at)}</span>
							</span>
							<span class="mt-0.5 block text-xs leading-relaxed text-slate-600">{n.message}</span>
						</span>
						{#if !n.read_at}
							<span class="mt-1 size-2.5 shrink-0 rounded-full bg-indigo-500"></span>
						{/if}
					</button>
				{/each}
			</div>
		{/if}

		{#if error}
			<p class="form-error" role="alert">{error}</p>
		{/if}
	</div>
</Sheet>
