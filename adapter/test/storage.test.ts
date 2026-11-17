import { describe, expect, test } from "bun:test"
import { createGoStorage, type StorageDriver } from "../src/storage"

type Update = { insert: Record<string, string>; remove: string[] }

function driver(initial: Record<string, string> = {}, options: { failUpdate?: boolean; failClear?: boolean } = {}) {
  const updates: Update[] = []
  const state = { ...initial }
  let failUpdate = options.failUpdate ?? false
  let failNext = 0
  let failClear = options.failClear ?? false
  const api: StorageDriver = {
    async items() {
      return { items: { ...state }, revision: 0 }
    },
    async update(_name, insert, remove) {
      if (failNext > 0) {
        failNext--
        throw new Error("disk full")
      }
      if (failUpdate) throw new Error("disk full")
      updates.push({ insert, remove })
      for (const key of remove) delete state[key]
      Object.assign(state, insert)
      return updates.length
    },
    async clear() {
      if (failClear) throw new Error("disk unavailable")
      for (const key of Object.keys(state)) delete state[key]
    },
  }
  return {
    api,
    updates,
    state,
    allowWrites() {
      failUpdate = false
    },
    allowClear() {
      failClear = false
    },
    failWrites() {
      failUpdate = true
    },
    failNextWrites(count: number) {
      failNext = count
    },
  }
}

describe("createGoStorage", () => {
  test("a failed set rejects and leaves the cached persisted value", async () => {
    const fake = driver({ key: "old" }, { failUpdate: true })
    const storage = createGoStorage("test", fake.api)

    await expect(storage.setItem("key", "new")).rejects.toThrow("disk full")
    expect(await storage.getItem("key")).toBe("old")

    fake.allowWrites()
    await storage.setItem("key", "new")
    expect(await storage.getItem("key")).toBe("new")
    expect(fake.state.key).toBe("new")
  })

  test("a failed remove rejects and keeps the value", async () => {
    const fake = driver({ key: "old" }, { failUpdate: true })
    const storage = createGoStorage("test", fake.api)
    await expect(storage.removeItem("key")).rejects.toThrow("disk full")
    expect(await storage.getItem("key")).toBe("old")

    fake.allowWrites()
    await storage.removeItem("key")
    expect(await storage.getItem("key")).toBeNull()
    expect(fake.state.key).toBeUndefined()
  })

  test("a failed clear rejects and keeps all values", async () => {
    const fake = driver({ a: "1", b: "2" }, { failClear: true })
    const storage = createGoStorage("test", fake.api)
    await expect(storage.clear()).rejects.toThrow("disk unavailable")
    expect(await storage.getItem("a")).toBe("1")
    expect(await storage.getItem("b")).toBe("2")

    fake.allowClear()
    await storage.clear()
    expect(await storage.getLength()).toBe(0)
    expect(fake.state).toEqual({})
  })

  test("writes run in call order and one failure does not poison the queue", async () => {
    const fake = driver()
    const storage = createGoStorage("test", fake.api)
    fake.failNextWrites(1)

    const failed = storage.setItem("first", "bad")
    const second = storage.setItem("second", "ok")
    await expect(failed).rejects.toThrow("disk full")
    await second

    expect(fake.updates.map((entry) => Object.keys(entry.insert)[0])).toEqual(["second"])
    expect(await storage.getItem("first")).toBeNull()
    expect(await storage.getItem("second")).toBe("ok")
  })

  test("a failed initial load can be retried", async () => {
    let calls = 0
    const state = { key: "value" }
    const api: StorageDriver = {
      async items() {
        calls++
        if (calls === 1) throw new Error("temporary")
        return { items: { ...state }, revision: 0 }
      },
      async update() {
        return 1
      },
      async clear() {},
    }
    const storage = createGoStorage("test", api)
    await expect(storage.getItem("key")).rejects.toThrow("temporary")
    expect(await storage.getItem("key")).toBe("value")
  })
})
