<script lang="ts">
	import { api, readJsonResponse, unwrapApiResult } from '$lib/api';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import { addToast } from '$lib/stores/toast.svelte';
	import type { ApiResult, Log, Maker, Sender } from '$lib/types';

	let items = $state<Log[]>([]);
	let senders = $state<Sender[]>([]);
	let makers = $state<Maker[]>([]);
	let loading = $state(false);
	let search = $state('');

	function formatBytes(b: number): string {
		if (b < 1024) return `${b} B`;
		if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
		if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
		return `${(b / (1024 * 1024 * 1024)).toFixed(2)} GB`;
	}
	let expandedOutputs = $state(new Set<string>());
	let overflowingOutputs = $state(new Set<string>());

	function checkOutputOverflow() {
		const els = document.querySelectorAll<HTMLElement>('[data-output-name]');
		const next = new Set<string>();
		els.forEach(el => {
			if (el.scrollWidth > el.clientWidth || el.scrollHeight > el.clientHeight + 2) {
				next.add(el.dataset.outputName!);
			}
		});
		overflowingOutputs = next;
	}

	$effect(() => {
		if (items.length >= 0) {
			setTimeout(checkOutputOverflow, 100);
		}
	});

	function onResize() {
		checkOutputOverflow();
	}

	// Sync hoveredMaker highlight in format editor
	$effect(() => {
		if (!formatTextarea) return;
		const spans = formatTextarea.querySelectorAll<HTMLElement>('[data-maker]');
		spans.forEach(el => {
			if (hoveredMaker && el.dataset.maker === hoveredMaker) {
				el.classList.add('hl-active');
			} else {
				el.classList.remove('hl-active');
			}
		});
	});
	let viewMode = $state<'grid' | 'table'>((typeof localStorage !== 'undefined' && localStorage.getItem('logmaker-viewMode-log') as 'grid' | 'table') || 'grid');
	$effect(() => { localStorage.setItem('logmaker-viewMode-log', viewMode); });

	let dialogOpen = $state(false);
	let editMode = $state(false);
	let maximized = $state(false);
	let importOpen = $state(false);
	let confirmOpen = $state(false);
	let confirmName = $state('');
	let confirmLoading = $state(false);

	// Form
	let formName = $state('');
	let formFormat = $state('');
	let formEps = $state(0);
	let formEpsUnit = $state<'events' | 'bytes'>('events');
	let formEpsTimeUnit = $state<'sec' | 'min' | 'hour' | 'day'>('sec');

	const byteScales = [
		{ label: 'B', factor: 1 },
		{ label: 'KB', factor: 1024 },
		{ label: 'MB', factor: 1024 * 1024 },
		{ label: 'GB', factor: 1024 * 1024 * 1024 },
	] as const;
	let formByteScaleIdx = $state(0);

	const rateOptions = [
		{ value: 'evt-sec', label: 'evt/s' },
		{ value: 'evt-min', label: 'evt/m' },
		{ value: 'evt-hour', label: 'evt/h' },
		{ value: 'evt-day', label: 'evt/d' },
		{ value: 'B-sec', label: 'B/s' },
		{ value: 'B-min', label: 'B/m' },
		{ value: 'B-hour', label: 'B/h' },
		{ value: 'B-day', label: 'B/d' },
		{ value: 'KB-sec', label: 'KB/s' },
		{ value: 'KB-min', label: 'KB/m' },
		{ value: 'KB-hour', label: 'KB/h' },
		{ value: 'KB-day', label: 'KB/d' },
		{ value: 'MB-sec', label: 'MB/s' },
		{ value: 'MB-min', label: 'MB/m' },
		{ value: 'MB-hour', label: 'MB/h' },
		{ value: 'MB-day', label: 'MB/d' },
		{ value: 'GB-sec', label: 'GB/s' },
		{ value: 'GB-min', label: 'GB/m' },
		{ value: 'GB-hour', label: 'GB/h' },
		{ value: 'GB-day', label: 'GB/d' },
	];

	function getRateKey(): string {
		const unit = formEpsUnit === 'events' ? 'evt' : byteScales[formByteScaleIdx].label;
		return `${unit}-${formEpsTimeUnit}`;
	}

	function applyRateKey(key: string) {
		const [unit, time] = key.split('-');
		formEpsTimeUnit = (time as 'sec' | 'min' | 'hour' | 'day');
		if (unit === 'evt') {
			formEpsUnit = 'events';
			formByteScaleIdx = 0;
		} else {
			formEpsUnit = 'bytes';
			formByteScaleIdx = byteScales.findIndex(s => s.label === unit);
		}
	}

	function detectByteScale(rawBytes: number): { value: number; idx: number } {
		for (let i = byteScales.length - 1; i > 0; i--) {
			if (rawBytes >= byteScales[i].factor && rawBytes % byteScales[i].factor === 0) {
				return { value: rawBytes / byteScales[i].factor, idx: i };
			}
		}
		return { value: rawBytes, idx: 0 };
	}
	let formSenders = $state<string[]>([]);
	let previewText = $state('');
	let previewLoading = $state(false);
	let hoveredMaker = $state<string | null>(null);
	let makerSearch = $state('');
	let makerPaletteOpen = $state(false);

	// Floating tooltip (no wrapper element needed)
	let ftip = $state({ show: false, x: 0, y: 0, title: '', text: '' });

	function showMakerTip(e: MouseEvent, makerName: string) {
		const mk = makers.find(m => m.name === makerName);
		const rect = (e.target as HTMLElement).getBoundingClientRect();
		ftip = {
			show: true,
			x: rect.left + rect.width / 2,
			y: rect.top - 4,
			title: makerName,
			text: 'TYPE: ' + (mk?.type ?? '?') + '\nSAMPLE: ' + (mk?.sample ?? '?')
		};
	}

	function hideMakerTip() {
		ftip.show = false;
	}

	let rateDropOpen = $state(false);
	let rateDropEl = $state<HTMLDivElement | null>(null);

	function handleRateClickOutside(e: MouseEvent) {
		if (!rateDropOpen) return;
		if (rateDropEl?.contains(e.target as Node)) return;
		const trigger = (e.target as HTMLElement).closest('.rate-select-trigger');
		if (trigger) return;
		rateDropOpen = false;
	}

	let formatTextarea = $state<HTMLDivElement | null>(null);

	const filtered = $derived(
		search.trim()
			? items.filter((i) => i.name.toLowerCase().includes(search.toLowerCase()))
			: items
	);

	const filteredMakers = $derived(
		makerSearch.trim()
			? makers.filter((m) =>
					m.name.toLowerCase().includes(makerSearch.toLowerCase()) ||
					m.type.toLowerCase().includes(makerSearch.toLowerCase())
				)
			: makers
	);

	async function fetchItems() {
		loading = true;
		try {
			[items] = await Promise.all([api.getLogs(), fetchSupport()]);
		} catch {
			/* toast shown */
		} finally {
			loading = false;
		}
	}

	async function fetchSupport() {
		try {
			[senders, makers] = await Promise.all([api.getSenders(), api.getMakers()]);
		} catch {
			/* ignored */
		}
	}

	function openAdd() {
		editMode = false;
		formName = '';
		formFormat = '';
		formEps = 0;
		formEpsUnit = 'events';
		formEpsTimeUnit = 'sec';
		formByteScaleIdx = 0;
		formSenders = [];
		previewText = '';
		makerSearch = '';
		makerPaletteOpen = false;
		dialogOpen = true;
		fetchSupport();
	}

	function openEdit(item: Log) {
		editMode = true;
		formName = item.name;
		formFormat = item.format;
		formEps = item.eps;
		formEpsUnit = item.epsUnit ?? 'events';
		formEpsTimeUnit = item.epsTimeUnit ?? 'sec';
		if (formEpsUnit === 'bytes') {
			const d = detectByteScale(item.eps);
			formEps = d.value;
			formByteScaleIdx = d.idx;
		} else {
			formByteScaleIdx = 0;
		}
		formSenders = [...item.sender];
		previewText = '';
		makerSearch = '';
		makerPaletteOpen = false;
		dialogOpen = true;
		fetchSupport();
		setTimeout(() => {
			if (formatTextarea) formatTextarea.textContent = item.format;
			highlightFormat();
		}, 10);
		runPreview();
	}

	function openCopy(item: Log) {
		editMode = false;
		formName = 'copy-of-' + item.name;
		formFormat = item.format;
		formEps = item.eps;
		formEpsUnit = item.epsUnit ?? 'events';
		formEpsTimeUnit = item.epsTimeUnit ?? 'sec';
		if (formEpsUnit === 'bytes') {
			const d = detectByteScale(item.eps);
			formEps = d.value;
			formByteScaleIdx = d.idx;
		} else {
			formByteScaleIdx = 0;
		}
		formSenders = [...item.sender];
		previewText = '';
		makerSearch = '';
		makerPaletteOpen = false;
		dialogOpen = true;
		fetchSupport();
	}

	function closeDialog() {
		dialogOpen = false;
		maximized = false;
		previewText = '';
	}

	async function runPreview() {
		if (!formFormat) return;
		previewLoading = true;
		try {
			const result = await api.previewLog({
				name: formName,
				format: formFormat,
				eps: formEps,
				sender: formSenders
			});
			previewText = result.message ?? '';
		} catch (err: unknown) {
			previewText = err instanceof Error ? err.message : 'Preview error';
		} finally {
			previewLoading = false;
		}
	}

	function insertMaker(makerName: string) {
		const token = `<${makerName}>`;
		if (!formatTextarea) {
			formFormat = formFormat + token;
		} else {
			insertTextAtCursor(token);
		}
		runPreview();
	}

	function insertTextAtCursor(text: string) {
		if (!formatTextarea) {
			formFormat += text;
			return;
		}

		formatTextarea.focus();
		const selection = window.getSelection();
		const range = getFormatEditorRange(selection);
		range.deleteContents();
		const textNode = document.createTextNode(text);
		range.insertNode(textNode);
		range.setStartAfter(textNode);
		range.setEndAfter(textNode);
		selection?.removeAllRanges();
		selection?.addRange(range);
		formFormat = formatTextarea.textContent ?? '';
	}

	function getFormatEditorRange(selection: Selection | null) {
		if (formatTextarea && selection && selection.rangeCount > 0) {
			const range = selection.getRangeAt(0);
			if (formatTextarea === range.commonAncestorContainer || formatTextarea.contains(range.commonAncestorContainer)) {
				return range;
			}
		}

		const range = document.createRange();
		range.selectNodeContents(formatTextarea!);
		range.collapse(false);
		return range;
	}

	function toggleMakerPalette() {
		makerPaletteOpen = !makerPaletteOpen;
		if (!makerPaletteOpen) makerSearch = '';
	}

	// Highlight <makerName> in contenteditable
	function highlightFormat() {
		if (!formatTextarea || !formFormat) return;
		// Save cursor position
		const sel = window.getSelection();
		let cursorOffset = 0;
		if (sel && sel.rangeCount > 0) {
			const range = sel.getRangeAt(0);
			const preRange = document.createRange();
			preRange.selectNodeContents(formatTextarea);
			preRange.setEnd(range.startContainer, range.startOffset);
			cursorOffset = preRange.toString().length;
		}
		// Build highlighted HTML
		const segments = parseFormatSegments(formFormat);
		let html = '';
		for (const seg of segments) {
			if (seg.maker) {
				html += `<span class="hl-maker" data-maker="${seg.maker}">${seg.text.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</span>`;
			} else {
				html += seg.text.replace(/</g, '&lt;').replace(/>/g, '&gt;');
			}
		}
		formatTextarea.innerHTML = html;
		// Restore cursor
		try {
			const newSel = window.getSelection();
			if (newSel) {
				const newRange = document.createRange();
				const node = formatTextarea;
				let offset = 0;
				const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
				let textNode: Node | null = null;
				while ((textNode = walker.nextNode())) {
					const len = (textNode.textContent ?? '').length;
					if (offset + len >= cursorOffset) {
						newRange.setStart(textNode, cursorOffset - offset);
						newRange.collapse(true);
						newSel.removeAllRanges();
						newSel.addRange(newRange);
						break;
					}
					offset += len;
				}
			}
		} catch { /* ignore cursor restore errors */ }
	}

	$effect(() => {
		if (formFormat !== undefined) {
			// Small delay to not interfere with typing
			const id = setTimeout(highlightFormat, 50);
			return () => clearTimeout(id);
		}
	});

	function toggleSender(name: string) {
		if (formSenders.includes(name)) {
			formSenders = formSenders.filter((s) => s !== name);
		} else {
			formSenders = [...formSenders, name];
		}
	}

	async function submit() {
		if (!formName.trim() || !formFormat.trim()) {
			addToast('warning', 'Name and Format are required');
			return;
		}
		loading = true;
		try {
			const actualEps = formEpsUnit === 'bytes' ? formEps * byteScales[formByteScaleIdx].factor : formEps;
			const payload = { name: formName, format: formFormat, eps: actualEps, epsUnit: formEpsUnit, epsTimeUnit: formEpsTimeUnit, sender: formSenders };
			if (editMode) await api.updateLog(formName, payload);
			else await api.createLog(payload);
			closeDialog();
			await fetchItems();
		} catch {
			/* toast shown */
		} finally {
			loading = false;
		}
	}

	async function toggleLog(name: string, isPaused: boolean) {
		try {
			if (isPaused) await api.startLog(name);
			else await api.stopLog(name);
			await fetchItems();
		} catch { /* toast shown */ }
	}

	function askDelete(name: string) {
		confirmName = name;
		confirmOpen = true;
	}

	async function confirmDelete() {
		confirmLoading = true;
		try {
			await api.deleteLog(confirmName);
			confirmOpen = false;
			await fetchItems();
		} catch {
			/* toast shown */
		} finally {
			confirmLoading = false;
		}
	}

	async function exportData() {
		const data = await api.getLogs();
		const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
		const a = document.createElement('a');
		a.href = URL.createObjectURL(blob);
		a.download = 'logmaker-log.json';
		a.click();
	}

	async function handleImport(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (!file) return;
		loading = true;
		try {
			const fd = new FormData();
			fd.append('file', file);
			const res = await fetch('/api/v1/log:import-file', { method: 'POST', body: fd });
			const result = await readJsonResponse<Array<ApiResult | { body?: ApiResult }>>(res, `Import failed (${res.status})`);
			if (!res.ok) {
				throw new Error(`Import failed (${res.status})`);
			}
			const entries = Array.isArray(result) ? result.map(unwrapApiResult) : [];
			if (entries.some((r) => r.type === 'ERROR')) {
				addToast('error', 'Import failed for some entries');
			} else {
				addToast('success', 'Import successful');
				importOpen = false;
			}
			await fetchItems();
		} catch (err) {
			addToast('error', err instanceof Error ? err.message : 'Import failed');
		} finally {
			loading = false;
		}
	}

	// Extract maker names from format string
	function extractMakers(format: string): string[] {
		const matches = format.match(/<([^>]+)>/g);
		if (!matches) return [];
		return [...new Set(matches.map((m) => m.slice(1, -1)))];
	}

	// Parse format into segments: [{text, maker?}]
	function parseFormatSegments(format: string): Array<{ text: string; maker?: string }> {
		const segments: Array<{ text: string; maker?: string }> = [];
		const regex = /<([^>]+)>/g;
		let lastIndex = 0;
		let match;
		while ((match = regex.exec(format)) !== null) {
			if (match.index > lastIndex) {
				segments.push({ text: format.slice(lastIndex, match.index) });
			}
			segments.push({ text: `<${match[1]}>`, maker: match[1] });
			lastIndex = regex.lastIndex;
		}
		if (lastIndex < format.length) {
			segments.push({ text: format.slice(lastIndex) });
		}
		return segments;
	}

	function getMakerTitle(name: string): string {
		const mk = makers.find(m => m.name === name);
		return mk ? `${name} — ${mk.type}` : name;
	}

	function getMakerDetail(name: string): string {
		const mk = makers.find(m => m.name === name);
		if (!mk) return `name: ${name}`;
		const lines: string[] = [];
		for (const [k, v] of Object.entries(mk.args || {})) {
			const val = Array.isArray(v) ? v.join(', ') : String(v);
			lines.push(`${k}: ${val}`);
		}
		lines.push(`ref: ${mk.ref}`);
		return lines.join('\n');
	}

	function getSenderTitle(name: string): string {
		const sn = senders.find(s => s.name === name);
		return sn ? `${name} — ${sn.type}` : name;
	}

	function getSenderDetail(name: string): string {
		const sn = senders.find(s => s.name === name);
		if (!sn) return '';
		const entries = Object.entries(sn.args || {}).slice(0, 4);
		const lines = entries.map(([k, v]) => `${k}: ${v}`);
		lines.push(`output: ${(sn.output ?? 0).toLocaleString()}`);
		return lines.join('\n');
	}

	// Map sample output back to format: find which parts of the sample came from which maker
	function mapSampleToFormat(format: string, sample: string): Array<{ text: string; maker?: string }> {
		if (!sample || !format) return [{ text: sample || format || '' }];
		const segments = parseFormatSegments(format);
		const makerSlots: string[] = [];
		for (const seg of segments) {
			if (seg.maker) {
				makerSlots.push(seg.maker);
			}
		}
		if (makerSlots.length === 0) return [{ text: sample }];

		let remaining = sample;
		const result: Array<{ text: string; maker?: string }> = [];
		let segIdx = 0;
		for (const seg of segments) {
			if (!seg.maker) {
				const pos = remaining.indexOf(seg.text);
				if (pos > 0) {
					result.push({ text: remaining.slice(0, pos) });
				}
				if (pos >= 0) {
					result.push({ text: seg.text });
					remaining = remaining.slice(pos + seg.text.length);
				} else {
					result.push({ text: remaining });
					remaining = '';
					break;
				}
			} else {
				const nextStatic = segments.slice(segIdx + 1).find(s => !s.maker);
				if (nextStatic) {
					const endPos = remaining.indexOf(nextStatic.text);
					if (endPos >= 0) {
						result.push({ text: remaining.slice(0, endPos), maker: seg.maker });
						remaining = remaining.slice(endPos);
					} else {
						result.push({ text: remaining, maker: seg.maker });
						remaining = '';
						break;
					}
				} else {
					result.push({ text: remaining, maker: seg.maker });
					remaining = '';
				}
			}
			segIdx++;
		}
		if (remaining) result.push({ text: remaining });
		return result;
	}

	function timeUnitLabel(tu?: string): string {
		return tu === 'min' ? '/m' : tu === 'hour' ? '/h' : tu === 'day' ? '/d' : '/s';
	}

	function timeDivisor(tu?: string): number {
		return tu === 'min' ? 60 : tu === 'hour' ? 3600 : tu === 'day' ? 86400 : 1;
	}

	function epsPct(log: Log): number {
		if (log.eps <= 0) return 0;
		const div = timeDivisor(log.epsTimeUnit);
		const actual = log.epsUnit === 'bytes' ? (log.bytesPerSec ?? 0) : log.currentEps;
		const targetPerSec = log.eps / div;
		const raw = (actual / targetPerSec) * 100;
		if (raw > 0 && raw < 1) return Math.max(0.1, parseFloat(raw.toFixed(1)));
		return Math.min(100, Math.round(raw));
	}

	// Truncate format string for table display, keep <maker> parts visually intact
	function truncateFormat(format: string, maxLen = 52): string {
		if (format.length <= maxLen) return format;
		return format.slice(0, maxLen) + '…';
	}

	// Auto-refresh every 5 seconds for live EPS/count
	$effect(() => {
		fetchItems();
		const interval = setInterval(fetchItems, 5000);
		return () => clearInterval(interval);
	});
</script>

<svelte:window onresize={onResize} onclick={handleRateClickOutside} />
<svelte:head><title>Log — LogMaker</title></svelte:head>

<div class="page">
	<header class="page-header">
		<div class="header-left">
			<h1 class="page-title">Log</h1>
			<span class="item-count">{filtered.length} of {items.length}</span>
			<span class="page-hint">Log pipelines — combine makers into a format template, set EPS, and send to destinations.</span>
		</div>
		<div class="header-actions">
			<div class="search-wrap">
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
				<input class="search-input" type="search" placeholder="Search logs…" bind:value={search} aria-label="Search logs" />
			</div>
			<button class="btn btn-ghost" onclick={fetchItems} disabled={loading}>
				{#if loading}
					<span class="spinner-muted"></span>
				{:else}
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
				{/if}
				Reload
			</button>
			<div class="view-toggle" role="radiogroup" aria-label="View mode">
				<button
					class="toggle-btn"
					class:active={viewMode === 'grid'}
					onclick={() => (viewMode = 'grid')}
					aria-label="Grid view"
					aria-pressed={viewMode === 'grid'}
				>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>
				</button>
				<button
					class="toggle-btn"
					class:active={viewMode === 'table'}
					onclick={() => (viewMode = 'table')}
					aria-label="Table view"
					aria-pressed={viewMode === 'table'}
				>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
				</button>
			</div>
			<button class="btn btn-ghost" onclick={() => (importOpen = true)} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
				Import
			</button>
			<button class="btn btn-ghost" onclick={exportData} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
				Export
			</button>
			<button class="btn btn-primary" onclick={openAdd} disabled={loading}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Add Log
			</button>
		</div>
	</header>

	{#if loading && items.length === 0}
		<div class="loading-state">
			<span class="spinner-muted"></span>
			<span>Loading logs…</span>
		</div>
	{:else if items.length === 0}
		<div class="empty-state">
			<div class="empty-state-icon">
				<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
			</div>
			<p class="empty-state-title">No logs yet</p>
			<p class="empty-state-desc">Create a log definition to start generating and sending events</p>
			<button class="btn btn-primary" onclick={openAdd}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
				Add Log
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
	{:else if viewMode === 'grid'}
		<div class="pipeline-grid" role="list" aria-label="Log pipeline list">
			{#each filtered as item}
				{@const running = !item.paused && item.eps > 0 && item.sender.length > 0 && (item.currentEps > 0 || (item.bytesPerSec ?? 0) > 0)}
				{@const pct = epsPct(item)}
				{@const lagging = running && pct < 80}
				{@const makerNames = extractMakers(item.format)}
				<div
					class="pipeline-card"
					class:running
					class:lagging
					role="button"
					onclick={() => openEdit(item)}
					onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && openEdit(item)}
					tabindex="0"
					aria-label="Edit log {item.name}"
				>
					<!-- Card header row -->
					<div class="pipeline-header">
						<div class="pipeline-name-block">
							<span class="pipeline-name">{item.name}</span>
							{#if item.description}
								<span class="pipeline-desc">{item.description}</span>
							{/if}
						</div>
						<div class="pipeline-status" class:running class:lagging>
							<span class="status-dot" class:pulse={running}></span>
							{lagging ? 'Lag' : running ? 'Running' : 'Stopped'}
						</div>
					</div>

					<!-- Makers / Senders inline bar -->
					<div class="pipeline-bar">
						<div class="bar-section">
							<span class="bar-label">Makers</span>
							<div class="bar-chips">
								{#each makerNames as m}
									<Tooltip title={getMakerTitle(m)} text={getMakerDetail(m)} position="bottom">
										<span class="chip chip-maker">{m}</span>
									</Tooltip>
								{/each}
								{#if makerNames.length === 0}
									<span class="chip chip-empty">none</span>
								{/if}
							</div>
						</div>
						<div class="bar-sep" aria-hidden="true"></div>
						<div class="bar-section">
							<span class="bar-label">Senders</span>
							<div class="bar-chips">
								{#each item.sender as s}
									<Tooltip title={getSenderTitle(s)} text={getSenderDetail(s)} position="bottom">
										<span class="chip chip-sender">{s}</span>
									</Tooltip>
								{/each}
								{#if item.sender.length === 0}
									<span class="chip chip-empty">none</span>
								{/if}
							</div>
						</div>
					</div>

					<!-- Log output: sample with hoverable maker-generated parts -->
					<div class="pipeline-body">
						<div class="body-label">Output</div>
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="output-line mono" class:expanded={expandedOutputs.has(item.name)} data-output-name={item.name}>{#if item.sample}{#each mapSampleToFormat(item.format, item.sample) as seg}{#if seg.maker}<span class="out-maker" onmouseenter={(e) => showMakerTip(e, seg.maker ?? '')} onmouseleave={hideMakerTip}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{:else}{#each parseFormatSegments(item.format) as seg}{#if seg.maker}<span class="out-maker" onmouseenter={(e) => showMakerTip(e, seg.maker ?? '')} onmouseleave={hideMakerTip}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{/if}</div>
						{#if overflowingOutputs.has(item.name) || expandedOutputs.has(item.name)}
							<button
								class="output-toggle"
								onclick={(e) => { e.stopPropagation(); const s = new Set(expandedOutputs); if (s.has(item.name)) s.delete(item.name); else s.add(item.name); expandedOutputs = s; }}
							>
								{expandedOutputs.has(item.name) ? '▲ Less' : '▼ More'}
							</button>
						{/if}
					</div>

					<!-- EPS + Count metrics row -->
					<div class="pipeline-metrics">
						<div class="metric-item">
							<span class="metric-label-sm">Target</span>
							<span class="metric-count-val">{item.epsUnit === 'bytes' ? formatBytes(item.eps) + timeUnitLabel(item.epsTimeUnit) : item.eps.toLocaleString() + ' evt' + timeUnitLabel(item.epsTimeUnit)}</span>
						</div>
						<div class="eps-bar-wrap">
							<div class="eps-bar">
								<div
									class="eps-bar-fill"
									style="width:{pct}%;background:{pct >= 90 ? 'var(--success)' : pct >= 50 ? 'var(--accent)' : running ? 'var(--warning)' : 'var(--border)'}"
								></div>
							</div>
							<span class="eps-pct">{pct}%</span>
						</div>
						<div class="metric-item">
							<span class="metric-label-sm">Current</span>
							<span class="metric-count-val">{item.currentEps.toLocaleString()} evt/s · {formatBytes(item.bytesPerSec ?? 0)}/s</span>
						</div>
						<div class="metric-item metric-count">
							<span class="metric-label-sm">Total</span>
							<span class="metric-count-val">{item.count.toLocaleString()} evt · {formatBytes(item.bytes ?? 0)}</span>
						</div>
					</div>

					<!-- Card footer actions -->
					<div
						class="pipeline-footer"
						role="group"
						aria-label="Actions"
					>
						{#if item.eps > 0 && item.sender.length > 0}
							<button
								class="btn btn-sm {item.paused ? 'btn-primary' : 'btn-ghost btn-danger-ghost'}"
								onclick={(e) => { e.stopPropagation(); toggleLog(item.name, !!item.paused); }}
								aria-label="{item.paused ? 'Start' : 'Stop'} {item.name}"
							>
								{#if item.paused}
									<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
									Start
								{:else}
									<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
									Stop
								{/if}
							</button>
						{/if}
						<button
							class="btn btn-ghost btn-sm"
							onclick={(e) => { e.stopPropagation(); openCopy(item); }}
							aria-label="Duplicate {item.name}"
						>
							<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
							Duplicate
						</button>
						<button
							class="btn btn-ghost btn-sm"
							onclick={(e) => { e.stopPropagation(); openEdit(item); }}
							aria-label="Edit {item.name}"
						>
							<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
							Edit
						</button>
						<button
							class="btn btn-ghost btn-sm btn-danger-ghost"
							onclick={(e) => { e.stopPropagation(); askDelete(item.name); }}
							aria-label="Delete {item.name}"
						>
							<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
							Delete
						</button>
					</div>
				</div>
			{/each}
		</div>
	{:else}
		<!-- Table view -->
		<div class="table-wrap">
			<table class="table" aria-label="Log list">
				<thead>
					<tr>
						<th>Name</th>
						<th>Status</th>
						<th>Format</th>
						<th class="right">Throughput</th>
						<th class="right">Total</th>
						<th>Senders</th>
						<th class="right">Actions</th>
					</tr>
				</thead>
				<tbody>
					{#each filtered as item}
						{@const running = !item.paused && item.eps > 0 && item.sender.length > 0 && (item.currentEps > 0 || (item.bytesPerSec ?? 0) > 0)}
						{@const pct = epsPct(item)}
						{@const lagging = running && pct < 80}
						{@const formatSegs = parseFormatSegments(truncateFormat(item.format))}
						<tr
							class="table-row-clickable"
							onclick={() => openEdit(item)}
							onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && openEdit(item)}
							tabindex="0"
							role="button"
							aria-label="Edit log {item.name}"
						>
							<td>
								<span class="tbl-name mono">{item.name}</span>
							</td>
							<td>
									<span class="tbl-status" class:tbl-status-running={running && !lagging} class:tbl-status-lagging={lagging}>
										<span class="tbl-status-dot" class:pulse={running}></span>
										{lagging ? 'Lagging' : running ? 'Running' : 'Stopped'}
									</span>
								</td>
							<td class="tbl-format-cell">
								<span class="tbl-format mono">{#each formatSegs as seg}{#if seg.maker}<span class="tbl-format-maker">{seg.text}</span>{:else}<span class="tbl-format-static">{seg.text}</span>{/if}{/each}</span>
							</td>
							<td class="right">
								<span class="tbl-count mono">{item.currentEps.toLocaleString()} evt/s · {formatBytes(item.bytesPerSec ?? 0)}/s</span>
							</td>
							<td class="right">
								<span class="tbl-count mono">{item.count.toLocaleString()} evt · {formatBytes(item.bytes ?? 0)}</span>
							</td>
							<td>
								<span class="tbl-senders">
									{#if item.sender.length > 0}
										{item.sender.slice(0, 2).join(', ')}{item.sender.length > 2 ? ` +${item.sender.length - 2}` : ''}
									{:else}
										<span class="tbl-empty">—</span>
									{/if}
								</span>
							</td>
							<td class="right">
								<div class="row-actions">
									{#if item.eps > 0 && item.sender.length > 0}
										<button
											class="icon-btn {item.paused ? 'icon-btn-primary' : ''}"
											onclick={(e) => { e.stopPropagation(); toggleLog(item.name, !!item.paused); }}
											title={item.paused ? 'Start' : 'Stop'}
											aria-label="{item.paused ? 'Start' : 'Stop'} {item.name}"
										>
											{#if item.paused}
												<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
											{:else}
												<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
											{/if}
										</button>
									{/if}
									<button
										class="icon-btn"
										onclick={(e) => { e.stopPropagation(); openCopy(item); }}
										title="Duplicate"
										aria-label="Duplicate {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
									</button>
									<button
										class="icon-btn"
										onclick={(e) => { e.stopPropagation(); openEdit(item); }}
										title="Edit"
										aria-label="Edit {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
									</button>
									<button
										class="icon-btn danger"
										onclick={(e) => { e.stopPropagation(); askDelete(item.name); }}
										title="Delete"
										aria-label="Delete {item.name}"
									>
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
									</button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
					<tfoot>
						<tr class="tbl-total-row">
							<td colspan="3"><strong>Total ({filtered.length})</strong></td>
							<td class="right mono">{filtered.reduce((s, l) => s + l.currentEps, 0).toLocaleString()} evt/s · {formatBytes(filtered.reduce((s, l) => s + (l.bytesPerSec ?? 0), 0))}/s</td>
							<td class="right mono">{filtered.reduce((s, l) => s + l.count, 0).toLocaleString()} evt · {formatBytes(filtered.reduce((s, l) => s + (l.bytes ?? 0), 0))}</td>
							<td colspan="2"></td>
						</tr>
					</tfoot>
			</table>
		</div>
	{/if}
</div>

<!-- Add/Edit Dialog -->
{#if dialogOpen}
	<div class="overlay" role="presentation">
		<div
			class="dialog wide {maximized ? 'maximized' : ''}"
			role="dialog"
			aria-modal="true"
			tabindex="-1"
			onkeydown={(e) => e.key === 'Escape' && closeDialog()}
		>
			<div class="dialog-header">
				<h2 class="dialog-title">{editMode ? 'Edit Log' : 'Add Log'}</h2>
				<div class="dialog-header-actions">
					<button class="close-btn" onclick={() => (maximized = !maximized)} aria-label={maximized ? 'Restore' : 'Maximize'}>
						{#if maximized}
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>
						{:else}
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg>
						{/if}
					</button>
					<button class="close-btn" onclick={closeDialog} aria-label="Close">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
					</button>
				</div>
			</div>
			<div class="dialog-body">
				<!-- Section 1: Basic Info -->
				<div class="pipeline-section">
					<div class="basic-info-row">
						<div class="field field-name">
							<label class="field-label" for="log-name">NAME <span class="required">*</span></label>
							<input id="log-name" class="input" type="text" bind:value={formName} disabled={editMode} placeholder="my-log" />
						</div>
						<div class="field field-eps">
							<label class="field-label" for="log-eps">RATE <span class="required">*</span></label>
							<div class="eps-input-group">
								<input id="log-eps" class="input eps-input" type="number" bind:value={formEps} min="0" placeholder={formEpsUnit === 'bytes' ? '1048576' : '1000'} />
								<div class="rate-select-wrap">
									<button type="button" class="rate-select-trigger" onclick={() => (rateDropOpen = !rateDropOpen)}>
										{rateOptions.find(o => o.value === getRateKey())?.label ?? 'evt/s'}
										<svg class="rate-chevron" class:rotated={rateDropOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="6 9 12 15 18 9"/></svg>
									</button>
									{#if rateDropOpen}
										{@const triggerRect = (() => { const el = rateDropEl?.previousElementSibling ?? rateDropEl?.parentElement?.querySelector('.rate-select-trigger'); return el?.getBoundingClientRect() ?? { bottom: 0, left: 0 }; })()}
										<!-- svelte-ignore a11y_no_static_element_interactions -->
										<div class="rate-dropdown" bind:this={rateDropEl} style="top:{triggerRect.bottom + 4}px;left:{triggerRect.left}px;" onkeydown={(e) => e.key === 'Escape' && (rateDropOpen = false)}>
											{#each ['evt', 'B', 'KB', 'MB', 'GB'] as unit}
												<div class="rate-group-label">{unit}</div>
												<div class="rate-group-row">
													{#each ['sec', 'min', 'hour', 'day'] as time}
														{@const key = `${unit}-${time}`}
														{@const label = `${unit === 'evt' ? 'evt' : unit}/${time === 'sec' ? 's' : time === 'min' ? 'm' : time === 'hour' ? 'h' : 'd'}`}
														<button
															type="button"
															class="rate-option"
															class:selected={getRateKey() === key}
															onclick={() => { applyRateKey(key); rateDropOpen = false; }}
														>{label}</button>
													{/each}
												</div>
											{/each}
										</div>
									{/if}
								</div>
							</div>
						</div>
					</div>
				</div>

				<!-- Section 2: Format Editor -->
				<div class="pipeline-section">
					<div class="section-header">
						<label id="log-format-label" class="field-label" for="log-format">FORMAT TEMPLATE <span class="required">*</span></label>
					</div>
					<!-- svelte-ignore a11y_mouse_events_have_key_events -->
					<div
						id="log-format"
						class="format-editable mono"
						contenteditable="true"
						spellcheck="false"
						bind:this={formatTextarea}
						oninput={(e) => {
							formFormat = e.currentTarget.textContent ?? '';
							runPreview();
						}}
						onpaste={(e) => {
							e.preventDefault();
							const text = e.clipboardData?.getData('text/plain') ?? '';
							insertTextAtCursor(text);
							runPreview();
						}}
						onmouseover={(e) => {
							const el = (e.target as HTMLElement).closest('[data-maker]');
							hoveredMaker = el ? (el as HTMLElement).dataset.maker ?? null : null;
						}}
						onfocusin={(e) => {
							const el = (e.target as HTMLElement).closest('[data-maker]');
							hoveredMaker = el ? (el as HTMLElement).dataset.maker ?? null : null;
						}}
						onmouseleave={() => (hoveredMaker = null)}
						onfocusout={() => (hoveredMaker = null)}
						role="textbox"
						tabindex="0"
						aria-multiline="true"
						aria-labelledby="log-format-label"
						data-placeholder="<maker1> <maker2> some static text"
					></div>
				</div>

				<!-- Section 3: Maker Palette -->
				<div class="pipeline-section">
					<div class="section-header maker-section-header">
						<button
							type="button"
							class="section-collapse-btn"
							onclick={toggleMakerPalette}
							aria-expanded={makerPaletteOpen}
							aria-controls="maker-palette"
						>
							<svg class="section-chevron" class:rotated={makerPaletteOpen} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="6 9 12 15 18 9"/></svg>
							<span class="field-label">AVAILABLE MAKERS</span>
							<span class="section-count">{filteredMakers.length}/{makers.length}</span>
						</button>
						{#if makerPaletteOpen}
							<div class="maker-search-wrap">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="maker-search-icon"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
								<input class="maker-search-input" type="search" placeholder="Filter makers…" bind:value={makerSearch} aria-label="Filter makers" />
							</div>
						{/if}
					</div>
					{#if makerPaletteOpen}
						<div id="maker-palette" class="maker-palette">
							{#if makers.length === 0}
								<span class="palette-empty">No makers available</span>
							{:else if filteredMakers.length === 0}
								<span class="palette-empty">No makers found</span>
							{:else}
								{#each filteredMakers as maker}
									<Tooltip
										title={maker.name}
										text={"TYPE: " + maker.type + (maker.sample != null ? "\nSAMPLE: " + maker.sample : "")}
										position="bottom"
									>
										<button
											class="palette-chip"
											type="button"
											onclick={() => insertMaker(maker.name)}
										>
											<span class="palette-chip-name">{maker.name}</span>
											{#if maker.type}
												<span class="palette-chip-type">{maker.type}</span>
											{/if}
										</button>
									</Tooltip>
								{/each}
							{/if}
						</div>
					{/if}
				</div>

				<!-- Section 4: Live Preview -->
				<div class="pipeline-section preview-section">
					<div class="section-header">
						<span class="field-label">
							LIVE PREVIEW
							{#if previewLoading}
								<span class="preview-spinner"></span>
							{/if}
						</span>
					</div>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="preview-box mono" onmouseleave={() => { hoveredMaker = null; hideMakerTip(); }}>{#if previewText}{#each mapSampleToFormat(formFormat, previewText) as seg}{#if seg.maker}<span class="hl-maker {hoveredMaker === seg.maker ? 'hl-active' : ''}" data-maker={seg.maker} onmouseenter={(e) => { hoveredMaker = seg.maker ?? null; showMakerTip(e, seg.maker ?? ''); }} onmouseleave={() => { hoveredMaker = null; hideMakerTip(); }}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{:else}<span class="hl-placeholder">Output will appear here…</span>{/if}</div>
				</div>

				<!-- Section 5: Senders -->
				<div class="pipeline-section last-section">
					<div class="section-header">
						<span class="field-label">SEND TO</span>
					</div>
					<div class="sender-chips">
						{#if senders.length === 0}
							<span class="palette-empty">No senders available</span>
						{:else}
							{#each senders as s}
								<label class="sender-chip" class:selected={formSenders.includes(s.name)}>
									<input
										type="checkbox"
										class="sr-only"
										checked={formSenders.includes(s.name)}
										onchange={() => toggleSender(s.name)}
									/>
									<span class="sender-chip-check" aria-hidden="true">
										{#if formSenders.includes(s.name)}
											<svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5"><polyline points="20 6 9 17 4 12"/></svg>
										{/if}
									</span>
									<span class="sender-chip-name">{s.name}</span>
									<span class="sender-chip-type">{s.type}</span>
								</label>
							{/each}
						{/if}
					</div>
				</div>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-ghost" onclick={closeDialog} disabled={loading}>Cancel</button>
				<button class="btn btn-primary" onclick={submit} disabled={loading}>
					{#if loading}<span class="spinner"></span>{/if}
					{editMode ? 'Save Changes' : 'Add Log'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Import Dialog -->
{#if importOpen}
	<div class="overlay" role="presentation">
		<div
			class="dialog narrow"
			role="dialog"
			aria-modal="true"
			tabindex="-1"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.key === 'Escape' && (importOpen = false)}
		>
			<div class="dialog-header">
				<h2 class="dialog-title">Import Logs</h2>
				<button class="close-btn" onclick={() => (importOpen = false)} aria-label="Close">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<div class="dialog-body">
				<label class="upload-zone">
					<input type="file" accept=".json" class="sr-only" onchange={handleImport} />
					<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="1.5"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
					<span class="upload-label">Drop JSON file here or click to browse</span>
				</label>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-ghost" onclick={() => (importOpen = false)}>Cancel</button>
			</div>
		</div>
	</div>
{/if}

<ConfirmDialog
	open={confirmOpen}
	title="Delete Log"
	message="Delete '{confirmName}'? This cannot be undone."
	loading={confirmLoading}
	onconfirm={confirmDelete}
	oncancel={() => (confirmOpen = false)}
/>

<!-- Floating maker tooltip (no wrapper, same design as Tooltip component) -->
{#if ftip.show}
	{@const entries = ftip.text.split('\n').filter(l => l.includes(': ')).map(l => { const i = l.indexOf(': '); return { key: l.slice(0, i), val: l.slice(i + 2) }; })}
	<div class="ftip" style="position:fixed;z-index:9999;pointer-events:none;bottom:{window.innerHeight - ftip.y}px;left:{ftip.x}px;transform:translateX(-50%);">
		<table class="ftip-table"><tbody>
			{#if ftip.title}<tr><td colspan="2" class="ftip-title">{ftip.title}</td></tr>{/if}
			{#each entries as e}<tr><td class="ftip-key">{e.key}</td><td class="ftip-val">{e.val}</td></tr>{/each}
		</tbody></table>
	</div>
{/if}

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

	/* ── Pipeline grid ── */
	.pipeline-grid {
		display: grid;
		grid-template-columns: 1fr;
		gap: 0.75rem;
	}

	/* ── Pipeline card ── */
	.pipeline-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow: hidden;
	}

	.pipeline-card:hover {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--bg-surface) 96%, var(--accent));
	}

	.pipeline-card.running {
		border-left: 2px solid var(--accent);
	}

	.pipeline-card.lagging {
		border-left: 2px solid var(--warning);
	}

	.pipeline-card:focus {
		outline: none;
		border-color: var(--accent);
		background: color-mix(in srgb, var(--bg-surface) 96%, var(--accent));
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
	}

	/* Card header */
	.pipeline-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 0.875rem;
		border-bottom: 1px solid var(--border);
		gap: 1rem;
	}

	.pipeline-name-block {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.pipeline-name {
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.pipeline-desc {
		font-size: 0.6875rem;
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.pipeline-status {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		font-size: 0.6875rem;
		font-weight: 600;
		white-space: nowrap;
		background: var(--bg-raised);
		color: var(--text-muted);
		border: 1px solid var(--border);
		flex-shrink: 0;
		letter-spacing: 0.02em;
	}

	.pipeline-status.running {
		background: var(--success-light);
		color: var(--success);
		border-color: color-mix(in srgb, var(--success) 25%, transparent);
	}

	.pipeline-status.lagging {
		background: var(--warning-light);
		color: var(--warning);
		border-color: color-mix(in srgb, var(--warning) 25%, transparent);
	}

	.status-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	.status-dot.pulse {
		animation: pulse-ring 2s ease-out infinite;
	}

	/* Makers/Senders bar */
	.pipeline-bar {
		display: flex;
		align-items: center;
		padding: 0.5rem 0.875rem;
		gap: 0.5rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg-raised);
		flex-wrap: wrap;
		overflow: hidden;
	}

	.bar-section {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		min-width: 0;
	}

	.bar-label {
		font-size: 0.5625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-muted);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.bar-chips {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		flex-wrap: wrap;
	}

	.chip {
		display: inline-block;
		padding: 0.1rem 0.4375rem;
		border-radius: 4px;
		font-size: 0.625rem;
		font-weight: 500;
		white-space: nowrap;
		font-family: var(--font-mono);
	}

	.chip-maker {
		background: var(--accent-light);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
	}

	.chip-sender {
		background: var(--bg-surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
	}

	.chip-empty {
		color: var(--text-muted);
		border: 1px dashed var(--border);
		background: transparent;
	}

	.bar-sep {
		width: 1px;
		height: 16px;
		background: var(--border);
		flex-shrink: 0;
		margin: 0 0.125rem;
	}

	/* Pipeline body */
	.pipeline-body {
		padding: 0.625rem 0.875rem;
		border-bottom: 1px solid var(--border);
		min-width: 0;
		overflow: hidden;
	}

	.body-label {
		font-size: 0.5625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-muted);
		margin-bottom: 0.25rem;
	}

	.output-line {
		font-size: 0.75rem;
		line-height: 1.6;
		color: var(--text-primary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 0.5rem 0.625rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-family: var(--font-mono);
	}

	.output-line.expanded {
		white-space: pre-wrap;
		word-break: break-all;
		overflow: visible;
		text-overflow: unset;
	}

	.output-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		margin-top: 0.25rem;
		padding: 0.125rem 0.5rem;
		background: none;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 0.625rem;
		font-family: var(--font-ui);
		cursor: pointer;
		transition: color 0.12s, border-color 0.12s;
	}

	.output-toggle:hover {
		color: var(--accent);
		border-color: var(--accent);
	}


	:global(.out-maker) {
		color: var(--accent);
		font-weight: 600;
		border-bottom: 1px dotted color-mix(in srgb, var(--accent) 50%, transparent);
		cursor: help;
		transition: background 0.12s;
	}

	:global(.out-maker:hover) {
		background: var(--accent-light);
		border-bottom-color: var(--accent);
	}

	/* Metrics row */
	.pipeline-metrics {
		display: flex;
		align-items: center;
		gap: 0.875rem;
		padding: 0.5rem 0.875rem;
		border-bottom: 1px solid var(--border);
	}

	.metric-item {
		display: flex;
		align-items: baseline;
		gap: 0.3rem;
		flex-shrink: 0;
	}

	.metric-count {
		margin-left: auto;
	}

	.metric-label-sm {
		font-size: 0.5625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-muted);
	}

	.eps-bar-wrap {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.375rem;
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
		font-family: var(--font-mono);
	}

	.metric-count-val {
		font-size: 0.875rem;
		font-weight: 700;
		letter-spacing: -0.03em;
		color: var(--text-primary);
		font-family: var(--font-mono);
	}

	/* Card footer */
	.pipeline-footer {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.625rem;
		background: var(--bg-raised);
		border-radius: 0 0 var(--radius-md) var(--radius-md);
	}

	.btn-danger-ghost:hover:not(:disabled) {
		color: var(--danger);
		background: var(--danger-light);
		border-color: color-mix(in srgb, var(--danger) 25%, transparent);
	}

	/* ── Table view ── */
	.table-row-clickable {
		cursor: pointer;
	}

	.table-row-clickable:focus {
		outline: none;
	}

	.table-row-clickable:focus td {
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}

	.tbl-name {
		font-weight: 600;
		color: var(--text-primary);
		font-size: 0.8125rem;
	}

	.tbl-status {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		padding: 0.175rem 0.4375rem;
		border-radius: 4px;
		font-size: 0.6875rem;
		font-weight: 600;
		background: var(--bg-raised);
		color: var(--text-muted);
		border: 1px solid var(--border);
		white-space: nowrap;
	}

	.tbl-status.tbl-status-running {
		background: var(--success-light);
		color: var(--success);
		border-color: color-mix(in srgb, var(--success) 25%, transparent);
	}

	.tbl-status.tbl-status-lagging {
		background: var(--warning-light);
		color: var(--warning);
		border-color: color-mix(in srgb, var(--warning) 25%, transparent);
	}

	.tbl-status-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	.tbl-status-dot.pulse {
		animation: pulse-ring 2s ease-out infinite;
	}

	.tbl-format-cell {
		max-width: 280px;
	}

	.tbl-format {
		font-size: 0.75rem;
		white-space: nowrap;
		overflow: hidden;
		display: inline-block;
		max-width: 100%;
		text-overflow: ellipsis;
		vertical-align: middle;
	}

	.tbl-format-maker {
		color: var(--accent);
		font-weight: 600;
	}

	.tbl-format-static {
		color: var(--text-secondary);
	}


	.tbl-count {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.tbl-senders {
		font-size: 0.75rem;
		color: var(--text-secondary);
		font-family: var(--font-mono);
		white-space: nowrap;
	}

	.tbl-empty {
		color: var(--text-muted);
		font-size: 0.75rem;
	}

	/* ── Pipeline Builder Dialog ── */

	.pipeline-section {
		padding: 0.875rem 0;
		border-bottom: 1px solid var(--border);
	}

	.pipeline-section.last-section {
		border-bottom: none;
		padding-bottom: 0;
	}

	.pipeline-section.preview-section {
		padding-top: 0.625rem;
	}

	.section-header {
		margin-bottom: 0.5rem;
	}

	.maker-section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}

	.section-collapse-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0;
		border: 0;
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	.section-collapse-btn:hover .field-label {
		color: var(--text-secondary);
	}

	.section-chevron {
		color: var(--text-muted);
		transition: transform 0.15s ease;
	}

	.section-chevron.rotated {
		transform: rotate(180deg);
	}

	.section-count {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--text-muted);
	}

	.maker-search-wrap {
		position: relative;
		width: min(15rem, 42%);
		min-width: 10rem;
	}

	.maker-search-icon {
		position: absolute;
		top: 50%;
		left: 0.5rem;
		transform: translateY(-50%);
		color: var(--text-muted);
		pointer-events: none;
	}

	.maker-search-input {
		width: 100%;
		height: 1.875rem;
		padding: 0.25rem 0.625rem 0.25rem 1.625rem;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 0.75rem;
		font-family: var(--font-ui);
	}

	.maker-search-input:focus {
		outline: none;
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 16%, transparent);
	}

	.basic-info-row {
		display: flex;
		gap: 0.75rem;
		align-items: flex-end;
	}

	.field-name {
		flex: 1;
		margin-bottom: 0;
	}

	.field-eps {
		width: 260px;
		flex-shrink: 0;
		margin-bottom: 0;
	}

	.eps-input-group {
		display: flex;
		gap: 0;
		align-items: stretch;
	}

	.eps-input {
		border-top-right-radius: 0 !important;
		border-bottom-right-radius: 0 !important;
		flex: 1;
		min-width: 0;
	}

	.rate-select-wrap {
		position: relative;
		flex-shrink: 0;
		display: flex;
	}

	.rate-select-trigger {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0 0.5rem;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-left: none;
		border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
		color: var(--accent);
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
		white-space: nowrap;
		transition: background 0.12s;
		font-family: var(--font-mono);
	}

	.rate-select-trigger:hover {
		background: var(--accent-light);
	}

	.rate-chevron {
		color: var(--text-muted);
		transition: transform 0.15s;
	}

	.rate-chevron.rotated {
		transform: rotate(180deg);
	}

	.rate-dropdown {
		position: fixed;
		z-index: 9999;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		padding: 0.375rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		animation: pop-in 0.1s ease-out;
	}

	.rate-group-label {
		font-size: 0.5625rem;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 0.25rem 0.25rem 0.0625rem;
	}

	.rate-group-row {
		display: flex;
		gap: 0.1875rem;
	}

	.rate-option {
		flex: 1;
		padding: 0.25rem 0.375rem;
		font-size: 0.6875rem;
		font-family: var(--font-mono);
		font-weight: 500;
		color: var(--text-secondary);
		background: var(--bg-raised);
		border: 1px solid transparent;
		border-radius: 4px;
		cursor: pointer;
		text-align: center;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
		white-space: nowrap;
	}

	.rate-option:hover {
		background: var(--accent-light);
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 30%, transparent);
	}

	.rate-option.selected {
		background: var(--accent);
		color: #fff;
		border-color: var(--accent);
		font-weight: 700;
	}

	.maker-palette {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
		padding: 0.625rem 0.75rem;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		min-height: 40px;
		max-height: 11.5rem;
		overflow: auto;
		align-items: center;
	}

	.palette-empty {
		font-size: 0.8125rem;
		color: var(--text-muted);
		font-style: italic;
	}

	:global(.palette-chip) {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		gap: 0.0625rem;
		padding: 0.25rem 0.625rem;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 5px;
		cursor: pointer;
		transition: background 0.12s, border-color 0.12s, color 0.12s;
		line-height: 1.2;
	}

	:global(.palette-chip:hover) {
		background: var(--accent-light);
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
	}

	:global(.palette-chip-name) {
		font-size: 0.75rem;
		font-family: var(--font-mono);
		font-weight: 600;
		color: var(--text-secondary);
		transition: color 0.12s;
	}

	:global(.palette-chip:hover .palette-chip-name) {
		color: var(--accent);
	}

	:global(.palette-chip-type) {
		font-size: 0.5625rem;
		font-family: var(--font-ui);
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-weight: 500;
	}

	.format-editable {
		padding: 0.625rem 0.75rem;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		font-size: 0.8125rem;
		line-height: 1.7;
		min-height: 100px;
		color: var(--text-secondary);
		white-space: pre-wrap;
		word-break: break-all;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
		cursor: text;
	}

	.format-editable:focus {
		border-color: var(--border-focus);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.format-editable:empty::before {
		content: attr(data-placeholder);
		color: var(--text-muted);
		pointer-events: none;
	}

	.format-editable :global(.hl-maker),
	:global(.hl-maker) {
		color: var(--accent);
		font-weight: 600;
		background: var(--accent-light);
		border-radius: 2px;
		transition: background 0.15s, box-shadow 0.15s;
		cursor: default;
	}

	.format-editable :global(.hl-active),
	:global(.hl-active) {
		background: rgba(74, 144, 226, 0.35);
		box-shadow: 0 0 0 1.5px var(--accent);
		border-radius: 3px;
	}


	.hl-placeholder {
		color: var(--text-muted);
	}

	.ftip-table {
		background: var(--bg-raised);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-collapse: collapse;
		border-radius: var(--radius-sm);
		min-width: 120px;
		max-width: 320px;
		box-shadow: var(--shadow-md);
		font-size: 11px;
		line-height: 1.3;
		overflow: hidden;
		font-family: var(--font-ui);
	}

	.ftip-title {
		padding: 5px 10px 4px;
		font-weight: 600;
		font-size: 11px;
		color: var(--accent);
		background: color-mix(in srgb, var(--bg-raised) 70%, var(--accent));
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
	}

	.ftip-key {
		padding: 3px 6px 3px 10px;
		color: var(--text-muted);
		white-space: nowrap;
		text-transform: uppercase;
		font-weight: 600;
		font-size: 10px;
		letter-spacing: 0.05em;
		vertical-align: baseline;
	}

	.ftip-val {
		padding: 3px 10px 3px 4px;
		color: var(--text-primary);
		font-family: var(--font-mono);
		font-size: 11px;
		word-break: break-all;
		vertical-align: baseline;
	}

	.preview-box {
		margin: 0;
		padding: 0.625rem 0.75rem;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-top: none;
		border-radius: 0 0 var(--radius-sm) var(--radius-sm);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		white-space: pre-wrap;
		word-break: break-all;
		min-height: 44px;
		line-height: 1.7;
		position: relative;
	}

	.preview-box::before {
		content: '';
		position: absolute;
		top: 0;
		left: 0.75rem;
		right: 0.75rem;
		height: 1px;
		background: color-mix(in srgb, var(--accent) 22%, transparent);
	}

	.preview-spinner {
		display: inline-block;
		width: 10px;
		height: 10px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.6s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.sender-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
	}

	.sender-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.3125rem 0.625rem 0.3125rem 0.5rem;
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		font-size: 0.8125rem;
		transition: background 0.12s, border-color 0.12s;
		user-select: none;
	}

	.sender-chip:hover {
		background: var(--bg-surface);
		border-color: var(--text-muted);
	}

	.sender-chip.selected {
		background: var(--accent-light);
		border-color: color-mix(in srgb, var(--accent) 35%, transparent);
	}

	.sender-chip-check {
		width: 14px;
		height: 14px;
		border: 1px solid var(--border);
		border-radius: 3px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		color: var(--accent);
		background: var(--bg-base);
		transition: background 0.12s, border-color 0.12s;
	}

	.sender-chip.selected .sender-chip-check {
		background: var(--accent);
		border-color: var(--accent);
		color: #000;
	}

	.sender-chip-name {
		font-weight: 500;
		color: var(--text-primary);
	}

	.sender-chip.selected .sender-chip-name {
		color: var(--accent);
	}

	.sender-chip-type {
		font-size: 0.6875rem;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	@media (max-width: 700px) {
		.pipeline-grid { grid-template-columns: 1fr; }
		.pipeline-bar { flex-wrap: wrap; gap: 0.375rem; }
		.bar-sep { display: none; }
		.search-input { width: 150px; }
		.search-input:focus { width: 150px; }
		.tbl-format-cell { max-width: 140px; }
		.maker-section-header {
			align-items: stretch;
			flex-direction: column;
		}
		.maker-search-wrap {
			width: 100%;
			min-width: 0;
		}
	}
</style>
