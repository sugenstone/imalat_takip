<script lang="ts">
	// Ince ilerleme cubugu — kurumsal kart dili.
	let {
		value,
		total,
		label = true,
		color = 'auto'
	}: {
		value: number;
		total: number;
		label?: boolean;
		/** auto: %100 emerald, >0 indigo, 0 slate */
		color?: 'auto' | 'indigo' | 'emerald' | 'amber' | 'red' | 'slate';
	} = $props();

	const pct = $derived(total > 0 ? Math.round((value / total) * 100) : 0);
	const barColor = $derived.by(() => {
		if (color !== 'auto') return `bg-${color}-500`;
		if (total === 0) return 'bg-slate-300';
		if (pct === 100) return 'bg-emerald-500';
		return 'bg-indigo-500';
	});
</script>

<div class="flex items-center gap-2">
	<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-slate-200/80">
		<div class="h-full rounded-full transition-all {barColor}" style="width: {pct}%"></div>
	</div>
	{#if label}
		<span class="w-8 shrink-0 text-right text-[11px] font-semibold tabular-nums text-slate-500">{pct}%</span>
	{/if}
</div>
