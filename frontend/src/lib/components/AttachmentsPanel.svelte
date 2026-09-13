<script lang="ts">
	import { api, ApiError } from '$lib/api/client';
	import type { Attachment } from '$lib/api/types';
	import { fileIcon, fileSize } from '$lib/api/types';

	let {
		wid,
		entityType,
		entityId
	}: {
		wid: string;
		entityType: string;
		entityId: string;
	} = $props();

	let files = $state<Attachment[]>([]);
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	$effect(() => {
		entityId;
		void load();
	});

	async function load() {
		loading = true;
		try {
			files = await api.get<Attachment[]>(
				`/workspaces/${wid}/attachments?entity_type=${entityType}&entity_id=${entityId}`
			);
		} catch {
			files = [];
		} finally {
			loading = false;
		}
	}

	async function upload(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const f = input.files?.[0];
		if (!f) return;
		error = '';
		if (f.size > 10 * 1024 * 1024) {
			error = 'Dosya en fazla 10MB olabilir';
			input.value = '';
			return;
		}
		busy = true;
		try {
			const fd = new FormData();
			fd.append('entity_type', entityType);
			fd.append('entity_id', entityId);
			fd.append('file', f);
			const res = await fetch(`/api/workspaces/${wid}/attachments`, {
				method: 'POST',
				body: fd
			});
			if (!res.ok) {
				const err = (await res.json().catch(() => null)) as { error?: { message?: string } } | null;
				throw new Error(err?.error?.message ?? 'Yüklenemedi');
			}
			input.value = '';
			await load();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Yüklenemedi';
		} finally {
			busy = false;
		}
	}

	async function remove(a: Attachment) {
		if (!confirm(`${a.file_name} silinsin mi?`)) return;
		error = '';
		try {
			await api.del(`/workspaces/${wid}/attachments/${a.id}`);
			await load();
		} catch (err) {
			error = err instanceof ApiError ? err.message : 'Silinemedi';
		}
	}
</script>

{#if loading}
	<div class="flex justify-center py-8">
		<div class="size-6 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"></div>
	</div>
{:else}
	<div class="mb-3 flex items-center justify-between">
		<p class="text-sm text-slate-500">{files.length} dosya</p>
		<button
			type="button"
			class="btn-secondary !min-h-9 !px-3 !text-xs"
			onclick={() => fileInput?.click()}
			disabled={busy}
		>
			{busy ? 'Yükleniyor…' : '+ Dosya Ekle'}
		</button>
		<input
			type="file"
			class="hidden"
			bind:this={fileInput}
			onchange={upload}
			accept="image/*,application/pdf,.doc,.docx,.xls,.xlsx,.txt,.zip"
		/>
	</div>

	{#if files.length === 0}
		<div class="card py-8 text-center text-sm text-slate-400">
			Fotoğraf veya dosya eklenmemiş.
		</div>
	{:else}
		<div class="card divide-y divide-slate-100 !p-0">
			{#each files as a (a.id)}
				<div class="flex items-center gap-3 px-3 py-3">
					<span class="text-2xl" aria-hidden="true">{fileIcon(a.mime_type)}</span>
					<div class="min-w-0 flex-1">
						<a
							href={`/api/workspaces/${wid}/attachments/${a.id}/file`}
							class="block truncate text-sm font-medium text-indigo-600 hover:text-indigo-700"
							download={a.file_name}
						>
							{a.file_name}
						</a>
						<p class="truncate text-xs text-slate-500">
							{fileSize(a.size)} · {a.uploader_name} · {new Date(a.created_at).toLocaleDateString('tr-TR')}
						</p>
					</div>
					<button
						type="button"
						class="flex size-11 shrink-0 items-center justify-center rounded-lg text-red-400 hover:bg-red-50"
						onclick={() => remove(a)}
						aria-label="Sil"
					>
						<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
							<path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
						</svg>
					</button>
				</div>
			{/each}
		</div>
	{/if}
{/if}

{#if error}
	<p class="form-error" role="alert">{error}</p>
{/if}
