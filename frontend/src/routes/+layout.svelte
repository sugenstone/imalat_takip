<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { auth } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';

	let { children } = $props();

	// Oturum kontrolu - sayfa ilk acildiginda
	$effect(() => {
		void auth.init();
	});

	// Giris gerektiren sayfalarda oturum yoksa yonlendir
	const publicPaths = ['/giris', '/kayit', '/davet'];

	$effect(() => {
		if (!auth.loaded) return;
		const path = page.url.pathname;
		const isPublic = publicPaths.some((p) => path.startsWith(p));
		if (!auth.isLoggedIn && !isPublic) {
			void goto('/giris');
		} else if (auth.isLoggedIn && isPublic && !path.startsWith('/davet')) {
			// Davet sayfasi oturumlu kullanici icin de erisilebilir kalmali
			void goto('/');
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<meta name="description" content="Operasyon takip platformu" />
</svelte:head>

{@render children()}
