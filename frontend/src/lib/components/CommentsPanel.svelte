<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Comment } from '$lib/api/types';
	import { auth } from '$lib/stores/auth.svelte';

	let {
		wid,
		entityType,
		entityId
	}: {
		wid: string;
		entityType: string;
		entityId: string;
	} = $props();

	let comments = $state<Comment[]>([]);
	let loading = $state(true);
	let body = $state('');
	let busy = $state(false);
	let error = $state('');
	let editing = $state<Comment | null>(null);
	let editBody = $state('');

	$effect(() => {
		entityId;
		void load();
	});

	async function load() {
		loading = true;
		try {
			comments = await api.get<Comment[]>(
				`/workspaces/${wid}/comments?entity_type=${entityType}&entity_id=${entityId}`
			);
		} catch {
			comments = [];
		} finally {
			loading = false;
		}
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!body.trim()) return;
		error = '';
		busy = true;
		try {
			await api.post(`/workspaces/${wid}/comments`, {
				entity_type: entityType,
				entity_id: entityId,
				body: body.trim()
			});
			body = '';
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Gönderilemedi';
		} finally {
			busy = false;
		}
	}

	function startEdit(c: Comment) {
		editing = c;
		editBody = c.body;
	}

	async function saveEdit(e: SubmitEvent) {
		e.preventDefault();
		if (!editing || !editBody.trim()) return;
		error = '';
		try {
			await api.patch(`/workspaces/${wid}/comments/${editing.id}`, {
				body: editBody.trim()
			});
			editing = null;
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Kaydedilemedi';
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
		return new Date(iso).toLocaleDateString('tr-TR');
	}
</script>

{#if loading}
	<div class="flex justify-center py-8">
		<div class="size-6 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else}
	<div class="space-y-3">
		{#if comments.length === 0}
			<p class="py-6 text-center text-sm text-slate-400">Henüz yorum yok.</p>
		{:else}
			{#each comments as c (c.id)}
				<div class="card !p-3">
					{#if editing?.id === c.id}
						<form class="space-y-2" onsubmit={saveEdit}>
							<textarea class="input min-h-16" rows="2" bind:value={editBody}></textarea>
							<div class="flex gap-2">
								<button type="submit" class="btn-primary !min-h-9 !px-3 !text-xs">Kaydet</button>
								<button type="button" class="btn-secondary !min-h-9 !px-3 !text-xs" onclick={() => (editing = null)}>İptal</button>
							</div>
						</form>
					{:else}
						<div class="flex items-center gap-2">
							<span class="flex size-7 shrink-0 items-center justify-center rounded-full bg-indigo-100 text-xs font-bold text-indigo-700">
								{c.author_name.charAt(0).toUpperCase()}
							</span>
							<p class="min-w-0 flex-1 truncate text-sm font-medium">{c.author_name}</p>
							<span class="shrink-0 text-xs text-slate-400">
								{timeAgo(c.created_at)}{c.edited_at ? ' · düzenlendi' : ''}
							</span>
						</div>
						<p class="mt-1.5 pl-9 text-sm leading-relaxed text-slate-700">{c.body}</p>
						{#if c.created_by === auth.user?.id}
							<button type="button" class="ml-9 mt-1 text-xs text-indigo-600" onclick={() => startEdit(c)}>
								düzenle
							</button>
						{/if}
					{/if}
				</div>
			{/each}
		{/if}
	</div>
{/if}

<!-- Yorum girisi (mobilde alt kisimda) -->
<form class="sticky bottom-0 mt-4 flex gap-2 rounded-xl bg-white p-2 shadow-lg ring-1 ring-slate-200" onsubmit={submit}>
	<input
		class="input flex-1"
		placeholder="Yorum yazın…"
		bind:value={body}
		maxlength={4000}
	/>
	<button type="submit" class="btn-primary !min-h-11 shrink-0 !px-4" disabled={busy || !body.trim()}>
		Gönder
	</button>
</form>

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}
