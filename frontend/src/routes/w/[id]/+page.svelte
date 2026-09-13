<script lang="ts">
	import { api } from '$lib/api/client';
	import type { Dashboard } from '$lib/api/types';
	import { fmtDuration } from '$lib/api/types';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';

	let dash = $state<Dashboard | null>(null);
	let loading = $state(true);

	const wid = $derived(page.params.id ?? '');

	$effect(() => {
		wid;
		void load();
	});

	async function load() {
		loading = true;
		try {
			dash = await api.get<Dashboard>(`/workspaces/${wid}/reports/dashboard`);
		} catch {
			dash = null;
		} finally {
			loading = false;
		}
	}

	const maxFail = $derived(
		dash ? Math.max(1, ...dash.top_failed_processes.map((t) => t.cnt)) : 1
	);
	const maxDur = $derived(
		dash ? Math.max(1, ...dash.process_durations.map((t) => t.avg_seconds)) : 1
	);

	const METRICS = $derived.by(() => {
		if (!dash) return [];
		return [
			{ icon: 'clipboard', tint: 'bg-indigo-50 text-indigo-600', value: dash.active_items, label: 'Aktif İş Kalemi', alert: false },
			{ icon: 'check-circle', tint: 'bg-emerald-50 text-emerald-600', value: dash.completed_items, label: 'Tamamlanan İş', alert: false },
			{ icon: 'clock', tint: 'bg-red-50 text-red-600', value: dash.overdue_items, label: 'Geciken İş', alert: dash.overdue_items > 0 },
			{ icon: 'users', tint: 'bg-slate-100 text-slate-600', value: dash.member_count, label: 'Üye', alert: false },
			{ icon: 'zap', tint: 'bg-blue-50 text-blue-600', value: dash.ready_processes, label: 'Hazır Süreç', alert: false },
			{ icon: 'activity', tint: 'bg-amber-50 text-amber-600', value: dash.in_progress_processes, label: 'Devam Eden Süreç', alert: false },
			{ icon: 'timer', tint: 'bg-violet-50 text-violet-600', value: dash.pending_approval_processes, label: 'Onay Bekleyen', alert: dash.pending_approval_processes > 0 },
			{ icon: 'rotate', tint: 'bg-orange-50 text-orange-600', value: dash.rework_count, label: 'Rework', alert: dash.rework_count > 0 }
		];
	});
</script>

<svelte:head><title>Genel Bakış</title></svelte:head>

{#if loading}
	<div class="flex justify-center py-16">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else if dash}
	<!-- Metrik kartlari -->
	<div class="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
		{#each METRICS as m (m.label)}
			<button
				type="button"
				class="flex items-center gap-3 rounded-2xl bg-white p-3.5 text-left shadow-sm ring-1 ring-slate-200 transition-all hover:-translate-y-0.5 hover:shadow-md {m.alert ? 'ring-2 ring-red-200' : ''}"
				onclick={() => goto(`/w/${wid}/isler`)}
			>
				<div class="flex size-10 shrink-0 items-center justify-center rounded-xl {m.tint}">
					<Icon name={m.icon} size={19} />
				</div>
				<div class="min-w-0">
					<p class="text-xl leading-tight font-bold {m.alert ? 'text-red-600' : 'text-slate-800'}">{m.value}</p>
					<p class="truncate text-[11px] font-medium text-slate-500">{m.label}</p>
				</div>
			</button>
		{/each}
	</div>

	<!-- Kalite ozeti -->
	<div class="mt-3 rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-200">
		<div class="mb-3 flex items-center gap-2">
			<Icon name="target" size={16} class="text-slate-500" />
			<h2 class="text-sm font-semibold text-slate-700">Kalite Özeti</h2>
		</div>
		<div class="mb-1.5 flex items-baseline justify-between">
			<span class="text-sm text-slate-600">İlk Seferde Başarı</span>
			<span class="text-2xl font-bold {dash.first_pass_success_rate !== null && dash.first_pass_success_rate < 100 ? 'text-amber-600' : 'text-emerald-600'}">
				{dash.first_pass_success_rate !== null ? `%${dash.first_pass_success_rate}` : '—'}
			</span>
		</div>
		<div class="h-2.5 overflow-hidden rounded-full bg-slate-100">
			<div
				class="h-full rounded-full {dash.first_pass_success_rate !== null && dash.first_pass_success_rate < 100 ? 'bg-amber-500' : 'bg-emerald-500'}"
				style="width: {dash.first_pass_success_rate ?? 0}%"
			></div>
		</div>
		<div class="mt-3 grid grid-cols-2 gap-2 text-center">
			<div class="rounded-xl bg-slate-50 py-2.5">
				<p class="flex items-center justify-center gap-1 text-[11px] text-slate-500"><Icon name="trending" size={12} /> Ort. Cycle Time</p>
				<p class="text-sm font-bold text-slate-800">
					{dash.avg_cycle_time_hours !== null ? `${dash.avg_cycle_time_hours.toFixed(1)} sa` : '—'}
				</p>
			</div>
			<div class="rounded-xl bg-slate-50 py-2.5">
				<p class="flex items-center justify-center gap-1 text-[11px] text-slate-500"><Icon name="rotate" size={12} /> Rework Oranı</p>
				<p class="text-sm font-bold text-slate-800">{dash.rework_rate !== null ? `%${dash.rework_rate}` : '—'}</p>
			</div>
		</div>
	</div>

	<!-- En cok hata cikan surecler -->
	{#if dash.top_failed_processes.length > 0}
		<div class="mt-3 rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-200">
			<div class="mb-3 flex items-center gap-2">
				<Icon name="alert" size={16} class="text-red-500" />
				<h2 class="text-sm font-semibold text-slate-700">En Çok Hata Çıkan Süreçler</h2>
			</div>
			<div class="space-y-2">
				{#each dash.top_failed_processes as t (t.name)}
					<div class="flex items-center gap-2">
						<span class="w-24 shrink-0 truncate text-sm text-slate-700">{t.name}</span>
						<div class="h-4 flex-1 overflow-hidden rounded bg-slate-100">
							<div class="h-full rounded bg-red-400/80" style="width: {(t.cnt / maxFail) * 100}%"></div>
						</div>
						<span class="w-6 shrink-0 text-right text-sm font-semibold text-red-600">{t.cnt}</span>
					</div>
				{/each}
			</div>
		</div>
	{/if}

	<!-- Surec sureleri (darbogaz) -->
	{#if dash.process_durations.length > 0}
		<div class="mt-3 rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-200">
			<div class="mb-3 flex items-center gap-2">
				<Icon name="timer" size={16} class="text-indigo-500" />
				<h2 class="text-sm font-semibold text-slate-700">Süreç Süreleri <span class="text-xs font-normal text-slate-400">(ortalama)</span></h2>
			</div>
			<div class="space-y-2">
				{#each dash.process_durations as t (t.name)}
					<div class="flex items-center gap-2">
						<span class="w-24 shrink-0 truncate text-sm text-slate-700">{t.name}</span>
						<div class="h-4 flex-1 overflow-hidden rounded bg-slate-100">
							<div class="h-full rounded bg-indigo-400/80" style="width: {(t.avg_seconds / maxDur) * 100}%"></div>
						</div>
						<span class="shrink-0 text-xs font-semibold text-slate-600">{fmtDuration(t.avg_seconds)}</span>
					</div>
				{/each}
			</div>
		</div>
	{/if}
{/if}
