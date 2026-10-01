<script lang="ts">
	import { getToasts, removeToast } from '$lib/stores/toast.svelte';

	const icons = {
		success: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>`,
		error: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`,
		warning: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`,
		info: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>`
	};
</script>

<div class="toast-stack" aria-live="polite" aria-label="Notifications">
	{#each getToasts() as toast (toast.id)}
		<div class="toast toast-{toast.type}" role="alert">
			<span class="toast-icon">{@html icons[toast.type]}</span>
			<p class="toast-msg">{toast.message}</p>
			<button
				onclick={() => removeToast(toast.id)}
				class="toast-close"
				aria-label="Dismiss notification"
			>
				<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
					<line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
				</svg>
			</button>
		</div>
	{/each}
</div>

<style>
	.toast-stack {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 9999;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		pointer-events: none;
	}

	.toast {
		pointer-events: auto;
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.75rem 0.875rem;
		border-radius: var(--radius-md);
		border: 1px solid;
		min-width: 280px;
		max-width: 400px;
		animation: toast-in 0.18s ease-out;
		background: var(--bg-surface);
	}

	@keyframes toast-in {
		from {
			opacity: 0;
			transform: translateX(12px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.toast-success {
		border-color: color-mix(in srgb, var(--success) 30%, transparent);
		background: color-mix(in srgb, var(--bg-surface) 92%, var(--success));
	}

	.toast-success .toast-icon {
		color: var(--success);
	}

	.toast-error {
		border-color: color-mix(in srgb, var(--danger) 30%, transparent);
		background: color-mix(in srgb, var(--bg-surface) 92%, var(--danger));
	}

	.toast-error .toast-icon {
		color: var(--danger);
	}

	.toast-warning {
		border-color: color-mix(in srgb, var(--warning) 30%, transparent);
		background: color-mix(in srgb, var(--bg-surface) 92%, var(--warning));
	}

	.toast-warning .toast-icon {
		color: var(--warning);
	}

	.toast-info {
		border-color: color-mix(in srgb, var(--info) 30%, transparent);
		background: color-mix(in srgb, var(--bg-surface) 92%, var(--info));
	}

	.toast-info .toast-icon {
		color: var(--info);
	}

	.toast-icon {
		display: flex;
		align-items: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.toast-msg {
		flex: 1;
		font-size: 0.8125rem;
		color: var(--text-primary);
		line-height: 1.4;
		margin: 0;
		font-family: var(--font-ui);
	}

	.toast-close {
		flex-shrink: 0;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		padding: 0;
		display: flex;
		align-items: center;
		transition: color 0.15s;
		margin-top: 1px;
	}

	.toast-close:hover {
		color: var(--text-primary);
	}
</style>
