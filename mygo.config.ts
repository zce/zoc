import { readFileSync } from "node:fs"
import { defineConfig } from "mygo-cli"

const version = process.env.ZOC_VERSION ?? readFileSync("VERSION", "utf8").trim()
const publicKey = process.env.ZOC_UPDATER_PUBLIC_KEY?.trim()

export default defineConfig({
  name: "ZOC",
  slug: "zoc",
  identifier: "dev.zce.zoc",
  version,
  copyright: "© 2026 ZOC contributors",
  license: "MIT",

  buildCommand: "scripts/build-frontend.sh",
  frontendDist: "frontend",
  bindings: "adapter/src/mygo.ts",
  out: "build",

  urlSchemes: ["zoc"],
  linux: {
    maintainer: "ZOC contributors <zoc@zce.dev>",
    comment: "An OpenCode desktop client built with MyGo",
    categories: ["Development", "Utility"],
    formats: ["rpm"],
    command: ["zoc"],
  },
  ...(publicKey
    ? {
        updates: {
          publicKey,
          github: "zce/zoc",
          tagPrefix: "zoc-v",
        },
      }
    : {}),
})
