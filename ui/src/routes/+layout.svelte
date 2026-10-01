<script lang="ts">
	import '../app.css';
	import Nav from '$lib/components/Nav.svelte';
	import Toast from '$lib/components/Toast.svelte';

	let { children } = $props();
	// Theme is applied in app.html inline script (before render) to prevent FOUC

	let serverDown = $state(false);
	let retryCount = $state(0);

	async function checkHealth() {
		try {
			const res = await fetch('/api/v1/dashboard', { signal: AbortSignal.timeout(3000) });
			if (res.ok) {
				if (serverDown) {
					serverDown = false;
					retryCount = 0;
				}
			} else {
				serverDown = true;
			}
		} catch {
			serverDown = true;
		}
	}

	$effect(() => {
		const id = setInterval(() => {
			checkHealth();
			if (serverDown) retryCount++;
		}, 5000);
		return () => clearInterval(id);
	});
</script>

<div class="app-shell">
	<Nav />
	<main class="app-main">
		{@render children()}
	</main>
</div>

<Toast />

{#if serverDown}
	<div class="server-down-overlay">
		<div class="server-down-box">
			<div class="server-down-icon">
				<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--danger)" stroke-width="1.5">
					<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
					<line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
				</svg>
			</div>
			<h2 class="server-down-title">Server Unreachable</h2>
			<p class="server-down-text">LogMaker 서버에 연결할 수 없습니다.<br/>서버가 복구되면 자동으로 재연결됩니다.</p>
			<div class="server-down-spinner">
				<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.5">
					<path d="M21 12a9 9 0 1 1-6.219-8.56"/>
				</svg>
				<span class="server-down-retry">재연결 시도 중… ({retryCount})</span>
			</div>
		</div>
	</div>
{/if}

<style>
	.app-shell {
		display: flex;
		min-height: 100vh;
	}

	.app-main {
		flex: 1;
		margin-left: var(--sidebar-w);
		padding: 2rem;
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow-x: hidden;
	}

	@media (max-width: 768px) {
		.app-shell {
			flex-direction: column;
		}
		.app-main {
			margin-left: 0;
			padding: 1rem;
		}
	}

	.server-down-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.75);
		backdrop-filter: blur(6px);
		z-index: 9000;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.server-down-box {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 2.5rem 3rem;
		text-align: center;
		box-shadow: var(--shadow-lg);
		max-width: 400px;
	}

	.server-down-icon {
		margin-bottom: 1rem;
		display: flex;
		justify-content: center;
	}

	.server-down-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--text-primary);
		margin: 0 0 0.5rem;
	}

	.server-down-text {
		font-size: 0.8125rem;
		color: var(--text-secondary);
		line-height: 1.6;
		margin: 0 0 1.25rem;
	}

	.server-down-spinner {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
	}

	.server-down-spinner svg {
		animation: spin 1s linear infinite;
	}

	.server-down-retry {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
