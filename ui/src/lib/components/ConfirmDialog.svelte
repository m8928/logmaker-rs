<script lang="ts">
	interface Props {
		open: boolean;
		title?: string;
		message?: string;
		confirmLabel?: string;
		loading?: boolean;
		onconfirm: () => void;
		oncancel: () => void;
	}

	let {
		open,
		title = 'Confirm Delete',
		message = 'Are you sure? This action cannot be undone.',
		confirmLabel = 'Delete',
		loading = false,
		onconfirm,
		oncancel
	}: Props = $props();
</script>

{#if open}
	<div class="overlay" role="presentation" onclick={oncancel}>
		<div
			class="dialog"
			role="alertdialog"
			aria-modal="true"
			tabindex="-1"
			aria-labelledby="confirm-title"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.key === 'Escape' && oncancel()}
		>
			<div class="dialog-icon-wrap">
				<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--danger)" stroke-width="2">
					<path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
					<line x1="12" y1="9" x2="12" y2="13" />
					<line x1="12" y1="17" x2="12.01" y2="17" />
				</svg>
			</div>
			<h3 id="confirm-title" class="dialog-title">{title}</h3>
			<p class="dialog-message">{message}</p>
			<div class="dialog-actions">
				<button class="btn btn-ghost" onclick={oncancel} disabled={loading}>Cancel</button>
				<button class="btn btn-danger" onclick={onconfirm} disabled={loading}>
					{#if loading}
						<span class="spinner"></span>
					{/if}
					{confirmLabel}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: var(--bg-overlay);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
		backdrop-filter: blur(4px);
	}

	.dialog {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 1.75rem 1.5rem 1.5rem;
		max-width: 340px;
		width: 90%;
		box-shadow: var(--shadow-lg);
		text-align: center;
		animation: pop-in 0.15s ease-out;
	}

	@keyframes pop-in {
		from {
			opacity: 0;
			transform: scale(0.96) translateY(4px);
		}
		to {
			opacity: 1;
			transform: scale(1) translateY(0);
		}
	}

	.dialog-icon-wrap {
		display: flex;
		justify-content: center;
		align-items: center;
		width: 44px;
		height: 44px;
		background: var(--danger-light);
		border: 1px solid color-mix(in srgb, var(--danger) 20%, transparent);
		border-radius: var(--radius-md);
		margin: 0 auto 1rem;
	}

	.dialog-title {
		font-size: 0.9375rem;
		font-weight: 600;
		margin: 0 0 0.5rem;
		color: var(--text-primary);
	}

	.dialog-message {
		color: var(--text-secondary);
		font-size: 0.8125rem;
		margin: 0 0 1.5rem;
		line-height: 1.5;
	}

	.dialog-actions {
		display: flex;
		gap: 0.625rem;
		justify-content: center;
	}

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.4375rem 1rem;
		border-radius: var(--radius-sm);
		font-size: 0.8125rem;
		font-weight: 500;
		font-family: var(--font-ui);
		border: 1px solid transparent;
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
	}

	.btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.btn-ghost {
		background: transparent;
		border-color: var(--border);
		color: var(--text-secondary);
	}

	.btn-ghost:hover:not(:disabled) {
		background: var(--bg-raised);
		color: var(--text-primary);
	}

	.btn-danger {
		background: var(--danger);
		color: white;
		border-color: var(--danger);
	}

	.btn-danger:hover:not(:disabled) {
		background: var(--danger-hover);
		border-color: var(--danger-hover);
	}

	.spinner {
		width: 12px;
		height: 12px;
		border: 2px solid rgba(255, 255, 255, 0.3);
		border-top-color: white;
		border-radius: 50%;
		animation: spin 0.6s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
