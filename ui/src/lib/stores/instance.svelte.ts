import { api, selectInstance, selectedInstance } from '$lib/api';

let names = $state<string[]>([]);
/** The server that serves this page. */
let current = $state('');
/** The instance this tab manages. */
let active = $state('');

export const instances = {
	get names() {
		return names;
	},
	get current() {
		return current;
	},
	get active() {
		return active;
	},
	/** Whether the instance switcher is shown. */
	get multiple() {
		return names.length > 1;
	}
};

export async function loadInstances() {
	try {
		const d = await api.getInstances();
		const selected = selectedInstance();
		if (selected && !d.instances.includes(selected)) {
			// The selected instance is no longer configured.
			selectInstance(d.current, d.current);
			return;
		}
		names = d.instances;
		current = d.current;
		active = selected || d.current;
	} catch {
		/* ignore */
	}
}

/** Browser tab title; starts with the instance when there are several. */
export function pageTitle(page: string): string {
	return names.length > 1 ? `${active} · ${page} — LogMaker` : `${page} — LogMaker`;
}
