<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { OutboxEntry } from '$lib/api/types';

	let { wid }: { wid: string } = $props();

	let entries = $state<OutboxEntry[]>([]);
	let loading = $state(true);
	let denied = $state(false);

	$effect(() => {
		wid;
		void load();
	});

	async function load() {
		loading = true;
		denied = false;
		try {
			entries = await api.get<OutboxEntry[]>(`/workspaces/${wid}/notifications/emails`);
		} catch (err) {
			denied = err instanceof ApiError && err.status === 403;
		} finally {
			loading = false;
		}
	}

	function timeAgo(iso: string): string {
		const min = Math.floor((Date.now() - new Date(iso).getTime()) / 60000);
		if (min < 1) return 'şimdi';
		if (min < 60) return `${min} dk önce`;
		return new Date(iso).toLocaleString('tr-TR', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
	}
</script>

<h2 class="mb-3 font-semibold">E-posta Kuyruğu</h2>

{#if loading}
	<div class="flex justify-center py-12">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if denied}
	<div class="card py-8 text-center text-sm text-slate-500">
		E-posta kuyruğunu yalnızca yöneticiler görüntüleyebilir.
	</div>
{:else if entries.length === 0}
	<div class="card py-8 text-center text-sm text-slate-400">Kuyrukta e-posta yok.</div>
{:else}
	<p class="mb-2 text-xs text-slate-400">
		E-postalar istek içinde değil, arka plandaki kuyruktan işlenir.
	</p>
	<div class="card divide-y divide-slate-100 !p-0">
		{#each entries as e (e.id)}
			<div class="px-4 py-3">
				<div class="flex items-center justify-between gap-2">
					<p class="min-w-0 truncate text-sm font-medium">{e.subject}</p>
					<span
						class="badge shrink-0
						{e.status === 'sent' ? 'bg-emerald-50 text-emerald-700' : e.status === 'failed' ? 'bg-red-50 text-red-700' : 'bg-amber-50 text-amber-700'}"
					>
						{e.status === 'sent' ? 'Gönderildi' : e.status === 'failed' ? 'Hata' : 'Bekliyor'}
					</span>
				</div>
				<p class="truncate text-xs text-slate-500">
					› {e.to_email} · {timeAgo(e.created_at)}
					{#if e.sent_at}· teslim {timeAgo(e.sent_at)}{/if}
				</p>
			</div>
		{/each}
	</div>
{/if}
