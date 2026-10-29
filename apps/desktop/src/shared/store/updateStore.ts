import { useSyncExternalStore } from "react";
import {
  checkForUpdate,
  hasNativeAppLifecycle,
  installUpdate,
  type AppUpdate,
} from "../native/appLifecycle";

export type UpdateSnapshot = {
  availableVersion: string | null;
  checking: boolean;
  installing: boolean;
};

let snapshot: UpdateSnapshot = {
  availableVersion: null,
  checking: false,
  installing: false,
};
let availableUpdate: AppUpdate | null = null;
let pendingCheck: Promise<string | null> | null = null;
const listeners = new Set<() => void>();

function update(patch: Partial<UpdateSnapshot>) {
  snapshot = { ...snapshot, ...patch };
  listeners.forEach((listener) => listener());
}

async function replaceAvailableUpdate(next: AppUpdate | null) {
  const previous = availableUpdate;
  availableUpdate = next;
  update({ availableVersion: next?.version ?? null });
  if (previous && previous !== next) await previous.close();
}

export const updateStore = {
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
  getSnapshot: () => snapshot,

  async check(): Promise<string | null> {
    if (!hasNativeAppLifecycle()) return null;
    if (pendingCheck) return pendingCheck;
    update({ checking: true });
    pendingCheck = (async () => {
      const next = await checkForUpdate();
      await replaceAvailableUpdate(next);
      return next?.version ?? null;
    })();
    try {
      return await pendingCheck;
    } finally {
      pendingCheck = null;
      update({ checking: false });
    }
  },

  async install(): Promise<void> {
    const current = availableUpdate;
    if (!current) return;
    update({ installing: true });
    try {
      await installUpdate(current);
    } finally {
      update({ installing: false });
    }
  },
};

export function useUpdateStore(): UpdateSnapshot {
  return useSyncExternalStore(updateStore.subscribe, updateStore.getSnapshot);
}
