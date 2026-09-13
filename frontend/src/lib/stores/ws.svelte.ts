import { api } from '$lib/api/client';

interface MeResponse {
	role_name: string;
	is_owner: boolean;
	permissions: string[];
}

/**
 * Aktif workspace'in rol/izin baglami.
 * Layout'ta wid basina bir kez yuklenir; sayfalar `can()` ile
 * yonetim yuzeylerini rol bazli gizler/gosterir.
 */
class WsStore {
	role = $state('');
	isOwner = $state(false);
	perms = $state<Set<string>>(new Set());
	loaded = $state(false);
	error = $state('');

	/** Yonetim profili: yonetim izinlerinden herhangi biri var mi */
	readonly isManager = $derived(
		this.can('work_item.create') || this.can('user.invite') || this.can('role.manage')
	);

	can(perm: string): boolean {
		return this.isOwner || this.perms.has(perm);
	}

	async load(wid: string) {
		this.loaded = false;
		this.error = '';
		this.role = '';
		this.isOwner = false;
		this.perms = new Set();
		try {
			const me = await api.get<MeResponse>(`/workspaces/${wid}/me`);
			this.role = me.role_name;
			this.isOwner = me.is_owner;
			this.perms = new Set(me.permissions);
		} catch {
			this.error = 'Yetkiler yüklenemedi';
		} finally {
			this.loaded = true;
		}
	}
}

export const ws = new WsStore();
