<script lang="ts">
	import { tick } from 'svelte';

	interface Option {
		value: string;
		label: string;
		sublabel?: string;
	}

	interface Props {
		value: string;
		options: Option[];
		placeholder?: string;
		disabled?: boolean;
		onchange: (value: string) => void;
		class?: string;
		id?: string;
		'aria-labelledby'?: string;
	}

	let {
		value,
		options,
		placeholder = 'Select…',
		disabled = false,
		onchange,
		class: extraClass = '',
		id,
		'aria-labelledby': ariaLabelledby
	}: Props = $props();

	let open = $state(false);
	let search = $state('');
	let activeIndex = $state(-1);
	let triggerEl = $state<HTMLButtonElement | null>(null);
	let dropdownEl = $state<HTMLDivElement | null>(null);
	let searchEl = $state<HTMLInputElement | null>(null);
	let dropStyle = $state('');

	const showSearch = $derived(options.length > 5);

	const filtered = $derived(
		search.trim()
			? options.filter(
					(o) =>
						o.label.toLowerCase().includes(search.toLowerCase()) ||
						(o.sublabel ?? '').toLowerCase().includes(search.toLowerCase())
				)
			: options
	);

	const selected = $derived(options.find((o) => o.value === value) ?? null);

	function updateDropdownPosition() {
		if (!triggerEl) return;
		const rect = triggerEl.getBoundingClientRect();
		dropStyle = `position:fixed;top:${rect.bottom + 3}px;left:${rect.left}px;width:${rect.width}px;z-index:9999;`;
	}

	async function openDropdown() {
		if (disabled) return;
		updateDropdownPosition();
		open = true;
		search = '';
		activeIndex = selected ? filtered.findIndex((o) => o.value === value) : 0;
		await tick();
		if (showSearch && searchEl) {
			searchEl.focus();
		} else {
			const items = dropdownEl?.querySelectorAll<HTMLElement>('[data-option]');
			items?.[Math.max(0, activeIndex)]?.focus();
		}
	}

	function closeDropdown() {
		open = false;
		search = '';
		activeIndex = -1;
		triggerEl?.focus();
	}

	function selectOption(optValue: string) {
		onchange(optValue);
		closeDropdown();
	}

	function handleTriggerKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
			e.preventDefault();
			openDropdown();
		}
	}

	function handleDropdownKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			closeDropdown();
			return;
		}
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			activeIndex = Math.min(activeIndex + 1, filtered.length - 1);
			focusActive();
			return;
		}
		if (e.key === 'ArrowUp') {
			e.preventDefault();
			activeIndex = Math.max(activeIndex - 1, 0);
			focusActive();
			return;
		}
		if (e.key === 'Enter') {
			e.preventDefault();
			const opt = filtered[activeIndex];
			if (opt) selectOption(opt.value);
			return;
		}
		if (e.key === 'Tab') {
			closeDropdown();
		}
	}

	function focusActive() {
		tick().then(() => {
			const items = dropdownEl?.querySelectorAll<HTMLElement>('[data-option]');
			items?.[activeIndex]?.focus();
		});
	}

	function handleClickOutside(e: MouseEvent) {
		if (!open) return;
		const target = e.target as Node;
		if (triggerEl?.contains(target) || dropdownEl?.contains(target)) return;
		closeDropdown();
	}

	function handleSearchInput() {
		activeIndex = 0;
	}

	function handleViewportChange() {
		if (open) updateDropdownPosition();
	}
</script>

<svelte:window onclick={handleClickOutside} onresize={handleViewportChange} onscroll={handleViewportChange} />

<div class="select-root {extraClass}" class:open class:disabled>
	<!-- Trigger -->
	<button
		bind:this={triggerEl}
		type="button"
		class="select-trigger input"
		class:open
		{id}
		aria-labelledby={ariaLabelledby}
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-disabled={disabled}
		{disabled}
		onclick={openDropdown}
		onkeydown={handleTriggerKeydown}
	>
		<span class="trigger-value" class:placeholder={!selected}>
			{selected ? selected.label : placeholder}
		</span>
		<svg
			class="chevron"
			class:rotated={open}
			width="12"
			height="12"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			stroke-width="2.5"
			aria-hidden="true"
		>
			<polyline points="6 9 12 15 18 9" />
		</svg>
	</button>

	<!-- Dropdown -->
	{#if open}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			bind:this={dropdownEl}
			class="select-dropdown"
			role="listbox"
			aria-label="Options"
			tabindex="-1"
			onkeydown={handleDropdownKeydown}
			style={dropStyle}
		>
			{#if showSearch}
				<div class="search-row">
					<svg
						class="search-ico"
						width="12"
						height="12"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						aria-hidden="true"
					>
						<circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
					</svg>
					<input
						bind:this={searchEl}
						class="search-input"
						type="text"
						placeholder="Filter…"
						autocomplete="off"
						bind:value={search}
						oninput={handleSearchInput}
					/>
				</div>
			{/if}

			<div class="options-list">
				{#if filtered.length === 0}
					<div class="no-results">No results</div>
				{:else}
					{#each filtered as opt, i}
						<!-- svelte-ignore a11y_interactive_supports_focus -->
						<div
							class="option"
							class:active={i === activeIndex}
							class:selected={opt.value === value}
							role="option"
							aria-selected={opt.value === value}
							data-option
							tabindex="-1"
							onclick={() => selectOption(opt.value)}
							onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && selectOption(opt.value)}
							onmouseenter={() => (activeIndex = i)}
						>
							<span class="option-selected-mark" aria-hidden="true">
								{#if opt.value === value}
									<svg
										width="10"
										height="10"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="3"
									>
										<polyline points="20 6 9 17 4 12" />
									</svg>
								{/if}
							</span>
							<span class="option-body">
								<span class="option-label">{opt.label}</span>
								{#if opt.sublabel}
									<span class="option-sublabel">{opt.sublabel}</span>
								{/if}
							</span>
						</div>
					{/each}
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.select-root {
		position: relative;
		width: 100%;
	}

	/* Trigger — inherits global .input styling, just override what differs */
	.select-trigger {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		cursor: pointer;
		text-align: left;
		user-select: none;
	}

	.select-trigger:focus {
		outline: none;
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.select-trigger.open {
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.select-trigger:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.trigger-value {
		flex: 1;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		font-size: 0.875rem;
		color: var(--text-primary);
		font-family: var(--font-ui);
	}

	.trigger-value.placeholder {
		color: var(--text-muted);
	}

	/* Chevron */
	.chevron {
		flex-shrink: 0;
		color: var(--text-muted);
		transition: transform 0.15s ease, color 0.15s;
	}

	.chevron.rotated {
		transform: rotate(180deg);
		color: var(--accent);
	}

	/* Dropdown panel — position:fixed via inline style to escape overflow:auto parents */
	.select-dropdown {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		overflow: hidden;
		animation: dropdown-open 0.1s ease-out;
	}

	@keyframes dropdown-open {
		from {
			opacity: 0;
			transform: translateY(-4px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	/* Search row */
	.search-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.625rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg-raised);
	}

	.search-ico {
		flex-shrink: 0;
		color: var(--text-muted);
	}

	.search-input {
		flex: 1;
		background: none;
		border: none;
		outline: none;
		color: var(--text-primary);
		font-size: 0.8125rem;
		font-family: var(--font-ui);
		padding: 0;
	}

	.search-input::placeholder {
		color: var(--text-muted);
	}

	/* Options list */
	.options-list {
		max-height: 200px;
		overflow-y: auto;
		padding: 0.25rem 0;
		overscroll-behavior: contain;
	}

	.options-list::-webkit-scrollbar {
		width: 4px;
	}

	.options-list::-webkit-scrollbar-thumb {
		background: var(--border);
		border-radius: 2px;
	}

	/* Option row */
	.option {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4375rem 0.625rem;
		cursor: pointer;
		transition: background 0.1s;
		outline: none;
	}

	.option:hover,
	.option.active {
		background: var(--bg-raised);
	}

	.option.selected {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}

	.option.selected.active,
	.option.selected:hover {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}

	/* Selected checkmark column — fixed width keeps labels aligned */
	.option-selected-mark {
		width: 14px;
		height: 14px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--accent);
	}

	.option-body {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
		min-width: 0;
		flex: 1;
	}

	.option-label {
		font-size: 0.875rem;
		color: var(--text-primary);
		font-family: var(--font-mono);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.3;
	}

	.option.selected .option-label {
		color: var(--accent);
		font-weight: 600;
	}

	.option-sublabel {
		font-size: 0.6875rem;
		color: var(--text-muted);
		font-family: var(--font-ui);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.no-results {
		padding: 0.75rem 1rem;
		font-size: 0.8125rem;
		color: var(--text-muted);
		text-align: center;
	}
</style>
