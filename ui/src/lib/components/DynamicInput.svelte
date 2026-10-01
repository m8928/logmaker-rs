<script lang="ts">
	interface Props {
		name: string;
		type: string;
		value: string | number | boolean | string[];
		required?: boolean;
		description?: string;
		error?: string;
		onchange: (name: string, value: string | number | boolean | string[]) => void;
	}

	let { name, type, value, required = false, description, error, onchange }: Props = $props();

	// Convert camelCase to spaced Title Case: connectionTimeout → Connection Timeout
	const displayName = $derived(
		name
			.replace(/([A-Z])/g, ' $1')
			.replace(/^./, (s) => s.toUpperCase())
			.trim()
	);

	// For ArrayList tag input
	let tagInput = $state('');

	function addTag() {
		if (!tagInput.trim()) return;
		const arr = Array.isArray(value) ? [...value] : [];
		if (!arr.includes(tagInput.trim())) {
			onchange(name, [...arr, tagInput.trim()]);
		}
		tagInput = '';
	}

	function removeTag(tag: string) {
		if (!Array.isArray(value)) return;
		onchange(
			name,
			value.filter((t) => t !== tag)
		);
	}

	function handleTagKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			addTag();
		}
	}

	const isNumber = $derived(type === 'integer' || type === 'number');
	const isBoolean = $derived(type === 'boolean');
	const isList = $derived(type === 'list');
	const isString = $derived(type === 'string' || (!isNumber && !isBoolean && !isList));
</script>

<div class="field">
	<label class="field-label" for="input-{name}">
		{displayName}
		{#if required}<span class="required">*</span>{/if}
	</label>

	{#if isString}
		<input
			id="input-{name}"
			class="input"
			class:input-error={error}
			type="text"
			value={String(value ?? '')}
			aria-invalid={error ? 'true' : undefined}
			aria-describedby={error ? `input-${name}-error` : undefined}
			oninput={(e) => onchange(name, (e.target as HTMLInputElement).value)}
		/>
	{:else if isNumber}
		<input
			id="input-{name}"
			class="input"
			class:input-error={error}
			type="number"
			value={value === '' || value == null ? '' : Number(value)}
			aria-invalid={error ? 'true' : undefined}
			aria-describedby={error ? `input-${name}-error` : undefined}
			oninput={(e) => {
				const raw = (e.target as HTMLInputElement).value;
				onchange(name, raw === '' ? '' : Number(raw));
			}}
		/>
	{:else if isBoolean}
		<label class="toggle-wrap">
			<input
				id="input-{name}"
				type="checkbox"
				class="sr-only"
				checked={Boolean(value)}
				onchange={(e) => onchange(name, (e.target as HTMLInputElement).checked)}
			/>
			<span class="toggle" class:on={Boolean(value)}>
				<span class="toggle-thumb"></span>
			</span>
			<span class="toggle-label">{Boolean(value) ? 'Enabled' : 'Disabled'}</span>
		</label>
	{:else if isList}
		<div class="tag-input-wrap">
			{#if Array.isArray(value) && value.length > 0}
				<div class="tags">
					{#each value as tag}
						<span class="tag">
							{tag}
							<button type="button" class="tag-remove" onclick={() => removeTag(tag)} aria-label="Remove {tag}">
								<svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" aria-hidden="true">
									<line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
								</svg>
							</button>
						</span>
					{/each}
				</div>
			{/if}
			<div class="tag-row">
				<input
					class="input"
					class:input-error={error}
					type="text"
					placeholder={required ? 'Required value' : 'Add value, press Enter'}
					bind:value={tagInput}
					aria-invalid={error ? 'true' : undefined}
					aria-describedby={error ? `input-${name}-error` : undefined}
					onkeydown={handleTagKeydown}
				/>
				<button type="button" class="btn-add" onclick={addTag}>Add</button>
			</div>
		</div>
	{/if}

	{#if error}
		<p id="input-{name}-error" class="field-error">{error}</p>
	{/if}

	{#if description}
		<p class="field-description">{description}</p>
	{/if}
</div>

<style>
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		margin-bottom: 1rem;
	}

	.field-label {
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--text-muted);
		letter-spacing: 0.07em;
		text-transform: uppercase;
	}

	.field-description {
		font-size: 0.75rem;
		color: var(--text-muted);
		margin: 0;
		line-height: 1.4;
	}

	.required {
		color: var(--danger);
		margin-left: 2px;
	}

	.input {
		width: 100%;
		padding: 0.5rem 0.75rem;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 0.875rem;
		font-family: var(--font-ui);
		transition: border-color 0.15s, box-shadow 0.15s;
	}

	.input::placeholder {
		color: var(--text-muted);
	}

	.input:focus {
		outline: none;
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.input-error {
		border-color: var(--danger);
	}

	.field-error {
		font-size: 0.75rem;
		color: var(--danger);
		margin: -0.125rem 0 0;
	}

	/* Toggle switch */
	.toggle-wrap {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		cursor: pointer;
		width: fit-content;
	}

	.toggle {
		position: relative;
		width: 36px;
		height: 20px;
		background: var(--border);
		border-radius: 10px;
		transition: background 0.2s;
		flex-shrink: 0;
	}

	.toggle.on {
		background: var(--accent);
	}

	.toggle-thumb {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		background: white;
		border-radius: 50%;
		transition: transform 0.2s;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
	}

	.toggle.on .toggle-thumb {
		transform: translateX(16px);
	}

	.toggle-label {
		font-size: 0.8125rem;
		color: var(--text-secondary);
	}

	/* Tag input */
	.tag-input-wrap {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.tags {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
	}

	.tag {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.175rem 0.375rem 0.175rem 0.5rem;
		background: var(--accent-light);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
		border-radius: var(--radius-sm);
		font-size: 0.75rem;
		font-weight: 500;
		font-family: var(--font-mono);
	}

	.tag-remove {
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		display: flex;
		align-items: center;
		color: var(--accent);
		opacity: 0.55;
		transition: opacity 0.15s;
	}

	.tag-remove:hover {
		opacity: 1;
	}

	.tag-row {
		display: flex;
		gap: 0.5rem;
	}

	.btn-add {
		padding: 0.5rem 0.75rem;
		background: var(--bg-raised);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		font-size: 0.8125rem;
		font-weight: 500;
		font-family: var(--font-ui);
		cursor: pointer;
		white-space: nowrap;
		transition: background 0.15s, color 0.15s;
	}

	.btn-add:hover {
		background: var(--accent);
		border-color: var(--accent);
		color: #000;
	}
</style>
