<script lang="ts">
	// Dinamik ozellik form alani - 12 veri tipini native inputlarla render eder.

	let {
		def,
		value = $bindable(undefined as unknown)
	}: {
		def: {
			id?: string;
			name: string;
			data_type: string;
			is_required?: boolean;
			unit?: string | null;
			options?: string[];
		};
		value?: unknown;
	} = $props();

	const isNumberType = $derived(
		['integer', 'decimal', 'currency', 'percentage'].includes(def.data_type)
	);

	function strVal(): string {
		if (value === null || value === undefined) return '';
		return String(value);
	}

	function setStr(v: string) {
		if (v === '') {
			value = null;
		} else {
			value = v;
		}
	}

	function setNum(v: string) {
		if (v === '') {
			value = null;
		} else {
			const n = Number(v.replace(',', '.'));
			value = Number.isNaN(n) ? v : n;
		}
	}

	function boolVal(): boolean {
		return value === true || value === 'true' || value === 1;
	}

	function setBool(v: boolean) {
		value = v;
	}

	function arrVal(): string[] {
		return Array.isArray(value) ? (value as string[]) : [];
	}

	function toggleMulti(opt: string) {
		const arr = arrVal();
		value = arr.includes(opt) ? arr.filter((x) => x !== opt) : [...arr, opt];
	}

	const inputMode = $derived(
		def.data_type === 'integer' ? 'numeric' : isNumberType ? 'decimal' : 'text'
	);
</script>

<div>
	<label class="label" for="attr-{def.id ?? def.name}">
		{def.name}{def.is_required ? ' *' : ''}
		{#if def.unit}<span class="font-normal text-slate-400">({def.unit})</span>{/if}
	</label>

	{#if def.data_type === 'textarea'}
		<textarea
			id="attr-{def.id ?? def.name}"
			class="input min-h-24"
			rows="3"
			value={strVal()}
			oninput={(e) => setStr(e.currentTarget.value)}
		></textarea>

	{:else if def.data_type === 'boolean'}
		<label class="flex min-h-11 items-center gap-3 rounded-lg border border-slate-300 bg-white px-3">
			<input
				type="checkbox"
				class="size-5 accent-indigo-600"
				checked={boolVal()}
				onchange={(e) => setBool(e.currentTarget.checked)}
			/>
			<span class="text-sm text-slate-600">{boolVal() ? 'Evet' : 'Hayır'}</span>
		</label>

	{:else if def.data_type === 'select'}
		<select
			id="attr-{def.id ?? def.name}"
			class="input"
			value={strVal()}
			onchange={(e) => setStr(e.currentTarget.value)}
		>
			<option value="">— Seçin —</option>
			{#each def.options ?? [] as opt (opt)}
				<option value={opt}>{opt}</option>
			{/each}
		</select>

	{:else if def.data_type === 'multiselect'}
		<div class="flex flex-wrap gap-2">
			{#each def.options ?? [] as opt (opt)}
				<button
					type="button"
					class="badge min-h-11 border px-3 text-sm transition-colors
						{arrVal().includes(opt)
							? 'border-indigo-600 bg-indigo-600 text-white'
							: 'border-slate-300 bg-white text-slate-700 hover:bg-slate-50'}"
					onclick={() => toggleMulti(opt)}
					aria-pressed={arrVal().includes(opt)}
				>
					{opt}
				</button>
			{/each}
		</div>

	{:else if isNumberType}
		<div class="relative">
			<input
				id="attr-{def.id ?? def.name}"
				class="input"
				type="text"
				inputmode={inputMode}
				value={strVal()}
				oninput={(e) => setNum(e.currentTarget.value)}
				placeholder="0"
			/>
			{#if def.data_type === 'percentage'}
				<span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-slate-400">%</span>
			{:else if def.data_type === 'currency'}
				<span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-slate-400">₺</span>
			{/if}
		</div>

	{:else if def.data_type === 'date'}
		<input
			id="attr-{def.id ?? def.name}"
			class="input"
			type="date"
			value={strVal()}
			onchange={(e) => setStr(e.currentTarget.value)}
		/>

	{:else if def.data_type === 'datetime'}
		<input
			id="attr-{def.id ?? def.name}"
			class="input"
			type="datetime-local"
			value={strVal().slice(0, 16)}
			onchange={(e) => {
				const v = e.currentTarget.value;
				setStr(v ? new Date(v).toISOString() : '');
			}}
		/>

	{:else}
		<input
			id="attr-{def.id ?? def.name}"
			class="input"
			type={def.data_type === 'email' ? 'email' : def.data_type === 'phone' ? 'tel' : 'text'}
			value={strVal()}
			oninput={(e) => setStr(e.currentTarget.value)}
		/>
	{/if}
</div>
