<script lang="ts">
	import { api } from '$lib/api';
	import type { DashboardData, Log, Scenario } from '$lib/types';

	let data = $state<DashboardData | null>(null);
	let logs = $state<Log[]>([]);
	let scenarios = $state<Scenario[]>([]);
	let loading = $state(true);
	let error = $state(false);
	let lastUpdated = $state(0);
	let secondsAgo = $state(0);

	async function fetchData() {
		if (!data) loading = true;
		error = false;
		try {
			[data, logs, scenarios] = await Promise.all([api.getDashboard(), api.getLogs(), api.getScenarios().catch(() => [] as Scenario[])]);
			lastUpdated = Date.now();
			secondsAgo = 0;
		} catch {
			error = true;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		fetchData();
		const fetchInterval = setInterval(fetchData, 5000);
		const clockInterval = setInterval(() => {
			if (lastUpdated > 0) {
				secondsAgo = Math.floor((Date.now() - lastUpdated) / 1000);
			}
		}, 1000);
		return () => {
			clearInterval(fetchInterval);
			clearInterval(clockInterval);
		};
	});

	const runningLogs = $derived(logs.filter((l) => !l.paused && l.eps > 0 && l.sender.length > 0 && (l.currentEps > 0 || (l.bytesPerSec ?? 0) > 0)));

	// Onboarding: show guide when system is empty
	const isEmpty = $derived(data !== null && data.maker === 0 && data.sender === 0 && logs.length === 0);
	let guideDismissed = $state(false);

	$effect(() => {
		guideDismissed = localStorage.getItem('logmaker-guide-dismissed') === 'true';
	});

	function dismissGuide() {
		guideDismissed = true;
		localStorage.setItem('logmaker-guide-dismissed', 'true');
	}

	function showGuide() {
		guideDismissed = false;
		localStorage.removeItem('logmaker-guide-dismissed');
	}

	const metricTiles = $derived([
		{
			label: 'Makers',
			value: data?.maker ?? 0,
			icon: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"/></svg>`,
			href: '/maker'
		},
		{
			label: 'Senders',
			value: data?.sender ?? 0,
			icon: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>`,
			href: '/sender'
		},
		{
			label: 'Running',
			value: runningLogs.length,
			icon: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="10 15 15 12 10 9 10 15"/></svg>`,
			href: '/log'
		},
		{
			label: 'Scenarios',
			value: scenarios.length,
			icon: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="5" cy="12" r="2"/><circle cx="19" cy="5" r="2"/><circle cx="19" cy="19" r="2"/><line x1="7" y1="11.5" x2="17" y2="6.5"/><line x1="7" y1="12.5" x2="17" y2="17.5"/></svg>`,
			href: '/scenario'
		},
		{
			label: 'Plugins',
			value: data?.plugin ?? 0,
			icon: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20.24 12.24a6 6 0 00-8.49-8.49L5 10.5V19h8.5z"/><line x1="16" y1="8" x2="2" y2="22"/><line x1="17.5" y1="15" x2="9" y2="15"/></svg>`,
			href: '/plugin'
		}
	]);

	function formatBytes(b: number): string {
		if (b < 1024) return `${b} B`;
		if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
		if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
		return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
	}

	function timeUnitSuffix(tu?: string): string {
		return tu === 'min' ? '/m' : tu === 'hour' ? '/h' : tu === 'day' ? '/d' : '/s';
	}

	function splitBytes(b: number, tu?: string): { val: string; unit: string } {
		const s = timeUnitSuffix(tu);
		if (b === 0) return { val: '0', unit: 'bytes' + s };
		if (b < 1024) return { val: String(b), unit: 'B' + s };
		if (b < 1024 * 1024) return { val: (b / 1024).toFixed(1), unit: 'KB' + s };
		if (b < 1024 * 1024 * 1024) return { val: (b / (1024 * 1024)).toFixed(1), unit: 'MB' + s };
		return { val: (b / (1024 * 1024 * 1024)).toFixed(2), unit: 'GB' + s };
	}

	/** Format `b` using the same unit scale as `ref` */
	function splitBytesAs(b: number, ref: number): string {
		if (ref < 1024) return String(b);
		if (ref < 1024 * 1024) return (b / 1024).toFixed(1);
		if (ref < 1024 * 1024 * 1024) return (b / (1024 * 1024)).toFixed(1);
		return (b / (1024 * 1024 * 1024)).toFixed(2);
	}

	function timeMul(tu?: string): number {
		return tu === 'min' ? 60 : tu === 'hour' ? 3600 : tu === 'day' ? 86400 : 1;
	}

	function epsPct(log: Log): number {
		if (log.eps <= 0) return 0;
		const m = timeMul(log.epsTimeUnit);
		const actual = log.epsUnit === 'bytes' ? (log.bytesPerSec ?? 0) * m : log.currentEps * m;
		const raw = (actual / log.eps) * 100;
		if (raw > 0 && raw < 1) return Math.max(0.1, parseFloat(raw.toFixed(1)));
		return Math.min(100, Math.round(raw));
	}

	const cpuPct = $derived(Math.min(100, data?.cpu ?? 0));
	const memPct = $derived(
		data ? Math.min(100, Math.round(((data.memory ?? 0) / 2048) * 100)) : 0
	);
	const totalEpsPct = $derived(
		data && data.eps > 0
			? Math.min(100, Math.round((data.actualEps / data.eps) * 100))
			: 0
	);
</script>

<svelte:head>
	<title>Dashboard — LogMaker</title>
</svelte:head>

<div class="dash">
	<!-- Top bar -->
	<div class="topbar">
		<div class="topbar-left">
			<h1 class="dash-title">Dashboard</h1>
			<div class="live-badge" class:live={!loading && !error} class:err={error}>
				<span class="live-dot"></span>
				{#if error}
					Connection error
				{:else if loading && secondsAgo === 0}
					Connecting
				{:else}
					Live · {secondsAgo}s ago
				{/if}
			</div>
		</div>
		<div class="topbar-right">
			{#if error}
				<span class="error-chip">
					<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
					Server unreachable
				</span>
			{/if}
			{#if guideDismissed}
				<button class="btn btn-ghost btn-sm" onclick={showGuide} aria-label="Show getting started guide" title="Getting Started">
					<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
				</button>
			{/if}
			<button class="btn btn-ghost btn-sm" onclick={fetchData} disabled={loading} aria-label="Refresh dashboard">
				<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class:spin={loading}><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
				Refresh
			</button>
		</div>
	</div>

	<!-- Getting Started Guide (shown when system is empty) -->
	{#if (isEmpty || !guideDismissed) && !loading && !guideDismissed}
		<div class="guide-panel">
			<div class="guide-header">
				<div class="guide-title-row">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
					<span class="guide-title">Getting Started</span>
				</div>
				<button class="guide-dismiss" onclick={dismissGuide} aria-label="Dismiss guide">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<p class="guide-desc">LogMaker generates log data and sends it to destinations. Follow these 3 steps:</p>
			<div class="guide-steps">
				<a href="/maker" class="guide-step">
					<div class="step-num">1</div>
					<div class="step-body">
						<span class="step-title">Create Makers</span>
						<span class="step-desc">Define data generators — IP, date, UUID, regex, etc.</span>
					</div>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="step-arrow"><polyline points="9 18 15 12 9 6"/></svg>
				</a>
				<div class="step-connector" aria-hidden="true"></div>
				<a href="/sender" class="guide-step">
					<div class="step-num">2</div>
					<div class="step-body">
						<span class="step-title">Create Senders</span>
						<span class="step-desc">Configure destinations — Kafka, Syslog, Debug output.</span>
					</div>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="step-arrow"><polyline points="9 18 15 12 9 6"/></svg>
				</a>
				<div class="step-connector" aria-hidden="true"></div>
				<a href="/log" class="guide-step">
					<div class="step-num">3</div>
					<div class="step-body">
						<span class="step-title">Create Logs</span>
						<span class="step-desc">Build a template with makers, set EPS, pick senders, and run.</span>
					</div>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="step-arrow"><polyline points="9 18 15 12 9 6"/></svg>
				</a>
			</div>
		</div>
	{/if}

	<!-- Metric strip -->
	<div class="metric-strip">
		{#each metricTiles as tile}
			<a href={tile.href} class="metric-tile">
				<div class="metric-icon">
					{@html tile.icon}
				</div>
				<div class="metric-body">
					<span class="metric-value mono">{loading ? '—' : tile.value.toLocaleString()}</span>
					<span class="metric-label">{tile.label}</span>
				</div>
			</a>
		{/each}
	</div>

	<!-- Main 2-col layout -->
	<div class="main-grid">
		<!-- Left: Live Activity Panel -->
		<div class="panel activity-panel">
			<div class="panel-header">
				<span class="panel-title">
					<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="10 15 15 12 10 9 10 15"/></svg>
					Live Activity
				</span>
				<span class="panel-count">{runningLogs.length} active</span>
			</div>
			<div class="activity-list">
				{#if loading && logs.length === 0}
					<div class="activity-loading">
						<span class="spinner-muted"></span>
						<span>Loading…</span>
					</div>
				{:else if runningLogs.length === 0}
					<div class="activity-empty">
						<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><polyline points="10 15 15 12 10 9 10 15"/></svg>
						<span>No active generators</span>
						<a href="/log" class="btn btn-ghost btn-sm">Go to Logs</a>
					</div>
				{:else}
					{#each runningLogs as log}
						{@const pct = epsPct(log)}
						<div class="activity-card">
							<div class="activity-card-top">
								<div class="activity-name">
									<span class="running-dot"></span>
									<span class="name-text">{log.name}</span>
								</div>
								<div class="activity-eps mono">
									<span class="eps-actual">{log.epsUnit === 'bytes' ? splitBytesAs((log.bytesPerSec ?? 0) * timeMul(log.epsTimeUnit), log.eps) : (log.currentEps * timeMul(log.epsTimeUnit)).toLocaleString()}</span>
									<span class="eps-sep">/</span>
									<span class="eps-target">{log.epsUnit === 'bytes' ? splitBytes(log.eps, log.epsTimeUnit).val : log.eps.toLocaleString()}</span>
									<span class="eps-unit">{log.epsUnit === 'bytes' ? splitBytes(log.eps, log.epsTimeUnit).unit : 'evt' + timeUnitSuffix(log.epsTimeUnit)}</span>
								</div>
							</div>
							<div class="activity-format mono">{log.format.length > 64 ? log.format.slice(0, 64) + '…' : log.format}</div>
							<div class="activity-card-bottom">
								<div class="sender-tags">
									{#each log.sender.slice(0, 3) as s}
										<span class="stag">{s}</span>
									{/each}
									{#if log.sender.length > 3}
										<span class="stag stag-more">+{log.sender.length - 3}</span>
									{/if}
								</div>
								<div class="eps-bar-row">
									<div class="eps-bar">
										<div class="eps-bar-fill" style="width:{pct}%"></div>
									</div>
									<span class="eps-pct mono">{pct}%</span>
								</div>
							</div>
						</div>
					{/each}
				{/if}
			</div>
		</div>

		<!-- Right: System Health Panel -->
		<div class="panel health-panel">
			<div class="panel-header">
				<span class="panel-title">
					<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
					System Health
				</span>
			</div>

			<div class="health-body">
				<!-- Throughput overview -->
				<div class="health-section">
					<div class="section-label">Throughput</div>
					<div class="eps-overview">
						<div class="eps-big">
							<span class="eps-big-val mono">{loading ? '—' : (data?.actualEps ?? 0).toLocaleString()}</span>
							<span class="eps-big-sub">evt/s</span>
						</div>
						<span class="eps-divider">·</span>
						<div class="eps-big">
							<span class="eps-big-val mono">{loading ? '—' : splitBytes(data?.actualBps ?? 0).val}</span>
							<span class="eps-big-sub">{splitBytes(data?.actualBps ?? 0).unit}</span>
						</div>
					</div>
					<div class="h-bar-wrap">
						<div class="h-bar">
							<div class="h-bar-fill" style="width:{totalEpsPct}%"></div>
						</div>
						<span class="h-bar-pct mono">{totalEpsPct}%</span>
					</div>
				</div>

				<div class="health-sep"></div>

				<!-- Gauge rings row -->
				<div class="gauges-row">
					<!-- CPU -->
					<div class="gauge-block">
						<div class="gauge-wrap">
							<svg viewBox="0 0 80 80" width="72" height="72">
								<circle cx="40" cy="40" r="30" fill="none" stroke="var(--bg-raised)" stroke-width="7"/>
								<circle
									cx="40" cy="40" r="30"
									fill="none"
									stroke="var(--accent)"
									stroke-width="7"
									stroke-linecap="round"
									stroke-dasharray="188.5"
									stroke-dashoffset="{188.5 - (188.5 * cpuPct / 100)}"
									transform="rotate(-90 40 40)"
									style="transition: stroke-dashoffset 0.6s ease"
								/>
							</svg>
							<div class="gauge-inner">
								<span class="gauge-val mono">{loading ? '—' : `${cpuPct}%`}</span>
							</div>
						</div>
						<span class="gauge-label">CPU</span>
					</div>

					<!-- Memory -->
					<div class="gauge-block">
						<div class="gauge-wrap">
							<svg viewBox="0 0 80 80" width="72" height="72">
								<circle cx="40" cy="40" r="30" fill="none" stroke="var(--bg-raised)" stroke-width="7"/>
								<circle
									cx="40" cy="40" r="30"
									fill="none"
									stroke="var(--accent)"
									stroke-width="7"
									stroke-linecap="round"
									stroke-dasharray="188.5"
									stroke-dashoffset="{188.5 - (188.5 * memPct / 100)}"
									transform="rotate(-90 40 40)"
									style="transition: stroke-dashoffset 0.6s ease"
								/>
							</svg>
							<div class="gauge-inner">
								<span class="gauge-val mono" style="font-size:0.6875rem">{loading ? '—' : `${data?.memory ?? 0}MB`}</span>
							</div>
						</div>
						<span class="gauge-label">Memory</span>
					</div>

					<!-- Threads -->
					{#if data?.thread !== undefined}
						<div class="gauge-block">
							<div class="gauge-wrap">
								<svg viewBox="0 0 80 80" width="72" height="72">
									<circle cx="40" cy="40" r="30" fill="none" stroke="var(--bg-raised)" stroke-width="7"/>
									<circle
										cx="40" cy="40" r="30"
										fill="none"
										stroke="var(--accent)"
										stroke-width="7"
										stroke-linecap="round"
										stroke-dasharray="188.5"
										stroke-dashoffset="{188.5 - (188.5 * Math.min(100, (data.thread / 200) * 100) / 100)}"
										transform="rotate(-90 40 40)"
										style="transition: stroke-dashoffset 0.6s ease"
									/>
								</svg>
								<div class="gauge-inner">
									<span class="gauge-val mono">{data.thread}</span>
								</div>
							</div>
							<span class="gauge-label">Threads</span>
						</div>
					{/if}
				</div>

				<div class="health-sep"></div>

				<!-- Log count stats -->
				<div class="health-section">
					<div class="stat-row">
						<div class="stat-item">
							<span class="stat-val mono">{loading ? '—' : (data?.log ?? 0)}</span>
							<span class="stat-label">Total</span>
						</div>
						<div class="stat-sep"></div>
						<div class="stat-item">
							<span class="stat-val mono accent">{runningLogs.length}</span>
							<span class="stat-label">Running</span>
						</div>
						<div class="stat-sep"></div>
						<div class="stat-item">
							<span class="stat-val mono muted">{(data?.log ?? 0) - runningLogs.length}</span>
							<span class="stat-label">Stopped</span>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</div>

<style>
	.dash {
		display: flex;
		flex-direction: column;
		gap: 0;
		width: 100%;
		overflow-x: hidden;
		min-width: 0;
	}

	/* ── Top bar ── */
	.topbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-bottom: 1.25rem;
		border-bottom: 1px solid var(--border);
		margin-bottom: 1.25rem;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.topbar-left {
		display: flex;
		align-items: center;
		gap: 0.875rem;
	}

	.topbar-right {
		display: flex;
		align-items: center;
		gap: 0.625rem;
	}

	.dash-title {
		font-size: 1.25rem;
		font-weight: 700;
		margin: 0;
		letter-spacing: -0.02em;
	}

	.live-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.2rem 0.5rem;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--text-muted);
		background: var(--bg-surface);
		letter-spacing: 0.02em;
	}

	.live-badge.live {
		border-color: color-mix(in srgb, var(--success) 35%, transparent);
		color: var(--success);
		background: var(--success-light);
	}

	.live-badge.err {
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
		color: var(--danger);
		background: var(--danger-light);
	}

	.live-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	.live-badge.live .live-dot {
		animation: pulse-ring 2s ease-out infinite;
	}

	.error-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.5rem;
		background: var(--danger-light);
		color: var(--danger);
		border: 1px solid color-mix(in srgb, var(--danger) 25%, transparent);
		border-radius: var(--radius-sm);
		font-size: 0.6875rem;
		font-weight: 500;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.spin {
		animation: spin 0.8s linear infinite;
	}

	/* ── Metric strip ── */
	.metric-strip {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		background: var(--bg-surface);
		border: 1px solid var(--border);
		min-width: 0;
		border-radius: var(--radius-md);
		overflow: hidden;
		margin-bottom: 1.25rem;
	}

	.metric-tile {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.875rem 1rem;
		text-decoration: none;
		border-right: 1px solid var(--border);
		transition: background 0.12s;
	}

	.metric-tile:last-child {
		border-right: none;
	}

	.metric-tile:hover {
		background: var(--bg-raised);
	}

	.metric-icon {
		width: 30px;
		height: 30px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: var(--radius-sm);
		background: var(--accent-light);
		color: var(--accent);
		flex-shrink: 0;
	}

	.metric-body {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
	}

	.metric-value {
		font-size: 1.375rem;
		font-weight: 700;
		letter-spacing: -0.04em;
		line-height: 1;
		color: var(--text-primary);
	}

	.metric-label {
		font-size: 0.6875rem;
		color: var(--text-muted);
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	/* ── Getting Started Guide ── */
	.guide-panel {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 1rem 1.25rem;
		margin-bottom: 1.25rem;
	}

	.guide-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.375rem;
	}

	.guide-title-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--accent);
	}

	.guide-title {
		font-weight: 700;
		font-size: 0.875rem;
	}

	.guide-dismiss {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		padding: 4px;
		border-radius: var(--radius-sm);
		transition: color 0.12s, background 0.12s;
	}

	.guide-dismiss:hover {
		color: var(--text-primary);
		background: var(--bg-raised);
	}

	.guide-desc {
		font-size: 0.8125rem;
		color: var(--text-secondary);
		margin: 0 0 0.75rem;
	}

	.guide-steps {
		display: flex;
		align-items: center;
		gap: 0;
	}

	.guide-step {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		flex: 1;
		padding: 0.625rem 0.75rem;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		text-decoration: none;
		color: var(--text-primary);
		transition: border-color 0.12s, background 0.12s;
	}

	.guide-step:hover {
		border-color: var(--accent);
		background: var(--accent-light);
	}

	.step-num {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: var(--accent-light);
		color: var(--accent);
		font-size: 0.75rem;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.step-body {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.step-title {
		font-size: 0.8125rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.step-desc {
		font-size: 0.6875rem;
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.step-arrow {
		flex-shrink: 0;
		color: var(--text-muted);
	}

	.step-connector {
		width: 24px;
		height: 2px;
		background: var(--border);
		flex-shrink: 0;
	}

	@media (max-width: 700px) {
		.guide-steps {
			flex-direction: column;
			gap: 0.5rem;
		}
		.step-connector {
			width: 2px;
			height: 12px;
		}
	}

	/* ── Main 2-col grid ── */
	.main-grid {
		display: grid;
		grid-template-columns: 3fr 2fr;
		gap: 1.25rem;
		align-items: start;
		min-width: 0;
	}

	@media (max-width: 900px) {
		.metric-strip { grid-template-columns: repeat(2, 1fr); }
		.main-grid { grid-template-columns: 1fr; }
	}

	@media (max-width: 500px) {
		.metric-strip { grid-template-columns: 1fr; }
	}

	/* ── Panel ── */
	.panel {
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.625rem 0.875rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg-raised);
	}

	.panel-title {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.07em;
	}

	.panel-count {
		font-size: 0.6875rem;
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}

	/* ── Activity panel ── */
	.activity-list {
		display: flex;
		flex-direction: column;
	}

	.activity-loading {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 1.5rem;
		color: var(--text-muted);
		font-size: 0.8125rem;
	}

	.activity-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.625rem;
		padding: 2.5rem 1rem;
		color: var(--text-muted);
		font-size: 0.8125rem;
		text-align: center;
	}

	.activity-card {
		display: flex;
		flex-direction: column;
		gap: 0.4375rem;
		padding: 0.75rem 0.875rem;
		border-bottom: 1px solid var(--border);
		transition: background 0.12s;
	}

	.activity-card:last-child {
		border-bottom: none;
	}

	.activity-card:hover {
		background: color-mix(in srgb, var(--bg-raised) 50%, transparent);
	}

	.activity-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}

	.activity-name {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
	}

	.name-text {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.running-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--success);
		flex-shrink: 0;
		animation: pulse-ring 2s ease-out infinite;
	}

	.activity-eps {
		display: flex;
		align-items: baseline;
		gap: 0.2rem;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.eps-actual {
		font-size: 0.875rem;
		font-weight: 700;
		color: var(--text-primary);
	}

	.eps-sep {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	.eps-target {
		font-size: 0.8125rem;
		color: var(--text-secondary);
	}

	.eps-unit {
		font-size: 0.625rem;
		color: var(--text-muted);
		margin-left: 1px;
	}

	.activity-format {
		font-size: 0.6875rem;
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.activity-card-bottom {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}

	.sender-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}

	.stag {
		display: inline-block;
		padding: 0.1rem 0.4rem;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 0.625rem;
		color: var(--text-muted);
		font-weight: 500;
		font-family: var(--font-mono);
	}

	.stag-more {
		color: var(--text-muted);
		opacity: 0.7;
	}

	.eps-bar-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		flex-shrink: 0;
		min-width: 90px;
	}

	.eps-bar {
		flex: 1;
		height: 3px;
		background: var(--bg-raised);
		border-radius: 2px;
		overflow: hidden;
	}

	.eps-bar-fill {
		height: 100%;
		background: var(--accent);
		border-radius: 2px;
		transition: width 0.5s ease;
	}

	.eps-pct {
		font-size: 0.625rem;
		font-weight: 600;
		color: var(--text-muted);
		width: 2.25rem;
		text-align: right;
	}

	/* ── Health panel ── */
	.health-body {
		display: flex;
		flex-direction: column;
	}

	.health-section {
		padding: 0.875rem;
	}

	.health-sep {
		height: 1px;
		background: var(--border);
	}

	.section-label {
		font-size: 0.625rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-muted);
		margin-bottom: 0.625rem;
	}

	.eps-overview {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		margin-bottom: 0.625rem;
	}

	.eps-big {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
	}

	.eps-big-val {
		font-size: 1.625rem;
		font-weight: 700;
		letter-spacing: -0.04em;
		line-height: 1;
		color: var(--text-primary);
	}

	.eps-big-sub {
		font-size: 0.625rem;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.eps-divider {
		font-size: 1.25rem;
		color: var(--border);
		font-weight: 300;
		line-height: 1;
	}

	.h-bar-wrap {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.h-bar {
		flex: 1;
		height: 4px;
		background: var(--bg-raised);
		border-radius: 2px;
		overflow: hidden;
	}

	.h-bar-fill {
		height: 100%;
		background: var(--accent);
		border-radius: 2px;
		transition: width 0.6s ease;
	}

	.h-bar-pct {
		font-size: 0.625rem;
		font-weight: 600;
		color: var(--text-muted);
		width: 2.25rem;
		text-align: right;
	}

	/* ── Gauge rings ── */
	.gauges-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-around;
		padding: 0.875rem;
		gap: 0.5rem;
	}

	.gauge-block {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
	}

	.gauge-wrap {
		position: relative;
		width: 72px;
		height: 72px;
	}

	.gauge-inner {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.gauge-val {
		font-size: 0.8125rem;
		font-weight: 700;
		letter-spacing: -0.03em;
		color: var(--text-primary);
	}

	.gauge-label {
		font-size: 0.625rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-muted);
	}

	/* ── Stats row ── */
	.stat-row {
		display: flex;
		align-items: center;
		gap: 0;
	}

	.stat-item {
		display: flex;
		flex-direction: column;
		gap: 0.1875rem;
		flex: 1;
	}

	.stat-sep {
		width: 1px;
		height: 32px;
		background: var(--border);
		flex-shrink: 0;
		margin: 0 0.875rem;
	}

	.stat-val {
		font-size: 1.25rem;
		font-weight: 700;
		letter-spacing: -0.04em;
		line-height: 1;
		color: var(--text-primary);
	}

	.stat-val.accent {
		color: var(--accent);
	}

	.stat-val.muted {
		color: var(--text-secondary);
	}

	.stat-label {
		font-size: 0.625rem;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-weight: 500;
	}

	/* ── Responsive ── */
	@media (max-width: 1100px) {
		.main-grid {
			grid-template-columns: 1fr;
		}
	}

	@media (max-width: 700px) {
		.metric-strip {
			grid-template-columns: repeat(2, 1fr);
		}
		.metric-tile {
			border-bottom: 1px solid var(--border);
		}
		.metric-tile:nth-child(2n) {
			border-right: none;
		}
		.metric-tile:nth-last-child(-n+2) {
			border-bottom: none;
		}
	}
</style>
