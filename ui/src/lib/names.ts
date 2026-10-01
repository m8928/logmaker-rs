/** Name rules for new makers, senders, logs and scenarios; mirrors core/src/names.rs. */
export const NAME_MAX = 64;

export function nameError(name: string, kind: 'maker' | 'other' = 'other'): string | null {
	if (!name) return 'Name is required';
	if (name.length > NAME_MAX || !/^[A-Za-z0-9_-]+$/.test(name))
		return `Only letters, numbers, '_' and '-' allowed (max ${NAME_MAX})`;
	if (kind === 'maker' && !/^[A-Za-z_]/.test(name))
		return "Maker names must start with a letter or '_' (used as <name> in log formats)";
	return null;
}
