<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title = '',
		text = '',
		position = 'top',
		children
	}: {
		title?: string;
		text?: string;
		position?: 'top' | 'bottom' | 'left' | 'right';
		children: Snippet;
	} = $props();

	let visible = $state(false);
	let tipX = $state(0);
	let tipY = $state(0);
	let triggerEl: HTMLSpanElement | undefined = $state();

	function show() {
		if (!text && !title) return;
		if (triggerEl) {
			const rect = triggerEl.getBoundingClientRect();
			if (position === 'top') {
				tipX = rect.left + rect.width / 2;
				tipY = rect.top - 4;
			} else if (position === 'bottom') {
				tipX = rect.left + rect.width / 2;
				tipY = rect.bottom + 4;
			} else if (position === 'left') {
				tipX = rect.left - 4;
				tipY = rect.top + rect.height / 2;
			} else {
				tipX = rect.right + 4;
				tipY = rect.top + rect.height / 2;
			}
		}
		visible = true;
	}

	function hide() {
		visible = false;
	}

	function parseEntries(t: string): Array<{key: string; val: string}> | null {
		if (!t || !t.includes(':')) return null;
		const lines = t.split('\n').filter(l => l.includes(':'));
		if (lines.length === 0) return null;
		return lines.map(l => {
			const idx = l.indexOf(':');
			return { key: l.slice(0, idx).trim(), val: l.slice(idx + 1).trim() };
		});
	}
</script>

<span
	class="tooltip-trigger"
	role="group"
	bind:this={triggerEl}
	onmouseenter={show}
	onmouseleave={hide}
	onfocusin={() => show()}
	onfocusout={() => hide()}
>{@render children()}</span>

{#if visible && (text || title)}
	{@const entries = parseEntries(text)}
	<div
		class="tip"
		role="tooltip"
		style="position:fixed;z-index:9999;pointer-events:none;
			{position === 'top' ? `bottom:${window.innerHeight - tipY}px;left:${tipX}px;transform:translateX(-50%);` : ''}
			{position === 'bottom' ? `top:${tipY}px;left:${tipX}px;transform:translateX(-50%);` : ''}
			{position === 'left' ? `right:${window.innerWidth - tipX}px;top:${tipY}px;transform:translateY(-50%);` : ''}
			{position === 'right' ? `left:${tipX}px;top:${tipY}px;transform:translateY(-50%);` : ''}
		"
	>
		<table class="tip-table"><tbody>
			{#if title}
				<tr class="tip-title-row">
					<td colspan="2" class="tip-title">{title}</td>
				</tr>
			{/if}
			{#if entries}
				{#each entries as e}
					<tr>
						<td class="tip-key">{e.key}</td>
						<td class="tip-val">{e.val}</td>
					</tr>
				{/each}
			{:else if text}
				<tr>
					<td colspan="2" class="tip-text">{text}</td>
				</tr>
			{/if}
		</tbody></table>
	</div>
{/if}

<style>
	.tooltip-trigger {
		display: inline;
	}

	.tip-table {
		background: var(--bg-raised);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-collapse: collapse;
		border-radius: var(--radius-sm);
		min-width: 120px;
		max-width: 300px;
		box-shadow: var(--shadow-md);
		font-size: 11px;
		line-height: 1.3;
		overflow: hidden;
		font-family: var(--font-ui);
	}

	.tip-title {
		padding: 5px 10px 4px;
		font-weight: 600;
		font-size: 11px;
		color: var(--accent);
		background: color-mix(in srgb, var(--bg-raised) 70%, var(--accent));
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
	}

	.tip-key {
		padding: 3px 6px 3px 10px;
		color: var(--text-muted);
		white-space: nowrap;
		text-transform: uppercase;
		font-weight: 600;
		font-size: 10px;
		letter-spacing: 0.05em;
		vertical-align: baseline;
	}

	.tip-val {
		padding: 3px 10px 3px 4px;
		color: var(--text-primary);
		font-family: var(--font-mono);
		font-size: 11px;
		word-break: break-all;
		vertical-align: baseline;
	}

	.tip-text {
		padding: 5px 10px;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text-secondary);
		font-size: 11px;
	}
</style>
