<script lang="ts">
	import { api, readJsonResponse } from '$lib/api';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { addToast } from '$lib/stores/toast.svelte';
	import type { ApiResult, Plugin } from '$lib/types';

	const PLUGIN_EXTENSIONS = ['.so', '.dylib', '.dll'];

	let items = $state<Plugin[]>([]);
	let loading = $state(false);
	let uploadOpen = $state(false);
	let confirmOpen = $state(false);
	let confirmName = $state('');
	let confirmLoading = $state(false);
	let uploading = $state(false);
	let dragOver = $state(false);
	let search = $state('');

	const filtered = $derived(
		search.trim()
			? items.filter(
					(i) =>
						i.name.toLowerCase().includes(search.toLowerCase()) ||
						(i.provider ?? '').toLowerCase().includes(search.toLowerCase()) ||
						(i.pluginState ?? '').toLowerCase().includes(search.toLowerCase())
				)
			: items
	);

	async function fetchItems() {
		loading = true;
		try {
			items = await api.getPlugins();
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
			await api.deletePlugin(confirmName);
			confirmOpen = false;
			await fetchItems();
		} catch {
			/* toast shown */
		} finally {
			confirmLoading = false;
		}
	}

	async function uploadPlugin(file: File) {
		if (!PLUGIN_EXTENSIONS.some((ext) => file.name.toLowerCase().endsWith(ext))) {
			addToast('error', 'Only plugin libraries (.so, .dylib, .dll) are supported');
			return;
		}
		uploading = true;
		try {
			const fd = new FormData();
			fd.append('file', file);
			const res = await fetch('/api/v1/plugin', { method: 'POST', body: fd });
			const result = await readJsonResponse<ApiResult>(res, `Upload failed (${res.status})`);
			if (!res.ok || result.type === 'ERROR') {
				addToast('error', result.message || `Upload failed (${res.status})`);
			} else {
				addToast('success', result.message || 'Plugin installed successfully');
				uploadOpen = false;
			}
			await fetchItems();
		} catch (err) {
			addToast('error', err instanceof Error ? err.message : 'Upload failed');
		} finally {
			uploading = false;
		}
	}

	function handleFileInput(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (file) uploadPlugin(file);
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragOver = false;
		const file = e.dataTransfer?.files?.[0];
		if (file) uploadPlugin(file);
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		dragOver = true;
	}

	const stateConfig: Record<string, { color: string; bg: string; label: string }> = {
		ACTIVE: { color: 'var(--success)', bg: 'var(--success-light)', label: 'Active' },
		STOPPED: { color: 'var(--warning)', bg: 'var(--warning-light)', label: 'Stopped' },
		FAILED: { color: 'var(--danger)', bg: 'var(--danger-light)', label: 'Failed' },
		UNRESOLVED: { color: 'var(--info)', bg: 'var(--info-light)', label: 'Unresolved' }
	};

	function getStateConfig(state?: string) {
		return stateConfig[state ?? ''] ?? { color: 'var(--text-muted)', bg: 'var(--bg-raised)', label: state ?? '—' };
	}

	$effect(() => {
		fetchItems();
	});
</script>

<svelte:head><title>Plugin — LogMaker</title></svelte:head>

<div class="page">
	<header class="page-header">
		<div class="header-left">
			<h1 class="page-title">Plugin</h1>
			<span class="item-count">{filtered.length} of {items.length}</span>
		</div>
		<div class="header-actions">
			<div class="search-wrap">
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
				<input class="search-input" type="search" placeholder="Search plugins…" bind:value={search} aria-label="Search plugins" />
			</div>
			<button class="btn btn-ghost" onclick={fetchItems} disabled={loading}>
				{#if loading}
					<span class="spinner-muted"></span>
				{:else}
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
				{/if}
				Reload
			</button>
			<button class="btn btn-primary" onclick={() => (uploadOpen = true)} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Install Plugin
			</button>
		</div>
	</header>

	{#if loading && items.length === 0}
		<div class="loading-state">
			<span class="spinner-muted"></span>
			<span>Loading plugins…</span>
		</div>
	{:else if items.length === 0}
		<div class="empty-state">
			<div class="empty-state-icon">
				<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.24 12.24a6 6 0 00-8.49-8.49L5 10.5V19h8.5z"/><line x1="16" y1="8" x2="2" y2="22"/><line x1="17.5" y1="15" x2="9" y2="15"/></svg>
			</div>
			<p class="empty-state-title">No plugins installed</p>
			<p class="empty-state-desc">Install a plugin library to extend LogMaker with custom makers and senders</p>
			<button class="btn btn-primary" onclick={() => (uploadOpen = true)}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Install Plugin
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
	{:else}
		<div class="plugin-grid" role="list" aria-label="Plugin list">
			{#each filtered as item}
				{@const sc = getStateConfig(item.pluginState)}
				<div class="plugin-card" role="listitem">
					<div class="plugin-card-bar" style="background:{sc.color}"></div>
					<div class="plugin-inner">
						<div class="plugin-header">
							<div class="plugin-icon" style="color:{sc.color};background:{sc.bg}">
								<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.24 12.24a6 6 0 00-8.49-8.49L5 10.5V19h8.5z"/><line x1="16" y1="8" x2="2" y2="22"/><line x1="17.5" y1="15" x2="9" y2="15"/></svg>
							</div>
							<div class="plugin-title-block">
								<div class="plugin-name-row">
									<span class="plugin-name">{item.name}</span>
									{#if item.version}
										<span class="plugin-version">v{item.version}</span>
									{/if}
								</div>
								{#if item.provider}
									<span class="plugin-provider">{item.provider}</span>
								{/if}
							</div>
							{#if item.pluginState}
								<div class="plugin-state" style="color:{sc.color};background:{sc.bg};border-color:color-mix(in srgb, {sc.color} 25%, transparent)">
									<span class="state-dot" style="background:{sc.color}"></span>
									{sc.label}
								</div>
							{/if}
						</div>

						{#if item.pluginDescription}
							<p class="plugin-desc">{item.pluginDescription}</p>
						{/if}

						<div class="plugin-meta">
							{#if item.filename}
								<div class="meta-row">
									<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
									<span class="mono meta-val">{item.filename}</span>
								</div>
							{/if}
							{#if (item.ref ?? 0) > 0}
								<div class="meta-row">
									<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
									<span class="meta-val">Referenced by {item.ref} item{(item.ref ?? 0) !== 1 ? 's' : ''}</span>
								</div>
							{/if}
						</div>

						<div class="plugin-footer">
							<div class="plugin-ref-info">
								{#if (item.ref ?? 0) > 0}
									<span class="ref-badge ref-badge-used">
										<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
										In use: {item.ref}
									</span>
								{:else}
									<span class="ref-badge ref-badge-free">Not in use</span>
								{/if}
							</div>
							<button
								class="btn btn-ghost btn-sm btn-uninstall"
								onclick={() => askDelete(item.name)}
								disabled={(item.ref ?? 0) > 0}
								title={(item.ref ?? 0) > 0 ? 'Plugin is in use' : 'Uninstall plugin'}
								aria-label="Uninstall {item.name}"
							>
								<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
								Uninstall
							</button>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<!-- Upload Dialog -->
{#if uploadOpen}
	<div class="overlay" role="presentation">
		<div
			class="dialog"
			role="dialog"
			aria-modal="true"
			tabindex="-1"
			aria-label="Install Plugin"
			onkeydown={(e) => e.key === 'Escape' && (uploadOpen = false)}
		>
			<div class="dialog-header">
				<h2 class="dialog-title">Install Plugin</h2>
				<button class="close-btn" onclick={() => (uploadOpen = false)} aria-label="Close">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<div class="dialog-body">
				<label
					class="upload-zone"
					class:drag={dragOver}
					class:busy={uploading}
					ondrop={handleDrop}
					ondragover={handleDragOver}
					ondragleave={() => (dragOver = false)}
				>
					<input type="file" accept={PLUGIN_EXTENSIONS.join(',')} class="sr-only" onchange={handleFileInput} disabled={uploading} />
					{#if uploading}
						<div class="upload-spinner"></div>
						<span class="upload-label">Installing…</span>
					{:else}
						<svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="1.5">
							<path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
							<polyline points="17 8 12 3 7 8"/>
							<line x1="12" y1="3" x2="12" y2="15"/>
						</svg>
						<span class="upload-label">Drop a plugin library here or click to browse</span>
						<span class="upload-hint">Native plugin libraries only (.so, .dylib, .dll)</span>
					{/if}
				</label>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-ghost" onclick={() => (uploadOpen = false)} disabled={uploading}>Cancel</button>
			</div>
		</div>
	</div>
{/if}

<ConfirmDialog
	open={confirmOpen}
	title="Uninstall Plugin"
	message="Uninstall '{confirmName}'? This cannot be undone."
	confirmLabel="Uninstall"
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

	.search-input::placeholder { color: var(--text-muted); }

	.search-input:focus {
		outline: none;
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
		width: 240px;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 2rem;
		color: var(--text-muted);
		font-size: 0.875rem;
	}

	/* Plugin grid */
	.plugin-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
		gap: 0.75rem;
	}

	.plugin-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		display: flex;
		flex-direction: column;
		transition: border-color 0.15s;
	}

	.plugin-card:hover {
		border-color: var(--accent);
	}

	.plugin-card-bar {
		height: 2px;
		flex-shrink: 0;
		background: var(--border);
		transition: background 0.15s;
	}

	.plugin-card:hover .plugin-card-bar {
		background: var(--accent);
	}

	.plugin-inner {
		padding: 0.875rem;
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
		flex: 1;
	}

	.plugin-header {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
	}

	.plugin-icon {
		width: 36px;
		height: 36px;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.plugin-title-block {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		flex: 1;
		min-width: 0;
	}

	.plugin-name-row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.plugin-name {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.plugin-version {
		font-size: 0.625rem;
		font-weight: 600;
		color: var(--text-muted);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.1rem 0.375rem;
		flex-shrink: 0;
		font-family: var(--font-mono);
	}

	.plugin-provider {
		font-size: 0.6875rem;
		color: var(--text-muted);
	}

	.plugin-state {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.175rem 0.4375rem;
		border-radius: 4px;
		font-size: 0.5625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		border: 1px solid;
		flex-shrink: 0;
	}

	.state-dot {
		width: 4px;
		height: 4px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.plugin-desc {
		font-size: 0.8125rem;
		color: var(--text-secondary);
		margin: 0;
		line-height: 1.5;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.plugin-meta {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.meta-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		color: var(--text-muted);
	}

	.meta-val {
		font-size: 0.6875rem;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.mono {
		font-family: var(--font-mono);
	}

	.plugin-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-top: 0.5rem;
		border-top: 1px solid var(--border);
		margin-top: auto;
	}

	.plugin-ref-info {
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

	.btn-uninstall:not(:disabled):hover {
		color: var(--danger);
		background: var(--danger-light);
		border-color: color-mix(in srgb, var(--danger) 25%, transparent);
	}

	.upload-spinner {
		width: 28px;
		height: 28px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	@media (max-width: 600px) {
		.plugin-grid { grid-template-columns: 1fr; }
		.search-input { width: 150px; }
		.search-input:focus { width: 150px; }
	}
</style>
