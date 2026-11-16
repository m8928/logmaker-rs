<script lang="ts">
	import { page } from '$app/stores';
	import { api } from '$lib/api';

	let version = $state('');
	let buildTime = $state('');

	async function fetchVersion() {
		try {
			const d = await api.getDashboard();
			version = d.version ?? '';
			buildTime = d.buildTime ?? '';
		} catch { /* ignore */ }
	}
	fetchVersion();

	let isDark = $state(
		typeof document !== 'undefined' && !document.documentElement.classList.contains('light')
	);

	function toggleDark() {
		isDark = !isDark;
		if (isDark) {
			document.documentElement.classList.remove('light');
		} else {
			document.documentElement.classList.add('light');
		}
		localStorage.setItem('theme', isDark ? 'dark' : 'light');
	}

	const navItems = [
		{
			href: '/',
			label: 'Dashboard',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/></svg>`
		},
		{
			href: '/maker',
			label: 'Maker',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"/></svg>`
		},
		{
			href: '/sender',
			label: 'Sender',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>`
		},
		{
			href: '/log',
			label: 'Log',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>`
		},
		{
			href: '/scenario',
			label: 'Scenario',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="5" cy="12" r="2"/><circle cx="19" cy="5" r="2"/><circle cx="19" cy="19" r="2"/><line x1="7" y1="11.5" x2="17" y2="6.5"/><line x1="7" y1="12.5" x2="17" y2="17.5"/></svg>`
		},
		{
			href: '/plugin',
			label: 'Plugin',
			icon: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M20.24 12.24a6 6 0 00-8.49-8.49L5 10.5V19h8.5z"/><line x1="16" y1="8" x2="2" y2="22"/><line x1="17.5" y1="15" x2="9" y2="15"/></svg>`
		}
	];

	function isActive(href: string) {
		if (href === '/') return $page.url.pathname === '/';
		return $page.url.pathname.startsWith(href);
	}
</script>

<nav class="nav-sidebar" aria-label="Main navigation">
	<div class="nav-brand">
		<div class="brand-icon">
			<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.5" aria-hidden="true">
				<polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
			</svg>
		</div>
		<span class="nav-brand-text">LogMaker</span>
	</div>

	<ul class="nav-items" role="list">
		{#each navItems as item}
			<li>
				<a href={item.href} class="nav-item" class:active={isActive(item.href)} aria-current={isActive(item.href) ? 'page' : undefined}>
					<span class="nav-icon">{@html item.icon}</span>
					<span class="nav-label">{item.label}</span>
				</a>
			</li>
		{/each}
	</ul>

	<div class="nav-footer">
		{#if version}
			<div class="nav-version">
				<span class="version-label">v{version}</span>
				{#if buildTime}<span class="version-build">{buildTime.slice(0, 10)}</span>{/if}
			</div>
		{/if}
		<button
			onclick={toggleDark}
			class="theme-toggle"
			aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
		>
			{#if isDark}
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
					<circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
				</svg>
				<span>Light Mode</span>
			{:else}
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
					<path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"/>
				</svg>
				<span>Dark Mode</span>
			{/if}
		</button>
	</div>
</nav>

<style>
	.nav-sidebar {
		width: var(--sidebar-w);
		height: 100vh;
		position: fixed;
		left: 0;
		top: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		padding: 0;
		z-index: 100;
	}

	.nav-brand {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0 1rem;
		height: 52px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.brand-icon {
		width: 28px;
		height: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--accent-light);
		border-radius: var(--radius-sm);
		flex-shrink: 0;
	}

	.nav-brand-text {
		font-weight: 700;
		font-size: 0.9375rem;
		letter-spacing: -0.02em;
		color: var(--text-primary);
	}

	.nav-items {
		list-style: none;
		margin: 0;
		padding: 0.75rem 0.625rem;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.75rem;
		border-radius: var(--radius-sm);
		color: var(--text-secondary);
		text-decoration: none;
		font-size: 0.8125rem;
		font-weight: 500;
		transition: background 0.12s, color 0.12s;
	}

	.nav-item:hover {
		background: var(--bg-raised);
		color: var(--text-primary);
	}

	.nav-item.active {
		background: var(--accent-light);
		color: var(--accent);
		font-weight: 600;
		position: relative;
	}

	.nav-item.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 50%;
		transform: translateY(-50%);
		width: 2px;
		height: 60%;
		background: var(--accent);
		border-radius: 0 2px 2px 0;
	}

	.nav-icon {
		display: flex;
		align-items: center;
		flex-shrink: 0;
		opacity: 0.8;
	}

	.nav-item.active .nav-icon {
		opacity: 1;
	}

	.nav-footer {
		padding: 0.625rem;
		border-top: 1px solid var(--border);
		flex-shrink: 0;
	}

	.theme-toggle {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.5rem 0.75rem;
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 0.8125rem;
		font-weight: 500;
		font-family: var(--font-ui);
		background: none;
		border: none;
		cursor: pointer;
		transition: background 0.12s, color 0.12s;
	}

	.theme-toggle:hover {
		background: var(--bg-raised);
		color: var(--text-primary);
	}

	.nav-version {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.25rem 0.75rem 0.375rem;
		font-size: 0.625rem;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.version-label {
		font-weight: 600;
	}

	.version-build {
		opacity: 0.6;
	}

	@media (max-width: 768px) {
		.nav-sidebar {
			width: 100%;
			height: auto;
			position: sticky;
			top: 0;
			flex-direction: row;
			align-items: center;
			padding: 0;
			border-right: none;
			border-bottom: 1px solid var(--border);
		}

		.nav-brand {
			border-bottom: none;
			border-right: 1px solid var(--border);
			padding: 0 0.875rem;
			flex-shrink: 0;
		}

		.nav-items {
			flex-direction: row;
			flex: 1;
			gap: 2px;
			overflow-x: auto;
			padding: 0.5rem 0.5rem;
		}

		.nav-item.active::before {
			display: none;
		}

		.nav-label {
			display: none;
		}

		.nav-item {
			padding: 0.5rem;
			justify-content: center;
		}

		.nav-icon {
			opacity: 1;
		}

		.nav-footer {
			padding: 0 0.5rem;
			border-top: none;
			border-left: 1px solid var(--border);
		}

		.theme-toggle span {
			display: none;
		}

		.theme-toggle {
			padding: 0.5rem;
			justify-content: center;
		}
	}
</style>
