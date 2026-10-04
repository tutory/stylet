'use strict'

const path = require('node:path')

const PACKAGE = `@tutory_de/stylet-${process.platform}-${process.arch}`
const EXE = process.platform === 'win32' ? 'stylet.exe' : 'stylet'

/** Path of the stylet binary for this platform (`STYLET_BINARY` overrides it). */
function binaryPath() {
  if (process.env.STYLET_BINARY) return process.env.STYLET_BINARY
  try {
    return path.join(path.dirname(require.resolve(`${PACKAGE}/package.json`)), 'bin', EXE)
  } catch {
    throw new Error(
      `stylet: no binary for ${process.platform}-${process.arch} (${PACKAGE} isn't installed). ` +
        'Reinstall without --no-optional, or set STYLET_BINARY to a stylet binary.'
    )
  }
}

module.exports = { binaryPath }
