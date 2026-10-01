export interface Maker {
	name: string;
	type: string;
	ref: number;
	sample?: string;
	size?: number;
	args: Record<string, string | number | boolean | string[]>;
}

export interface Sender {
	name: string;
	type: string;
	ref: number;
	output?: number;
	count?: number;
	bytes?: number;
	bytesPerSec?: number;
	limit?: number;
	args: Record<string, string | number | boolean | string[]>;
}

export interface Log {
	name: string;
	format: string;
	sample?: string;
	eps: number;
	epsUnit?: 'events' | 'bytes';
	epsTimeUnit?: 'sec' | 'min' | 'hour' | 'day';
	sender: string[];
	paused?: boolean;
	description?: string;
	bytes?: number;
	bytesPerSec?: number;
	status?: boolean;
	currentEps: number;
	count: number;
}

export interface Plugin {
	name: string;
	version: string;
	provider?: string;
	filename?: string;
	pluginId?: string;
	pluginDescription?: string;
	pluginClass?: string;
	pluginState?: string;
	ref?: number;
}

export interface PluginType {
	type: string;
	name?: string;
	args: Record<string, PluginArg>;
}

export interface PluginArg {
	name?: string;
	type: 'string' | 'integer' | 'number' | 'boolean' | 'list';
	description: string;
	required: boolean;
}

export interface DashboardData {
	log: number;
	maker: number;
	sender: number;
	plugin: number;
	eps: number;
	actualEps: number;
	bps: number;
	actualBps: number;
	cpu: number;
	memory: number;
	thread?: number;
	version?: string;
	buildTime?: string;
}

export interface ScenarioStep {
	logName: string;
	repeat: number;
	delayMinMs: number;
	delayMaxMs: number;
	senders: string[];
	overrides: Record<string, string>;
}

export interface Scenario {
	name: string;
	description?: string;
	status?: boolean;
	sharedVariables: Record<string, string>;
	steps: ScenarioStep[];
	intervalMinMs: number;
	intervalMaxMs: number;
	loopCount: number;
	count?: number;
	currentStep?: number;
	currentLoop?: number;
	totalSteps?: number;
	stepCounts?: number[];
}

export interface ApiResult<T = unknown> {
	type: 'SUCCESS' | 'ERROR' | 'VOID';
	message: string;
	notification?: boolean;
	data?: T;
}

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
	id: number;
	type: ToastType;
	message: string;
}
