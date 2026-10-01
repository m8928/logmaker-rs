import { addToast } from '$lib/stores/toast.svelte';
import type { ApiResult, DashboardData, Log, Maker, Plugin, PluginType, Scenario, Sender } from '$lib/types';

const BASE = '/api/v1';

function statusMessage(res: Response): string {
	return `Request failed with status ${res.status}`;
}

function isApiResult(value: unknown): value is ApiResult {
	return (
		typeof value === 'object' &&
		value !== null &&
		'type' in value &&
		typeof (value as ApiResult).type === 'string'
	);
}

export function unwrapApiResult(value: ApiResult | { body?: ApiResult }): ApiResult {
	return 'body' in value && value.body ? value.body : (value as ApiResult);
}

export async function readJsonResponse<T>(res: Response, fallbackMessage: string): Promise<T> {
	const contentType = res.headers.get('content-type') ?? '';
	if (!contentType.toLowerCase().includes('application/json')) {
		throw new Error(fallbackMessage);
	}

	try {
		return (await res.json()) as T;
	} catch {
		throw new Error('Invalid JSON response');
	}
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
	try {
		const res = await fetch(`${BASE}${path}`, {
			headers: { 'Content-Type': 'application/json', ...options?.headers },
			...options
		});
		let data: ApiResult;
		try {
			data = await readJsonResponse<ApiResult>(res, statusMessage(res));
		} catch (err) {
			addToast('error', err instanceof Error ? err.message : statusMessage(res));
			throw err;
		}
		if (!res.ok || data.type === 'ERROR') {
			const message = data.message || statusMessage(res);
			addToast('error', message);
			throw new Error(message);
		}
		if (data.notification !== false && data.message) {
			addToast('success', data.message);
		}
		return data as T;
	} catch (err) {
		if (err instanceof TypeError) {
			addToast('error', 'Network error — server may be offline');
		}
		throw err;
	}
}

async function fetchJson<T>(path: string): Promise<T> {
	try {
		const res = await fetch(`${BASE}${path}`);
		let data: T | ApiResult;
		try {
			data = await readJsonResponse<T | ApiResult>(res, `Failed to load data (${res.status})`);
		} catch (err) {
			addToast('error', err instanceof Error ? err.message : `Failed to load data (${res.status})`);
			throw err;
		}
		if (!res.ok) {
			const message = isApiResult(data) && data.message ? data.message : `Failed to load data (${res.status})`;
			addToast('error', message);
			throw new Error(message);
		}
		return data as T;
	} catch (err) {
		if (err instanceof TypeError) {
			addToast('error', 'Network error — server may be offline');
		}
		throw err;
	}
}

export const api = {
	// Dashboard
	getDashboard: () => fetchJson<DashboardData>('/dashboard'),

	// Makers
	getMakers: () => fetchJson<Maker[]>('/maker'),
	getMakerTypes: () => fetchJson<PluginType[]>('/plugin/maker'),
	createMaker: (data: Partial<Maker>) =>
		request<ApiResult>('/maker', { method: 'POST', body: JSON.stringify(data) }),
	updateMaker: (name: string, data: Partial<Maker>) =>
		request<ApiResult>(`/maker/${name}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteMaker: (name: string) => request<ApiResult>(`/maker/${name}`, { method: 'DELETE' }),

	// Senders
	getSenders: () => fetchJson<Sender[]>('/sender'),
	getSenderTypes: () => fetchJson<PluginType[]>('/plugin/sender'),
	createSender: (data: Partial<Sender>) =>
		request<ApiResult>('/sender', { method: 'POST', body: JSON.stringify(data) }),
	updateSender: (name: string, data: Partial<Sender>) =>
		request<ApiResult>(`/sender/${name}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteSender: (name: string) => request<ApiResult>(`/sender/${name}`, { method: 'DELETE' }),

	// Logs
	getLogs: () => fetchJson<Log[]>('/log'),
	createLog: (data: Partial<Log>) =>
		request<ApiResult>('/log', { method: 'POST', body: JSON.stringify(data) }),
	updateLog: (name: string, data: Partial<Log>) =>
		request<ApiResult>(`/log/${name}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteLog: (name: string) => request<ApiResult>(`/log/${name}`, { method: 'DELETE' }),
	startLog: (name: string) => request<ApiResult>(`/log/${name}:start`, { method: 'POST' }),
	stopLog: (name: string) => request<ApiResult>(`/log/${name}:stop`, { method: 'POST' }),
	previewLog: (data: Partial<Log>) =>
		request<ApiResult>('/log:preview', { method: 'POST', body: JSON.stringify(data) }),

	// Plugins
	getPlugins: () => fetchJson<Plugin[]>('/plugin'),
	deletePlugin: (name: string) => request<ApiResult>(`/plugin/${name}`, { method: 'DELETE' }),

	// Scenarios
	getScenarios: () => fetchJson<Scenario[]>('/scenario'),
	createScenario: (data: Partial<Scenario>) =>
		request<ApiResult>('/scenario', { method: 'POST', body: JSON.stringify(data) }),
	updateScenario: (name: string, data: Partial<Scenario>) =>
		request<ApiResult>(`/scenario/${name}`, { method: 'PUT', body: JSON.stringify(data) }),
	deleteScenario: (name: string) => request<ApiResult>(`/scenario/${name}`, { method: 'DELETE' }),
	startScenario: (name: string) =>
		request<ApiResult>(`/scenario/${name}:start`, { method: 'POST' }),
	stopScenario: (name: string) =>
		request<ApiResult>(`/scenario/${name}:stop`, { method: 'POST' })
};
