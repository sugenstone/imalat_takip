<script lang="ts">
	import { api } from '$lib/api/client';
	import type { WorkspaceSummary } from '$lib/api/types';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import NotificationsSheet from '$lib/components/NotificationsSheet.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { ws } from '$lib/stores/ws.svelte';

	let { children } = $props();

	let wsData = $state<WorkspaceSummary | null>(null);
	let loadError = $state('');

	// Bildirimler (Faz 9)
	let notifOpen = $state(false);
	let unread = $state(0);
	let notifKey = $state(0);

	async function refreshUnread() {
		try {
			const res = await api.get<{ unread: number }>(`/workspaces/${wid}/notifications/unread`);
			unread = res.unread;
		} catch {
			unread = 0;
		}
	}

	$effect(() => {
		wid;
		void refreshUnread();
		const t = setInterval(() => void refreshUnread(), 30000);
		return () => clearInterval(t);
	});

	const wid = $derived(page.params.id ?? '');
	const current = $derived(page.url.pathname);

	$effect(() => {
		wid;
		void load();
		void ws.load(wid);
	});

	async function load() {
		loadError = '';
		try {
			wsData = await api.get<WorkspaceSummary>(`/workspaces/${wid}`);
		} catch {
			loadError = 'Workspace bulunamadı veya erişiminiz yok';
		}
	}

	// Sekmeler rol/izin bazli: isci yalnizca Gorevler gorur.
	const allTabs = $derived([
		{
			href: `/w/${wid}`,
			label: 'Genel Bakış',
			icon: 'M3 12 12 3l9 9M5 10v10h14V10',
			exact: true,
			show: ws.isManager
		},
		{
			href: `/w/${wid}/yapi`,
			label: 'Yapı',
			icon: 'M3 7h6l2 3h10M3 17h6l2-3h10',
			exact: false,
			show: ws.can('section.create')
		},
		{
			href: `/w/${wid}/isler`,
			label: 'Görevler',
			icon: 'M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2m-6 9 2 2 4-4',
			exact: false,
			show: true
		},
		{
			href: `/w/${wid}/ayarlar`,
			label: 'Ayarlar',
			icon: 'M10.3 4.3a1 1 0 0 0 .9-.6l.5-1.4h2.6l.5 1.4a1 1 0 0 0 .9.6 1 1 0 0 1 .5 1.7l-1 1a1 1 0 0 0-.3.7v2.6a1 1 0 0 0 .3.7l1 1a1 1 0 0 1-.5 1.7 1 1 0 0 0-.9.6l-.5 1.4H11.7l-.5-1.4a1 1 0 0 0-.9-.6',
			exact: false,
			show:
				ws.can('user.invite') ||
				ws.can('role.manage') ||
				ws.can('team.manage') ||
				ws.can('workflow.create')
		}
	]);

	const tabs = $derived(allTabs.filter((t) => t.show));

	// Izinsiz rotaya elle girilirse gorev listesine yonlendir (403'e dusmeden)
	$effect(() => {
		if (!ws.loaded || loadError) return;
		const active = allTabs.find(
			(t) => current === t.href || (!t.exact && current.startsWith(t.href))
		);
		if (active && !active.show) {
			void goto(`/w/${wid}/isler`);
		}
	});
</script>

{#if loadError}
	<div class="flex min-dvh flex-col items-center justify-center px-6 text-center">
		<div class="flex size-14 items-center justify-center rounded-2xl bg-slate-100 text-slate-400"><svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg></div>
		<h1 class="mt-3 text-lg font-semibold">{loadError}</h1>
		<button type="button" class="btn-secondary mt-4" onclick={() => goto('/')}>Workspaces'e Dön</button>
	</div>
{:else}
	<div class="mx-auto min-dvh w-full max-w-4xl px-4 pb-28 pt-safe sm:px-6 md:pb-10">
		<header class="flex items-center gap-3 py-4">
			<a
				href="/"
				class="flex size-11 shrink-0 items-center justify-center rounded-lg text-slate-400 transition-colors hover:bg-white/10 hover:text-white"
				aria-label="Geri"
			>
				<Icon name="arrow-left" size={20} />
			</a>
			<div class="min-w-0 flex-1">
				<h1 class="truncate text-lg font-bold">{wsData?.name ?? '…'}</h1>
				{#if wsData}
					<div class="flex items-center gap-2">
						<span class="badge bg-indigo-50 text-indigo-700">{wsData.role_name}</span>
						{#if wsData.status === 'archived'}
							<span class="badge bg-slate-100 text-slate-600">Arşivlenmiş</span>
						{/if}
					</div>
				{/if}
			</div>
			<button
				type="button"
				class="relative flex size-11 shrink-0 items-center justify-center rounded-xl text-slate-300 transition-colors hover:bg-white/10 hover:text-white"
				onclick={() => { notifOpen = true; notifKey++; }}
				aria-label="Bildirimler"
			>
				<Icon name="bell" size={22} />
				{#if unread > 0}
					<span class="absolute top-1.5 right-1.5 flex min-w-4 items-center justify-center rounded-full bg-red-500 px-1 text-[10px] font-bold text-white ring-2 ring-slate-900">
						{unread > 9 ? '9+' : unread}
					</span>
				{/if}
			</button>
		</header>

		<NotificationsSheet bind:open={notifOpen} {wid} refreshKey={notifKey} onRead={refreshUnread} />

		<!-- Desktop: yatay sekmeler (isci tek sekmede gormez) -->
		{#if tabs.length > 1}
			<nav class="mb-4 hidden gap-1 rounded-xl bg-white p-1 shadow-sm ring-1 ring-slate-200 md:flex">
				{#each tabs as tab (tab.href)}
					<a
						href={tab.href}
						class="flex min-h-11 flex-1 items-center justify-center gap-2 rounded-lg px-4 text-sm font-medium transition-colors
							{current === tab.href || (!tab.exact && current.startsWith(tab.href)) ? 'bg-indigo-600 text-white' : 'text-slate-600 hover:bg-slate-100'}"
					>
						<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
							<path stroke-linecap="round" stroke-linejoin="round" d={tab.icon} />
						</svg>
						{tab.label}
					</a>
				{/each}
			</nav>
		{/if}

		{@render children()}
	</div>

	<!-- Mobil: alt tab bar (isci tek sekmede gormez) -->
	{#if tabs.length > 1}
		<nav class="fixed inset-x-0 bottom-0 z-40 border-t border-slate-200 bg-white/95 backdrop-blur pb-safe md:hidden">
			<div class="mx-auto flex max-w-md">
				{#each tabs as tab (tab.href)}
					<a
						href={tab.href}
						class="flex flex-1 flex-col items-center gap-1 px-2 pt-2 pb-1 text-xs font-medium
							{current === tab.href || (!tab.exact && current.startsWith(tab.href)) ? 'text-indigo-600' : 'text-slate-500'}"
					>
						<svg class="size-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.8">
							<path stroke-linecap="round" stroke-linejoin="round" d={tab.icon} />
						</svg>
						{tab.label}
					</a>
				{/each}
			</div>
		</nav>
	{/if}
{/if}
