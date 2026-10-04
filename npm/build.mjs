// Assembles the npm packages for a release into npm/dist:
//   node npm/build.mjs <version> <binaries dir>
// The binaries dir holds `<platform>/stylet[.exe]` (see platforms.json).
import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const [version, binaries] = process.argv.slice(2)
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version ?? '') || !binaries) {
  console.error('usage: node npm/build.mjs <version> <binaries dir>')
  process.exit(1)
}
const platforms = JSON.parse(readFileSync(join(here, 'platforms.json'), 'utf8'))
const main = JSON.parse(readFileSync(join(here, 'stylet/package.json'), 'utf8'))
const dist = join(here, 'dist')
rmSync(dist, { recursive: true, force: true })

for (const p of platforms) {
  const exe = p.os === 'win32' ? 'stylet.exe' : 'stylet'
  const source = join(binaries, p.name, exe)
  if (!existsSync(source)) {
    console.error(`missing binary: ${source}`)
    process.exit(1)
  }
  const dir = join(dist, `stylet-${p.name}`)
  mkdirSync(join(dir, 'bin'), { recursive: true })
  cpSync(source, join(dir, 'bin', exe))
  chmodSync(join(dir, 'bin', exe), 0o755)
  const pkg = {
    name: `${main.name}-${p.name}`,
    version,
    description: `The stylet binary for ${p.os} ${p.cpu}`,
    repository: main.repository,
    license: main.license,
    os: [p.os],
    cpu: [p.cpu],
    files: ['bin'],
  }
  writeFileSync(join(dir, 'package.json'), `${JSON.stringify(pkg, null, 2)}\n`)
  writeFileSync(join(dir, 'README.md'), `The \`${p.os}-${p.cpu}\` binary of [${main.name}](https://www.npmjs.com/package/${main.name}).\n`)
  main.optionalDependencies[pkg.name] = version
}

const dir = join(dist, 'stylet')
cpSync(join(here, 'stylet'), dir, { recursive: true })
cpSync(join(here, '..', 'README.md'), join(dir, 'README.md'))
cpSync(join(here, '..', 'LICENSE-MIT'), join(dir, 'LICENSE-MIT'))
cpSync(join(here, '..', 'LICENSE-APACHE'), join(dir, 'LICENSE-APACHE'))
main.version = version
writeFileSync(join(dir, 'package.json'), `${JSON.stringify(main, null, 2)}\n`)
console.log(`npm/dist: ${main.name} ${version} and ${platforms.length} platform packages`)
