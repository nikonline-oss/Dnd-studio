import { v7, validate, version } from 'uuid';

/**
 * The only place that generates ids (ESLint forbids `crypto.randomUUID` and `uuid` elsewhere).
 * Entity and intent ids are client-generated UUIDv7 (ADR-0007, ADR-0015); the server checks only
 * the version and format and never interprets the embedded timestamp.
 */
export function uuidv7(): string {
  return v7();
}

/** Whether `id` is a well-formed UUID version 7. */
export function isUuidV7(id: string): boolean {
  return validate(id) && version(id) === 7;
}
