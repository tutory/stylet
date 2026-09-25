// Highlights the code samples and makes the example editable; the page reads
// fine without it.
import stylet from './playground/stylet-hljs.js'

const HLJS = 'https://cdn.jsdelivr.net/npm/@highlightjs/cdn-assets@11.12.0/es'
const $ = (id) => document.getElementById(id)

async function loadHighlighter() {
  try {
    const [{ default: hljs }, { default: css }, { default: bash }] = await Promise.all([
      import(`${HLJS}/core.min.js`),
      import(`${HLJS}/languages/css.min.js`),
      import(`${HLJS}/languages/bash.min.js`),
    ])
    hljs.registerLanguage('css', css)
    hljs.registerLanguage('bash', bash)
    hljs.registerLanguage('stylet', stylet)
    return (code, language) => hljs.highlight(code, { language, ignoreIllegals: true }).value
  } catch {
    return null
  }
}

function playgroundLink(text) {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const b of bytes) binary += String.fromCharCode(b)
  const hash = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  return `playground/#${hash}`
}

/** Replaces the static example with a textarea compiled live by the playground's wasm. */
async function makeEditable(highlight) {
  const { default: init, compile } = await import('./playground/pkg/stylet_wasm.js')
  await init()

  const sourcePre = $('example-source')
  const output = $('example-output').firstElementChild
  const diagnostics = $('example-diagnostics')
  const link = $('open-playground')

  const editor = document.createElement('div')
  editor.className = 'editor'
  const shown = document.createElement('pre')
  shown.setAttribute('aria-hidden', 'true')
  const code = document.createElement('code')
  shown.append(code)
  const textarea = document.createElement('textarea')
  textarea.spellcheck = false
  textarea.setAttribute('aria-label', 'stylet example (editable)')
  textarea.value = sourcePre.textContent
  editor.append(shown, textarea)
  sourcePre.replaceWith(editor)

  const render = (element, text, language) => {
    if (highlight) element.innerHTML = highlight(text, language)
    else element.textContent = text
  }

  const update = () => {
    const text = textarea.value
    // A trailing newline needs a character after it to take up space.
    render(code, `${text} `, 'stylet')
    textarea.rows = text.split('\n').length
    const result = compile(text, false, false)
    render(output, result.css.trimEnd(), 'css')
    diagnostics.replaceChildren(
      ...result.diagnostics.map((d) => {
        const li = document.createElement('li')
        li.textContent = `${d.line}:${d.column} ${d.message}`
        return li
      }),
    )
    link.href = playgroundLink(text)
  }

  textarea.addEventListener('input', update)
  textarea.addEventListener('keydown', (event) => {
    if (event.key !== 'Tab' || event.shiftKey) return
    event.preventDefault()
    textarea.setRangeText('  ', textarea.selectionStart, textarea.selectionEnd, 'end')
    update()
  })
  update()
}

const highlight = await loadHighlighter()
if (highlight) {
  for (const code of document.querySelectorAll('code[data-language]')) {
    code.innerHTML = highlight(code.textContent, code.dataset.language)
  }
}
try {
  await makeEditable(highlight)
} catch {}
