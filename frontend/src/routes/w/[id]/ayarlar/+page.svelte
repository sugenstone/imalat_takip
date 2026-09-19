<script lang="ts">
	import { api } from '$lib/api/client';
	import type { Member, Role, Section, Team, WorkType, WorkflowTemplate, StepDefinition } from '$lib/api/types';
	import { page } from '$app/state';
	import MembersPanel from '$lib/components/settings/MembersPanel.svelte';
	import RolesPanel from '$lib/components/settings/RolesPanel.svelte';
	import TeamsPanel from '$lib/components/settings/TeamsPanel.svelte';
	import WorkTypesPanel from '$lib/components/settings/WorkTypesPanel.svelte';
	import WorkflowsPanel from '$lib/components/settings/WorkflowsPanel.svelte';
	import StepsPanel from '$lib/components/settings/StepsPanel.svelte';
	import AuditPanel from '$lib/components/settings/AuditPanel.svelte';
	import EmailsPanel from '$lib/components/settings/EmailsPanel.svelte';

	let tab = $state<'uyeler' | 'adimlar' | 'gruplar' | 'roller' | 'takimlar' | 'is-tipleri' | 'aktivite' | 'eposta'>('adimlar');
	let members = $state<Member[]>([]);
	let roles = $state<Role[]>([]);
	let teams = $state<Team[]>([]);
	let sections = $state<Section[]>([]);
	let workTypes = $state<WorkType[]>([]);
	let workflows = $state<WorkflowTemplate[]>([]);
	let steps = $state<StepDefinition[]>([]);
	let loading = $state(true);

	const wid = $derived(page.params.id ?? '');

	$effect(() => {
		wid;
		void load();
	});

	async function load() {
		loading = true;
		try {
			const [m, r, t, sec, wt, wfl, st] = await Promise.all([
				api.get<Member[]>(`/workspaces/${wid}/members`),
				api.get<Role[]>(`/workspaces/${wid}/roles`),
				api.get<Team[]>(`/workspaces/${wid}/teams`),
				api.get<Section[]>(`/workspaces/${wid}/sections`),
				api.get<WorkType[]>(`/workspaces/${wid}/work-types`),
				api.get<WorkflowTemplate[]>(`/workspaces/${wid}/workflows`),
				api.get<StepDefinition[]>(`/workspaces/${wid}/steps`)
			]);
			members = m;
			roles = r;
			teams = t;
			sections = sec;
			workTypes = wt;
			workflows = wfl;
			steps = st;
		} catch {
			/* layout hatayi gosterir */
		} finally {
			loading = false;
		}
	}

	const tabs = [
		{ id: 'uyeler', label: 'Üyeler' },
		{ id: 'adimlar', label: 'Adımlar' },
		{ id: 'gruplar', label: 'Süreç Grupları' },
		{ id: 'roller', label: 'Roller' },
		{ id: 'takimlar', label: 'Takımlar' },
		{ id: 'is-tipleri', label: 'İş Tipleri' },
		{ id: 'aktivite', label: 'Aktivite' },
		{ id: 'eposta', label: 'E-posta' }
	] as const;
</script>

<svelte:head><title>Ayarlar</title></svelte:head>

{#if loading}
	<div class="flex justify-center py-16">
		<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else}
	<!-- Segment kontrol - mobil dostu (yatay kaydiralabilir) -->
	<div class="mb-4 flex gap-1 overflow-x-auto rounded-xl bg-white p-1 shadow-sm ring-1 ring-slate-200">
		{#each tabs as t (t.id)}
			<button
				type="button"
				class="flex min-h-11 shrink-0 items-center justify-center rounded-lg px-3 text-sm font-medium transition-colors
					{tab === t.id ? 'bg-indigo-600 text-white' : 'text-slate-600 hover:bg-slate-100'}"
				onclick={() => (tab = t.id)}
			>
				{t.label}
			</button>
		{/each}
	</div>

	{#if tab === 'uyeler'}
		<MembersPanel {wid} {members} {roles} {sections} reload={load} />
	{:else if tab === 'adimlar'}
		<StepsPanel {wid} {steps} {members} {teams} {roles} reload={load} />
	{:else if tab === 'roller'}
		<RolesPanel {wid} {roles} reload={load} />
	{:else if tab === 'takimlar'}
		<TeamsPanel {wid} {teams} {members} reload={load} />
	{:else if tab === 'is-tipleri'}
		<WorkTypesPanel {wid} {workTypes} reload={load} />
	{:else if tab === 'gruplar'}
		<WorkflowsPanel {wid} {workflows} {steps} reload={load} />
	{:else if tab === 'aktivite'}
		<AuditPanel {wid} />
	{:else}
		<EmailsPanel {wid} />
	{/if}
{/if}

