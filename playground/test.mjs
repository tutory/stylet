// Smoke test for the WebAssembly module: node playground/test.mjs (after build.sh).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { initSync, compile, format } from './pkg/stylet_wasm.js'

initSync({ module: readFileSync(new URL('./pkg/stylet_wasm_bg.wasm', import.meta.url)) })

const out = compile('$b { x: 1 }\n.a {\n  @extend $b\n  &:hover { y: 2 }\n}\n', false, false)
assert.equal(out.css, '.a {\n  x: 1;\n}\n\n.a {\n  &:hover {\n    y: 2;\n  }\n}\n')
assert.deepEqual(out.diagnostics, [])

const broken = compile('.a {\n  @import "x"\n}', false, false)
assert.equal(broken.diagnostics[0].message, "`@import` isn't available in the playground")
assert.equal(broken.diagnostics[0].line, 2)

assert.equal(format('.a{color:red;}', 2, false, false).code, '.a {\n  color: red\n}\n')
assert.equal(format('.a {\n  .b { y: 2 }\n  x: 1\n}', 2, false, true).code, '.a {\n  x: 1\n\n  .b {\n    y: 2\n  }\n}\n')
assert.equal(format('.a {\n  color\n}', 2, false, false).code, undefined)
console.log('ok')
