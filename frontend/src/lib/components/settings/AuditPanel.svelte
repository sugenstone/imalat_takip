<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { AuditEntry } from '$lib/api/types';
	import { auditLabel, auditIcon } from '$lib/api/types';

	let { wid }: { wid: string } = $props();

	let entries = $state<AuditEntry[]>([]);
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
			entries = await api.get<AuditEntry[]>(`/workspaces/${wid}/audit?limit=100`);
		} catch (err) {
			denied = err instanceof ApiError && err.status === 403;
			entries = [];
		} finally {
			loading = false;
		}
	}

	function metaOf(e: AuditEntry): string {
		if (!e.metadata_json) return '';
		try {
			const m = JSON.parse(e.metadata_json);
			const parts: string[] = [];
			if (m.reason) parts.push(`sebep: ${m.reason}`);
			if (m.version !== undefined) parts.push(`v${m.version}`);
			if (m.email) parts.push(m.email);
			if (m.affected !== undefined) parts.push(`${m.affected} adım etkilendi`);
			return parts.join(' · ');
		} catch {
			return '';
		}
	}

	function timeAgo(iso: string): string {
		const d = new Date(iso).getTime();
		const diff = Date.now() - d;
		const min = Math.floor(diff / 60000);
		if (min < 1) return 'şimdi';
		if (min < 60) return `${min} dk önce`;
		const h = Math.floor(min / 60);
		if (h < 24) return `${h} sa önce`;
		return new Date(iso).toLocaleDateString('tr-TR', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
	}
</script>

<h2 class="mb-3 font-semibold">Aktivite</h2>

{#if loading}
	<div class="flex justify-center py-12">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if denied}
	<div class="card py-8 text-center text-sm text-slate-500">
		Aktivite geçmişini yalnızca yöneticiler görüntüleyebilir.
	</div>
{:else if entries.length === 0}
	<div class="card py-8 text-center text-sm text-slate-400">Henüz kayıt yok.</div>
{:else}
	<p class="mb-2 text-xs text-slate-400">Kayıtlar değiştirilemez (append-only).</p>
	<div class="space-y-2">
		{#each entries as e (e.id)}
			<div class="card flex items-start gap-3 !p-3">
				<span class="text-xl" aria-hidden="true">{auditIcon(e.action)}</span>
				<div class="min-w-0 flex-1">
					<p class="text-sm">
						<strong>{e.actor_name ?? 'Sistem'}</strong>
						{auditLabel(e.action)}
					</p>
					{#if metaOf(e)}
						<p class="truncate text-xs text-slate-500">{metaOf(e)}</p>
					{/if}
				</div>
				<span class="shrink-0 text-xs text-slate-400">{timeAgo(e.created_at)}</span>
			</div>
		{/each}
	</div>
{/if}
