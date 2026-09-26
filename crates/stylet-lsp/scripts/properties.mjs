// Regenerates src/properties.tsv from @webref/css:
//   npm pack @webref/css && tar xzf webref-css-*.tgz
//   node crates/stylet-lsp/scripts/properties.mjs package > crates/stylet-lsp/src/properties.tsv
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const dir = process.argv[2]
const css = JSON.parse(readFileSync(join(dir, 'css.json'), 'utf8'))
const { version } = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'))

const types = new Map()
for (const t of css.types) if (t.syntax && !types.has(t.name)) types.set(t.name, t.syntax)
const properties = new Map()
for (const p of css.properties) if (!properties.has(p.name)) properties.set(p.name, p)

// Keywords of a value syntax, following <type> and <'property'> references.
function keywords(syntax, out, seen) {
  for (const [, name] of syntax.matchAll(/<'([a-z-]+)'>/g)) {
    const p = properties.get(name)
    if (p?.syntax && !seen.has(`p:${name}`)) {
      seen.add(`p:${name}`)
      keywords(p.syntax, out, seen)
    }
  }
  const plain = syntax.replace(/'[^']*'/g, ' ')
  for (const m of plain.matchAll(/<([a-z-]+)(?:\(\))?(?:\s*\[[^\]]*\])?>|([a-zA-Z][a-zA-Z0-9-]*)(\()?/g)) {
    if (m[1]) {
      if (!seen.has(m[1]) && types.has(m[1])) {
        seen.add(m[1])
        keywords(types.get(m[1]), out, seen)
      }
    } else if (!m[3]) {
      out.add(m[2])
    }
  }
}

const rows = [...properties.values()]
  .sort((a, b) => a.name.localeCompare(b.name))
  .map(p => {
    const found = new Set()
    keywords(p.syntax ?? '', found, new Set())
    const syntax = (p.syntax ?? '').replace(/\s+/g, ' ')
    return [p.name, syntax, [...found].slice(0, 200).join(' '), p.href ?? ''].join('\t')
  })
console.log(`# CSS properties from @webref/css ${version} (generated): name, syntax, keywords, spec link`)
console.log(rows.join('\n'))
