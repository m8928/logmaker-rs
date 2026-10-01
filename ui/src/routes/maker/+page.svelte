<script lang="ts">
	import { tick } from 'svelte';
	import { api, readJsonResponse, unwrapApiResult } from '$lib/api';
	import DynamicInput from '$lib/components/DynamicInput.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import Select from '$lib/components/Select.svelte';
	import { nameError } from '$lib/names';
	import { addToast } from '$lib/stores/toast.svelte';
	import type { ApiResult, Maker, PluginType } from '$lib/types';

	let items = $state<Maker[]>([]);
	let types = $state<PluginType[]>([]);
	let loading = $state(false);
	let search = $state('');
	let viewMode = $state<'grid' | 'table'>((typeof localStorage !== 'undefined' && localStorage.getItem('logmaker-viewMode-maker') as 'grid' | 'table') || 'grid');
	$effect(() => { localStorage.setItem('logmaker-viewMode-maker', viewMode); });

	// Dialog state
	let dialogOpen = $state(false);
	let maximized = $state(false);
	let editMode = $state(false);
	let importOpen = $state(false);
	let confirmOpen = $state(false);
	let confirmName = $state('');
	let confirmLoading = $state(false);

	// Form state
	let formName = $state('');
	let formType = $state('');
	let formArgs = $state<Record<string, string | number | boolean | string[]>>({});
	let errors = $state<Record<string, string>>({});

	let triggerElement = $state<HTMLElement | null>(null);

	function formatArgName(name: string): string {
		return name
			.replace(/([A-Z])/g, ' $1')
			.replace(/^./, (s) => s.toUpperCase())
			.trim();
	}

	function hasArgValue(value: string | number | boolean | string[] | undefined): boolean {
		if (Array.isArray(value)) return value.length > 0;
		if (typeof value === 'string') return value.trim().length > 0;
		if (typeof value === 'number') return Number.isFinite(value);
		if (typeof value === 'boolean') return true;
		return false;
	}

	function validateRequiredArgs() {
		for (const [key, arg] of Object.entries(getCurrentArgs())) {
			if (arg.required && !hasArgValue(formArgs[key])) {
				errors[`arg.${key}`] = `${formatArgName(key)} is required`;
			}
		}
	}

	const filtered = $derived(
		search.trim()
			? items.filter(
					(i) =>
						i.name.toLowerCase().includes(search.toLowerCase()) ||
						i.type.toLowerCase().includes(search.toLowerCase())
				)
			: items
	);

	function validate(): boolean {
		errors = {};
		const nameProblem = editMode ? null : nameError(formName, 'maker');
		if (nameProblem) errors.name = nameProblem;
		if (!formType) errors.type = 'Type is required';
		if (formType) validateRequiredArgs();
		return Object.keys(errors).length === 0;
	}

	async function fetchItems() {
		loading = true;
		try {
			items = await api.getMakers();
		} catch {
			/* toast already shown */
		} finally {
			loading = false;
		}
	}

	async function fetchTypes() {
		try {
			types = await api.getMakerTypes();
		} catch {
			/* ignored */
		}
	}

	function openAdd(e?: MouseEvent) {
		triggerElement = (e?.currentTarget as HTMLElement) ?? null;
		editMode = false;
		formName = '';
		formType = '';
		formArgs = {};
		errors = {};
		dialogOpen = true;
		fetchTypes();
	}

	function openEdit(item: Maker, e?: MouseEvent) {
		triggerElement = (e?.currentTarget as HTMLElement) ?? null;
		editMode = true;
		formName = item.name;
		formType = item.type;
		formArgs = { ...item.args };
		errors = {};
		dialogOpen = true;
		fetchTypes();
	}

	function openCopy(item: Maker, e?: MouseEvent) {
		triggerElement = (e?.currentTarget as HTMLElement) ?? null;
		editMode = false;
		formName = 'copy-of-' + item.name;
		formType = item.type;
		formArgs = { ...item.args };
		errors = {};
		dialogOpen = true;
		fetchTypes();
	}

	function closeDialog() {
		dialogOpen = false;
		maximized = false;
		triggerElement?.focus();
		triggerElement = null;
	}

	$effect(() => {
		if (dialogOpen) {
			tick().then(() => {
				const firstInput = document.querySelector<HTMLElement>(
					'.dialog input:not([disabled]), .dialog button.select-trigger:not([disabled])'
				);
				firstInput?.focus();
			});
		}
	});

	function getCurrentArgs() {
		const t = types.find((t) => t.type === formType);
		return t ? t.args : {};
	}

	function handleTypeChange() {
		formArgs = {};
		errors = {};
	}

	function handleArgChange(name: string, value: string | number | boolean | string[]) {
		formArgs = { ...formArgs, [name]: value };
		if (errors[`arg.${name}`]) {
			const next = { ...errors };
			delete next[`arg.${name}`];
			errors = next;
		}
	}

	async function submit() {
		if (!validate()) return;
		loading = true;
		try {
			const payload = { name: formName, type: formType, args: formArgs };
			if (editMode) {
				await api.updateMaker(formName, payload);
			} else {
				await api.createMaker(payload);
			}
			closeDialog();
			await fetchItems();
		} catch {
			/* toast shown */
		} finally {
			loading = false;
		}
	}

	function askDelete(name: string) {
		confirmName = name;
		confirmOpen = true;
	}

	async function confirmDelete() {
		confirmLoading = true;
		try {
			await api.deleteMaker(confirmName);
			confirmOpen = false;
			await fetchItems();
		} catch {
			/* toast shown */
		} finally {
			confirmLoading = false;
		}
	}

	async function exportData() {
		const data = await api.getMakers();
		const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'logmaker-maker.json';
		a.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}

	let importInput = $state<HTMLInputElement | null>(null);

	async function handleImport(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (!file) return;
		loading = true;
		try {
			const formData = new FormData();
			formData.append('file', file);
			const res = await fetch('/api/v1/maker:import-file', { method: 'POST', body: formData });
			const result = await readJsonResponse<Array<ApiResult | { body?: ApiResult }>>(res, `Import failed (${res.status})`);
			if (!res.ok) {
				throw new Error(`Import failed (${res.status})`);
			}
			const entries = Array.isArray(result) ? result.map(unwrapApiResult) : [];
			if (entries.some((r) => r.type === 'ERROR')) {
				const failed = entries
					.filter((r) => r.type === 'ERROR')
					.map((r) => r.message ?? 'unknown')
					.join(', ');
				addToast('error', `Import failed for: ${failed}`);
			} else {
				addToast('success', 'Import successful');
				importOpen = false;
			}
			await fetchItems();
		} catch (err) {
			addToast('error', err instanceof Error ? err.message : 'Import failed');
		} finally {
			loading = false;
		}
	}

	// Icon per maker type
	function getMakerIcon(type: string): string {
		const t = type.toLowerCase();
		if (t.includes('ip') || t.includes('net'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z"/></svg>`;
		if (t.includes('regex') || t.includes('pattern'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>`;
		if (t.includes('date') || t.includes('time') || t.includes('timestamp'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="18" rx="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>`;
		if (t.includes('uuid') || t.includes('guid'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2a5 5 0 100 10 5 5 0 000-10z"/><path d="M21 21l-4.35-4.35M16.65 16.65A7.5 7.5 0 103.35 7.35a7.5 7.5 0 0013.3 9.3z"/></svg>`;
		if (t.includes('pick') || t.includes('choice') || t.includes('random'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="16 3 21 3 21 8"/><line x1="4" y1="20" x2="21" y2="3"/><polyline points="21 16 21 21 16 21"/><line x1="15" y1="15" x2="21" y2="21"/><line x1="4" y1="4" x2="9" y2="9"/></svg>`;
		if (t.includes('number') || t.includes('int') || t.includes('range'))
			return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="4" y1="9" x2="20" y2="9"/><line x1="4" y1="15" x2="20" y2="15"/><line x1="10" y1="3" x2="8" y2="21"/><line x1="16" y1="3" x2="14" y2="21"/></svg>`;
		return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"/></svg>`;
	}

	function getMakerAccent(_type: string): string {
		return 'var(--accent)';
	}

	function getArgPreview(args: Record<string, string | number | boolean | string[]>): [string, string][] {
		const entries = Object.entries(args).filter(
			([, v]) => v !== '' && v !== 0 && v !== false && !(Array.isArray(v) && v.length === 0)
		);
		return entries.slice(0, 3) as [string, string][];
	}

	function getArgInline(args: Record<string, string | number | boolean | string[]>): string {
		const entries = Object.entries(args).filter(
			([, v]) => v !== '' && v !== 0 && v !== false && !(Array.isArray(v) && v.length === 0)
		);
		if (entries.length === 0) return '—';
		const [key, val] = entries[0];
		const valStr = Array.isArray(val) ? val.join(', ') : String(val);
		const truncated = valStr.length > 20 ? valStr.slice(0, 20) + '…' : valStr;
		return `${key}: ${truncated}`;
	}

	$effect(() => {
		fetchItems();
	});
</script>

<svelte:head><title>Maker — LogMaker</title></svelte:head>

<div class="page">
	<header class="page-header">
		<div class="header-left">
			<h1 class="page-title">Maker</h1>
			<span class="item-count">{filtered.length} of {items.length}</span>
			<span class="page-hint">Data generators used in log format templates — IP, date, UUID, regex, etc.</span>
		</div>
		<div class="header-actions">
			<div class="search-wrap">
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
				<input class="search-input" type="search" placeholder="Search makers…" bind:value={search} aria-label="Search makers" />
			</div>
			<button class="btn btn-ghost" onclick={fetchItems} disabled={loading}>
				{#if loading}
					<span class="spinner-muted"></span>
				{:else}
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
				{/if}
				Reload
			</button>
			<div class="view-toggle" role="radiogroup" aria-label="View mode">
				<button
					class="toggle-btn"
					class:active={viewMode === 'grid'}
					onclick={() => (viewMode = 'grid')}
					aria-label="Grid view"
					aria-pressed={viewMode === 'grid'}
				>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
				</button>
				<button
					class="toggle-btn"
					class:active={viewMode === 'table'}
					onclick={() => (viewMode = 'table')}
					aria-label="Table view"
					aria-pressed={viewMode === 'table'}
				>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
				</button>
			</div>
			<button class="btn btn-ghost" onclick={() => (importOpen = true)} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
				Import
			</button>
			<button class="btn btn-ghost" onclick={exportData} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
				Export
			</button>
			<button class="btn btn-primary" onclick={(e) => openAdd(e)} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Add Maker
			</button>
		</div>
	</header>

	{#if loading && items.length === 0}
		<div class="loading-state">
			<span class="spinner-muted"></span>
			<span>Loading makers…</span>
		</div>
	{:else if items.length === 0}
		<div class="empty-state">
			<div class="empty-state-icon">
				<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"/></svg>
			</div>
			<p class="empty-state-title">No makers yet</p>
			<p class="empty-state-desc">Create your first data generator to start building log templates</p>
			<button class="btn btn-primary" onclick={(e) => openAdd(e)}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Add Maker
			</button>
		</div>
	{:else if filtered.length === 0}
		<div class="empty-state">
			<div class="empty-state-icon">
				<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
			</div>
			<p class="empty-state-title">No results for "{search}"</p>
			<p class="empty-state-desc">Try a different search term</p>
		</div>
	{:else if viewMode === 'grid'}
		<div class="card-grid" role="list" aria-label="Maker list">
			{#each filtered as item}
				{@const accent = getMakerAccent(item.type)}
				{@const preview = getArgPreview(item.args)}
				<div
					class="maker-card"
					role="button"
					style="--card-accent:{accent}"
					onclick={() => openEdit(item)}
					onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && openEdit(item)}
					tabindex="0"
					aria-label="Edit {item.name}"
				>
					<div class="card-top-bar"></div>
					<div class="card-inner">
						<div class="card-header">
							<div class="card-icon" style="color:{accent};background:color-mix(in srgb, {accent} 12%, transparent)">
								{@html getMakerIcon(item.type)}
							</div>
							<div class="card-title-block">
								<span class="card-name">{item.name}</span>
								<span class="card-type">{item.type}</span>
							</div>
						</div>

						{#if item.sample}
							<div class="card-sample-row">
								<span class="sample-label">Sample</span>
								<span class="card-sample mono">{item.sample}</span>
							</div>
						{/if}

						<div class="card-args">
							{#if preview.length > 0}
								{#each preview as [key, val]}
									<div class="arg-row">
										<span class="arg-key">{key}</span>
										<span class="arg-val">{Array.isArray(val) ? val.join(', ') : String(val)}</span>
									</div>
								{/each}
								{#if Object.keys(item.args).length > 3}
									<div class="arg-more">+{Object.keys(item.args).length - 3} more</div>
								{/if}
							{:else}
								<span class="arg-empty">No arguments configured</span>
							{/if}
						</div>

						<div class="card-footer">
							<div class="card-footer-left">
								{#if item.ref > 0}
									<span class="ref-badge ref-badge-used" title="Used by {item.ref} log(s)">
										<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
										Used: {item.ref}
									</span>
								{:else}
									<span class="ref-badge ref-badge-free">Unused</span>
								{/if}
							</div>
							<div class="card-actions" role="group" aria-label="Actions">
								<button
									class="icon-btn"
									onclick={(e) => { e.stopPropagation(); openCopy(item, e); }}
									title="Duplicate"
									aria-label="Duplicate {item.name}"
								>
									<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
								</button>
								<button
									class="icon-btn"
									onclick={(e) => { e.stopPropagation(); openEdit(item, e); }}
									title="Edit"
									aria-label="Edit {item.name}"
								>
									<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
								</button>
								<button
									class="icon-btn danger"
									onclick={(e) => { e.stopPropagation(); askDelete(item.name); }}
									disabled={item.ref > 0}
									title={item.ref > 0 ? 'In use by logs' : 'Delete'}
									aria-label="Delete {item.name}"
								>
									<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
								</button>
							</div>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{:else}
		<!-- Table view -->
		<div class="table-wrap">
			<table class="table" aria-label="Maker list">
				<thead>
					<tr>
						<th>Name</th>
						<th>Type</th>
						<th>Sample</th>
						<th>Args</th>
						<th>Used</th>
						<th class="right">Actions</th>
					</tr>
				</thead>
				<tbody>
					{#each filtered as item}
						<tr
							class="table-row-clickable"
							onclick={() => openEdit(item)}
							onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && openEdit(item)}
							tabindex="0"
							role="button"
							aria-label="Edit {item.name}"
						>
							<td>
								<span class="tbl-name mono">{item.name}</span>
							</td>
							<td>
								<span class="tbl-type-badge">{item.type}</span>
							</td>
							<td>
								{#if item.sample}
									<span class="tbl-sample mono">{item.sample}</span>
								{:else}
									<span class="tbl-empty">—</span>
								{/if}
							</td>
							<td>
								<span class="tbl-args-inline mono">{getArgInline(item.args)}</span>
							</td>
							<td>
								{#if item.ref > 0}
									<span class="ref-badge ref-badge-used">
										<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
										{item.ref}
									</span>
								{:else}
									<span class="ref-badge ref-badge-free">0</span>
								{/if}
							</td>
							<td class="right">
								<div class="row-actions">
									<button
										class="icon-btn"
										onclick={(e) => { e.stopPropagation(); openCopy(item, e); }}
										title="Duplicate"
										aria-label="Duplicate {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
									</button>
									<button
										class="icon-btn"
										onclick={(e) => { e.stopPropagation(); openEdit(item, e); }}
										title="Edit"
										aria-label="Edit {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
									</button>
									<button
										class="icon-btn danger"
										onclick={(e) => { e.stopPropagation(); askDelete(item.name); }}
										disabled={item.ref > 0}
										title={item.ref > 0 ? 'In use by logs' : 'Delete'}
										aria-label="Delete {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
									</button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<!-- Add/Edit Dialog -->
{#if dialogOpen}
	<div class="overlay" role="presentation">
		<div
			class="dialog {maximized ? 'maximized' : ''}"
			role="dialog"
			aria-modal="true"
			tabindex="-1"
			aria-label="{editMode ? 'Edit' : 'Add'} Maker"
			onkeydown={(e) => e.key === 'Escape' && closeDialog()}
		>
			<div class="dialog-header">
				<h2 class="dialog-title">{editMode ? 'Edit Maker' : 'Add Maker'}</h2>
				<div class="dialog-header-actions">
					<button class="close-btn" onclick={() => (maximized = !maximized)} aria-label={maximized ? 'Restore' : 'Maximize'}>
						{#if maximized}
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>
						{:else}
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>
						{/if}
					</button>
					<button class="close-btn" onclick={closeDialog} aria-label="Close">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
					</button>
				</div>
			</div>
			<div class="dialog-body">
				<div class="basic-row">
					<div class="field" style="flex:1">
						<label class="field-label" for="maker-name">NAME <span class="required">*</span></label>
						{#if errors.name}<span class="field-error">{errors.name}</span>{/if}
						<input id="maker-name" class="input" class:input-error={errors.name} type="text" bind:value={formName} disabled={editMode} placeholder="my-maker" />
					</div>
					<div class="field" style="flex:1">
						<span class="field-label" id="maker-type-label">TYPE <span class="required">*</span></span>
						{#if errors.type}<span class="field-error">{errors.type}</span>{/if}
						<Select
							value={formType}
							options={types.map(t => ({ value: t.type, label: t.type, sublabel: t.name || '' }))}
							placeholder="Select type…"
							disabled={editMode}
							aria-labelledby="maker-type-label"
							class={errors.type ? 'input-error' : ''}
							onchange={(v) => { formType = v; handleTypeChange(); }}
						/>
					</div>
				</div>
				{#if formType}
					{@const args = getCurrentArgs()}
					{#if Object.keys(args).length > 0}
						<div class="args-divider">
							<span>Arguments</span>
						</div>
						{#each Object.entries(args) as [key, arg]}
							<DynamicInput
								name={key}
								type={arg.type}
								value={formArgs[key] ??
									(arg.type === 'boolean'
										? false
										: arg.type === 'list'
											? []
											: '')}
								required={arg.required}
								description={arg.description}
								error={errors[`arg.${key}`]}
								onchange={handleArgChange}
							/>
						{/each}
					{/if}
				{/if}
			</div>
			<div class="dialog-footer">
				<button class="btn btn-ghost" onclick={closeDialog} disabled={loading}>Cancel</button>
				<button class="btn btn-primary" onclick={submit} disabled={loading}>
					{#if loading}<span class="spinner"></span>{/if}
					{editMode ? 'Save Changes' : 'Add Maker'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Import Dialog -->
{#if importOpen}
	<div class="overlay" role="presentation">
		<div
			class="dialog narrow"
			role="dialog"
			aria-modal="true"
			tabindex="-1"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.key === 'Escape' && (importOpen = false)}
		>
			<div class="dialog-header">
				<h2 class="dialog-title">Import Makers</h2>
				<button class="close-btn" onclick={() => (importOpen = false)} aria-label="Close">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<div class="dialog-body">
				<label class="upload-zone">
					<input bind:this={importInput} type="file" accept=".json" class="sr-only" onchange={handleImport} />
					<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="1.5"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
					<span class="upload-label">Drop JSON file here or click to browse</span>
					<span class="upload-hint">Exported maker configuration file</span>
				</label>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-ghost" onclick={() => (importOpen = false)}>Cancel</button>
			</div>
		</div>
	</div>
{/if}

<ConfirmDialog
	open={confirmOpen}
	title="Delete Maker"
	message="Delete '{confirmName}'? This cannot be undone."
	loading={confirmLoading}
	onconfirm={confirmDelete}
	oncancel={() => (confirmOpen = false)}
/>

<style>
	.header-left {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
	}

	.item-count {
		font-size: 0.8125rem;
		color: var(--text-muted);
	}

	/* Search */
	.search-wrap {
		position: relative;
		display: flex;
		align-items: center;
	}

	.search-icon {
		position: absolute;
		left: 0.5rem;
		color: var(--text-muted);
		pointer-events: none;
	}

	.search-input {
		padding: 0.375rem 0.75rem 0.375rem 1.875rem;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 0.8125rem;
		font-family: var(--font-ui);
		width: 192px;
		transition: border-color 0.15s, width 0.2s;
	}

	.search-input::placeholder {
		color: var(--text-muted);
	}

	.search-input:focus {
		outline: none;
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
		width: 240px;
	}

	/* Loading */
	.loading-state {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 2rem;
		color: var(--text-muted);
		font-size: 0.875rem;
	}

	/* Card grid */
	.card-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 0.75rem;
	}

	.maker-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
		display: flex;
		flex-direction: column;
	}

	.maker-card:hover {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--bg-surface) 96%, var(--accent));
	}

	.maker-card:focus {
		outline: none;
		border-color: var(--accent);
		background: color-mix(in srgb, var(--bg-surface) 96%, var(--accent));
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent);
	}

	.card-top-bar {
		height: 2px;
		background: var(--accent);
		flex-shrink: 0;
		opacity: 0;
		transition: opacity 0.15s;
	}

	.maker-card:hover .card-top-bar,
	.maker-card:focus .card-top-bar {
		opacity: 1;
	}

	.card-inner {
		padding: 0.875rem;
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
		flex: 1;
	}

	.card-header {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
	}

	.card-icon {
		width: 32px;
		height: 32px;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		background: var(--accent-light);
		color: var(--accent);
	}

	.card-title-block {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
		flex: 1;
	}

	.card-name {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.card-type {
		font-size: 0.6875rem;
		color: var(--text-muted);
		font-weight: 500;
		font-family: var(--font-mono);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.1rem 0.375rem;
		display: inline-block;
		width: fit-content;
	}

	/* Sample row — improved */
	.card-sample-row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding: 0.3rem 0.5rem;
		background: color-mix(in srgb, var(--accent) 7%, var(--bg-base));
		border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
		border-radius: var(--radius-sm);
	}

	.sample-label {
		font-size: 0.5625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--accent);
		flex-shrink: 0;
	}

	.card-sample {
		font-size: 0.6875rem;
		color: var(--accent);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-weight: 500;
	}

	/* Args */
	.card-args {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-height: 44px;
	}

	.arg-row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.arg-key {
		color: var(--text-muted);
		font-weight: 500;
		white-space: nowrap;
		flex-shrink: 0;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		min-width: 64px;
	}

	.arg-val {
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-family: var(--font-mono);
		font-size: 0.6875rem;
	}

	.arg-more {
		font-size: 0.6875rem;
		color: var(--text-muted);
	}

	.arg-empty {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	/* Footer */
	.card-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-top: 0.5rem;
		border-top: 1px solid var(--border);
		margin-top: auto;
	}

	.card-footer-left {
		display: flex;
		align-items: center;
	}

	.ref-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.4375rem;
		border-radius: 4px;
		font-size: 0.625rem;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.ref-badge-used {
		background: var(--warning-light);
		color: var(--warning);
		border: 1px solid color-mix(in srgb, var(--warning) 20%, transparent);
	}

	.ref-badge-free {
		background: var(--bg-raised);
		color: var(--text-muted);
		border: 1px solid var(--border);
	}

	.card-actions {
		display: flex;
		gap: 0.125rem;
	}

	/* Table view */
	.table-row-clickable {
		cursor: pointer;
	}

	.table-row-clickable:focus {
		outline: none;
	}

	.table-row-clickable:focus td {
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}

	.tbl-name {
		font-weight: 600;
		color: var(--text-primary);
		font-size: 0.8125rem;
	}

	.tbl-type-badge {
		display: inline-block;
		padding: 0.15rem 0.4375rem;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.6875rem;
		font-family: var(--font-mono);
		color: var(--text-muted);
		font-weight: 500;
		white-space: nowrap;
	}

	.tbl-sample {
		color: var(--accent);
		font-size: 0.75rem;
		font-weight: 500;
	}

	.tbl-args-inline {
		font-size: 0.75rem;
		color: var(--text-secondary);
		max-width: 200px;
		display: inline-block;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.tbl-empty {
		color: var(--text-muted);
		font-size: 0.75rem;
	}

	/* Error state */
	.field-error {
		font-size: 0.75rem;
		color: var(--danger);
		font-weight: 500;
	}

	.input-error {
		border-color: var(--danger) !important;
	}

	@media (max-width: 600px) {
		.search-input { width: 150px; }
		.search-input:focus { width: 150px; }
		.card-grid { grid-template-columns: 1fr; }
	}
</style>
