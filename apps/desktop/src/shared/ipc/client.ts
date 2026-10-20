import { commands, type IntentOutcome, type IpcError } from './bindings';

export { commands, events } from './bindings';
export type * from './bindings';

/** A command failed on the Rust side; carries the typed {@link IpcError}. */
export class IpcFailure extends Error {
  readonly error: IpcError;

  constructor(error: IpcError) {
    super(`${error.code}: ${error.message}`);
    this.name = 'IpcFailure';
    this.error = error;
  }
}

type CommandResult<T> = { status: 'ok'; data: T } | { status: 'error'; error: IpcError };

/** Unwraps a tauri-specta result, throwing {@link IpcFailure} on error (for TanStack Query). */
export async function unwrap<T>(result: Promise<CommandResult<T>>): Promise<T> {
  const r = await result;
  if (r.status === 'error') {
    throw new IpcFailure(r.error);
  }
  return r.data;
}

/** Stable error code of anything thrown by IPC helpers, for localized messages. */
export function errorCode(error: unknown): string {
  if (error instanceof IpcFailure) return error.error.code;
  if (error instanceof IntentRejected) return error.reason.code;
  return 'unknown';
}

type Rejected = Extract<IntentOutcome, { status: 'rejected' }>;

/** The game core rejected an intent (a normal outcome, e.g. a stale version). */
export class IntentRejected extends Error {
  readonly reason: Rejected['reason'];

  constructor(reason: Rejected['reason']) {
    super(`intent rejected: ${reason.code}`);
    this.name = 'IntentRejected';
    this.reason = reason;
  }
}

export const ipc = {
  appInfo: () => commands.appInfo(),
  campaignCreate: (name: string) => unwrap(commands.campaignCreate(name)),
  campaignOpen: () => unwrap(commands.campaignOpen()),
  campaignRestoreBackup: () => unwrap(commands.campaignRestoreBackup()),
  campaignClose: () => unwrap(commands.campaignClose()),
  campaignSave: () => unwrap(commands.campaignSave()),
  campaignCurrent: () => unwrap(commands.campaignCurrent()),
  getSnapshot: () => unwrap(commands.getSnapshot()),
};
