<script lang="ts">
	import { selectInstance } from '$lib/api';
	import { instances } from '$lib/stores/instance.svelte';

	let open = $state(false);

	function choose(name: string) {
		open = false;
		if (name !== instances.active) selectInstance(name, instances.current);
	}
</script>

<svelte:window onclick={() => (open = false)} onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<button
	class="switcher-trigger"
	aria-haspopup="menu"
	aria-expanded={open}
	aria-label="Switch instance (current: {instances.active})"
	onclick={(e) => {
		e.stopPropagation();
		open = !open;
	}}
>
	<span class="switcher-product">LogMaker</span>
	<span class="switcher-instance">
		<span class="switcher-name">{instances.active}</span>
		<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true">
			<polyline points="6 9 12 15 18 9" />
		</svg>
	</span>
</button>

{#if open}
	<div class="switcher-menu" role="menu" aria-label="Instances">
		<div class="switcher-menu-title">Instances</div>
		{#each instances.names as name}
			<button
				class="switcher-item"
				class:active={name === instances.active}
				role="menuitemradio"
				aria-checked={name === instances.active}
				onclick={() => choose(name)}
			>
				<span class="switcher-check" aria-hidden="true">
					{#if name === instances.active}
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
							<polyline points="20 6 9 17 4 12" />
						</svg>
					{/if}
				</span>
				<span>{name}</span>
			</button>
		{/each}
	</div>
{/if}

<style>
	.switcher-trigger {
		display: flex;
		flex: 1;
		flex-direction: column;
		align-items: flex-start;
		gap: 1px;
		min-width: 0;
		margin-left: -0.5rem;
		padding: 0.25rem 0.5rem;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		font-family: var(--font-ui);
		text-align: left;
		cursor: pointer;
		transition: background 0.12s;
	}

	.switcher-trigger:hover,
	.switcher-trigger[aria-expanded='true'] {
		background: var(--bg-raised);
	}

	.switcher-product {
		font-weight: 700;
		font-size: 0.9375rem;
		letter-spacing: -0.02em;
		line-height: 1.2;
		color: var(--text-primary);
	}

	.switcher-instance {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		max-width: 100%;
		color: var(--text-secondary);
	}

	.switcher-name {
		overflow: hidden;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		line-height: 1.2;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.switcher-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0.625rem;
		right: 0.625rem;
		z-index: 200;
		padding: 0.25rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		box-shadow: var(--shadow-lg);
	}

	.switcher-menu-title {
		padding: 0.375rem 0.625rem 0.25rem;
		font-size: 0.625rem;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.switcher-item {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		width: 100%;
		padding: 0.4375rem 0.625rem 0.4375rem 0.375rem;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text-secondary);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		text-align: left;
		cursor: pointer;
	}

	.switcher-item:hover {
		background: var(--bg-raised);
		color: var(--text-primary);
	}

	.switcher-item.active {
		color: var(--accent);
		font-weight: 600;
	}

	.switcher-check {
		display: flex;
		width: 14px;
		flex-shrink: 0;
	}

	@media (max-width: 768px) {
		/* The top bar is narrow: show only the instance. */
		.switcher-product {
			display: none;
		}

		.switcher-name {
			font-size: 0.8125rem;
		}

		.switcher-menu {
			left: 0.5rem;
			right: auto;
			width: 14rem;
		}
	}
</style>
