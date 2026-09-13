import { api } from '$lib/api/client';
import type { User } from '$lib/api/types';

// Svelte 5 runes ile global auth durumu.
class AuthStore {
	user = $state<User | null>(null);
	loaded = $state(false);

	get isLoggedIn(): boolean {
		return this.user !== null;
	}

	/** Sayfa acilista oturumu kontrol et. */
	async init(): Promise<void> {
		if (this.loaded) return;
		try {
			this.user = await api.get<User>('/auth/me');
		} catch {
			this.user = null;
		}
		this.loaded = true;
	}

	setUser(u: User | null): void {
		this.user = u;
		this.loaded = true;
	}

	async logout(): Promise<void> {
		try {
			await api.post('/auth/logout');
		} finally {
			this.user = null;
		}
	}
}

export const auth = new AuthStore();
