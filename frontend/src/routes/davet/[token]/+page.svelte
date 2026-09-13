<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { InviteInfo, User } from '$lib/api/types';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/stores/auth.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let info = $state<InviteInfo | null>(null);
	let loading = $state(true);
	let errorMsg = $state('');
	let name = $state('');
	let password = $state('');
	let busy = $state(false);

	const token = $derived(page.params.token ?? '');

	$effect(() => {
		token;
		void load();
	});

	async function load() {
		loading = true;
		errorMsg = '';
		try {
			info = await api.get<InviteInfo>(`/invites/${token}`);
		} catch (err) {
			errorMsg = err instanceof ApiError ? err.message : 'Davet yüklenemedi';
		} finally {
			loading = false;
		}
	}

	async function accept(e: SubmitEvent) {
		e.preventDefault();
		if (!password.trim()) return;
		busy = true;
		errorMsg = '';
		try {
			const res = await api.post<{ user: User; workspace_id: string }>(
				`/invites/${token}/accept`,
				{ name: name.trim(), password }
			);
			auth.setUser(res.user);
			await goto(`/w/${res.workspace_id}`);
		} catch (err) {
			errorMsg = err instanceof ApiError ? err.message : 'Kabul edilemedi';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Davet</title></svelte:head>

<div class="flex min-dvh flex-col justify-center px-4 py-8">
	<div class="mx-auto w-full max-w-sm">
		{#if loading}
			<div class="flex justify-center py-16">
				<div class="size-8 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
			</div>
		{:else if errorMsg}
			<!-- Gecersiz / sure dolmus / kullanilmis davet -->
			<div class="card text-center">
				<div class="mx-auto mb-3 flex size-14 items-center justify-center rounded-2xl bg-red-50 text-red-500">
					<Icon name="x-circle" size={28} />
				</div>
				<h1 class="text-lg font-bold">Davet açılamadı</h1>
				<p class="mt-1 text-sm text-slate-500">{errorMsg}</p>
				<a href="/giris" class="btn-secondary mt-4 inline-flex">Giriş Sayfasına Git</a>
			</div>
		{:else if info}
			<div class="mb-6 text-center">
				<div class="mx-auto mb-3 flex size-14 items-center justify-center rounded-2xl bg-indigo-600 text-white shadow-lg">
					<Icon name="mail" size={26} />
				</div>
				<h1 class="text-xl font-bold">Workspace Daveti</h1>
				<p class="mt-1 text-sm text-slate-500">
					<strong>{info.inviter_name}</strong> sizi
					<strong>“{info.workspace_name}”</strong> workspace'ine
					<span class="text-indigo-600 font-medium">{info.role_name}</span> rolüyle davet etti
				</p>
				<p class="mt-1 text-xs text-slate-400">{info.email}</p>
			</div>

			<form class="card space-y-4" onsubmit={accept}>
				{#if info.existing_account}
					<p class="rounded-lg bg-emerald-50 px-3 py-2 text-xs text-emerald-800">
						Bu e-posta ile hesabınız var — şifrenizle giriş yaparak workspace'e katılın.
					</p>
				{:else}
					<p class="rounded-lg bg-indigo-50 px-3 py-2 text-xs text-indigo-800">
						Hesabınızı oluşturun; workspace'e otomatik katılacaksınız.
					</p>
				{/if}

				{#if !info.existing_account}
					<div>
						<label class="label" for="acc-name">Ad Soyad</label>
						<input id="acc-name" class="input" bind:value={name} placeholder="Adınız Soyadınız" required maxlength={80} />
					</div>
				{/if}
				<div>
					<label class="label" for="acc-pass">Şifre</label>
					<input
						id="acc-pass"
						class="input"
						type="password"
						bind:value={password}
						placeholder={info.existing_account ? 'Mevcut şifreniz' : 'En az 8 karakter'}
						required
						minlength={info.existing_account ? 1 : 8}
					/>
				</div>

				{#if errorMsg}
					<p class="form-error" role="alert">{errorMsg}</p>
				{/if}

				<button type="submit" class="btn-primary w-full" disabled={busy || !password}>
					{busy ? 'İşleniyor…' : info.existing_account ? 'Katıl ve Giriş Yap' : 'Hesap Oluştur ve Katıl'}
				</button>
				<p class="text-center text-xs text-slate-400">
					Davet {new Date(info.expires_at).toLocaleDateString('tr-TR')} tarihine kadar geçerli
				</p>
			</form>
		{/if}
	</div>
</div>
