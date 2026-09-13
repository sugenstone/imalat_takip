<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { User } from '$lib/api/types';
	import { auth } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';

	let email = $state('');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		busy = true;
		try {
			const user = await api.post<User>('/auth/login', { email, password });
			auth.setUser(user);
			await goto('/');
		} catch (err) {
			error =
				err instanceof ApiError && err.status === 401
					? 'E-posta veya şifre hatalı'
					: 'Giriş yapılamadı, tekrar deneyin';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Giriş Yap</title></svelte:head>

<div class="flex min-dvh flex-col justify-center px-4 py-8">
	<div class="mx-auto w-full max-w-sm">
		<div class="mb-8 text-center">
			<div class="mx-auto mb-3 flex size-14 items-center justify-center rounded-2xl bg-indigo-600 text-white shadow-lg">
				<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
			</div>
			<h1 class="text-2xl font-bold">Tekrar hoş geldiniz</h1>
			<p class="mt-1 text-sm text-slate-500">Devam etmek için giriş yapın</p>
		</div>

		<form class="card space-y-4" onsubmit={submit}>
			<div>
				<label class="label" for="email">E-posta</label>
				<input
					id="email"
					class="input"
					type="email"
					bind:value={email}
					placeholder="ornek@sirket.com"
					autocomplete="email"
					required
				/>
			</div>
			<div>
				<label class="label" for="password">Şifre</label>
				<input
					id="password"
					class="input"
					type="password"
					bind:value={password}
					placeholder="••••••••"
					autocomplete="current-password"
					required
				/>
			</div>

			{#if error}
				<p class="form-error" role="alert">{error}</p>
			{/if}

			<button type="submit" class="btn-primary w-full" disabled={busy}>
				{busy ? 'Giriş yapılıyor…' : 'Giriş Yap'}
			</button>
		</form>

		<p class="mt-6 text-center text-sm text-slate-600">
			Hesabınız yok mu?
			<a href="/kayit" class="font-semibold text-indigo-600 hover:text-indigo-700">Kayıt olun</a>
		</p>
	</div>
</div>
