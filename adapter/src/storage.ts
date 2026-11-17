import type { AsyncStorage } from "@solid-primitives/storage"

/** The typed Go storage boundary, kept separate so it can be tested. */
export type StorageDriver = {
  items(name: string): Promise<{ items: Record<string, string>; revision: number }>
  update(name: string, insert: Record<string, string>, remove: string[]): Promise<number>
  clear(name: string): Promise<void>
}

/**
 * A page-side view of one persisted storage namespace.
 *
 * Writes are serialized. The cache only advances after the Go write
 * succeeds, so a failed write rejects to its caller and leaves reads at the
 * last value the disk confirms. A rejected write does not poison later
 * writes: the queue recovers and the next operation runs normally.
 */
export function createGoStorage(name: string, driver: StorageDriver): AsyncStorage {
  const cache = new Map<string, string>()
  let ready: Promise<void> | undefined
  let queue: Promise<void> = Promise.resolve()

  const load = () => {
    ready ??= driver.items(name).then(
      (snapshot) => {
        for (const [key, value] of Object.entries(snapshot.items)) cache.set(key, value)
      },
      (error) => {
        // A failed initial read may be retried by the next operation.
        ready = undefined
        throw error
      },
    )
    return ready
  }

  const enqueue = <T>(operation: () => Promise<T>): Promise<T> => {
    const run = queue.then(operation)
    queue = run.then(
      () => undefined,
      () => undefined,
    )
    return run
  }

  return {
    getItem: async (key) => {
      await load()
      return cache.get(key) ?? null
    },
    setItem: async (key, value) => {
      await load()
      await enqueue(async () => {
        await driver.update(name, { [key]: value }, [])
        cache.set(key, value)
      })
    },
    removeItem: async (key) => {
      await load()
      await enqueue(async () => {
        await driver.update(name, {}, [key])
        cache.delete(key)
      })
    },
    clear: async () => {
      await load()
      await enqueue(async () => {
        await driver.clear(name)
        cache.clear()
      })
    },
    key: async (index: number) => {
      await load()
      return [...cache.keys()][index] ?? null
    },
    getLength: async () => {
      await load()
      return cache.size
    },
    get length() {
      return cache.size
    },
  }
}
