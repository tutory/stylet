#!/usr/bin/env node
'use strict'

const { spawnSync } = require('node:child_process')
const { binaryPath } = require('..')

let binary
try {
  binary = binaryPath()
} catch (error) {
  console.error(error.message)
  process.exit(1)
}
const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' })
if (result.error) {
  console.error(`stylet: ${result.error.message}`)
  process.exit(1)
}
if (result.signal) process.kill(process.pid, result.signal)
process.exit(result.status ?? 1)
