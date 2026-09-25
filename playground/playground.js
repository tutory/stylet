import init, { compile, format, version } from './pkg/stylet_wasm.js'
import stylet from './stylet-hljs.js'

const HLJS = 'https://cdn.jsdelivr.net/npm/@highlightjs/cdn-assets@11.12.0/es'

/** highlight.js from the CDN; without it the playground works unhighlighted. */
async function loadHighlighter() {
  try {
    const [{ default: hljs }, { default: css }] = await Promise.all([
      import(`${HLJS}/core.min.js`),
      import(`${HLJS}/languages/css.min.js`),
    ])
    hljs.registerLanguage('css', css)
    hljs.registerLanguage('stylet', stylet)
    return (code, language) => hljs.highlight(code, { language, ignoreIllegals: true }).value
  } catch {
    return null
  }
}

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
const output = $('output').firstElementChild
const highlight = $('highlight')
let highlighter = null

function render(element, code, language) {
  if (highlighter) element.innerHTML = highlighter(code, language)
  else element.textContent = code
}

function renderSource() {
  // A trailing newline needs a character after it to take up space in the <pre>.
  render(highlight.firstElementChild, source.value + ' ', 'stylet')
  syncScroll()
}

function syncScroll() {
  highlight.scrollTop = source.scrollTop
  highlight.scrollLeft = source.scrollLeft
}
const diagnostics = $('diagnostics')
const options = { minify: $('minify'), customMedia: $('customMedia'), sort: $('sort'), nested: $('nested') }

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
  render(output, result.css, 'css')
  renderSource()
  showDiagnostics(result.diagnostics)
  try {
    localStorage.setItem('stylet-source', text)
  } catch {}
}

await init()
$('version').textContent = version()
source.value = initialSource()
update()
loadHighlighter().then((h) => {
  highlighter = h
  document.body.classList.toggle('highlighted', h != null)
  update()
})
source.addEventListener('scroll', syncScroll)

source.addEventListener('input', update)
for (const input of Object.values(options)) input.addEventListener('change', update)

source.addEventListener('keydown', (event) => {
  if (event.key !== 'Tab' || event.shiftKey) return
  event.preventDefault()
  source.setRangeText('  ', source.selectionStart, source.selectionEnd, 'end')
  update()
})

function flash(button, text) {
  const label = button.dataset.label ?? button.textContent
  button.dataset.label = label
  button.textContent = text
  clearTimeout(button.timer)
  button.timer = setTimeout(() => (button.textContent = label), 1500)
}

$('format').addEventListener('click', () => {
  const result = format(source.value, {
    sortProperties: options.sort.checked,
    nestedBlocksLast: options.nested.checked,
  })
  if (result.code == null) {
    showDiagnostics(result.diagnostics)
    flash($('format'), 'Fix errors first')
  } else if (result.code === source.value) {
    flash($('format'), 'Already formatted')
  } else {
    source.value = result.code
    update()
    flash($('format'), 'Formatted')
  }
})

$('share').addEventListener('click', async () => {
  history.replaceState(null, '', `#${encode(source.value)}`)
  try {
    await navigator.clipboard.writeText(location.href)
    flash($('share'), 'Link copied')
  } catch {
    flash($('share'), 'Link in address bar')
  }
})
