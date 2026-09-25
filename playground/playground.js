import init, { compile, format, version } from './pkg/stylet_wasm.js'

const EXAMPLE = `// Placeholders are extended, nesting stays native CSS nesting.
@custom-media --phone (width <= 600px)

$button {
  border-radius: 4px
  padding: .5rem 1rem
}

.toolbar {
  display: flex
  gap: .5rem

  .save {
    @extend $button
    background: var(--brand, royalblue)
    color: white

    &:hover {
      filter: brightness(1.1)
    }
  }

  @media (--phone) {
    flex-direction: column
  }
}
`

const $ = (id) => document.getElementById(id)
const source = $('source')
const output = $('output')
const diagnostics = $('diagnostics')
const options = { minify: $('minify'), customMedia: $('customMedia'), sort: $('sort') }

function encode(text) {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const b of bytes) binary += String.fromCharCode(b)
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function decode(hash) {
  const binary = atob(hash.replace(/-/g, '+').replace(/_/g, '/'))
  return new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0)))
}

function initialSource() {
  if (location.hash.length > 1) {
    try {
      return decode(location.hash.slice(1))
    } catch {}
  }
  try {
    return localStorage.getItem('stylet-source') ?? EXAMPLE
  } catch {
    return EXAMPLE
  }
}

function showDiagnostics(list) {
  diagnostics.replaceChildren(
    ...list.map((d) => {
      const li = document.createElement('li')
      li.className = d.severity
      li.textContent = `${d.line}:${d.column} ${d.severity}: ${d.message}`
      li.addEventListener('click', () => {
        const encoder = new TextEncoder()
        // Diagnostics use byte offsets; the textarea uses UTF-16 indices.
        const text = source.value
        let bytes = 0
        let start = text.length
        let end = text.length
        for (let i = 0; i <= text.length; i++) {
          if (bytes === d.start && start === text.length) start = i
          if (bytes >= d.end) {
            end = i
            break
          }
          bytes += encoder.encode(text[i] ?? '').length
        }
        source.focus()
        source.setSelectionRange(start, Math.max(start, end))
      })
      return li
    }),
  )
}

function update() {
  const text = source.value
  const result = compile(text, options.minify.checked, options.customMedia.checked)
  output.textContent = result.css
  showDiagnostics(result.diagnostics)
  try {
    localStorage.setItem('stylet-source', text)
  } catch {}
}

await init()
$('version').textContent = version()
source.value = initialSource()
update()

source.addEventListener('input', update)
for (const input of Object.values(options)) input.addEventListener('change', update)

source.addEventListener('keydown', (event) => {
  if (event.key !== 'Tab' || event.shiftKey) return
  event.preventDefault()
  source.setRangeText('  ', source.selectionStart, source.selectionEnd, 'end')
  update()
})

$('format').addEventListener('click', () => {
  const result = format(source.value, 2, options.sort.checked)
  if (result.code != null) {
    source.value = result.code
    update()
  } else {
    showDiagnostics(result.diagnostics)
  }
})

$('share').addEventListener('click', async () => {
  history.replaceState(null, '', `#${encode(source.value)}`)
  try {
    await navigator.clipboard.writeText(location.href)
    $('share').textContent = 'Link copied'
  } catch {
    $('share').textContent = 'Link in address bar'
  }
  setTimeout(() => ($('share').textContent = 'Share'), 1500)
})
