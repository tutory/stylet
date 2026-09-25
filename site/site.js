// Highlights the code samples; the page reads fine without it.
const HLJS = 'https://cdn.jsdelivr.net/npm/@highlightjs/cdn-assets@11.12.0/es'

try {
  const [{ default: hljs }, { default: css }, { default: bash }, { default: stylet }] = await Promise.all([
    import(`${HLJS}/core.min.js`),
    import(`${HLJS}/languages/css.min.js`),
    import(`${HLJS}/languages/bash.min.js`),
    import('./playground/stylet-hljs.js'),
  ])
  hljs.registerLanguage('css', css)
  hljs.registerLanguage('bash', bash)
  hljs.registerLanguage('stylet', stylet)
  for (const code of document.querySelectorAll('code[data-language]')) {
    code.innerHTML = hljs.highlight(code.textContent, { language: code.dataset.language }).value
  }
} catch {}
