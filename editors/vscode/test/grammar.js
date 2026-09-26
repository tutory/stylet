// Tokenizes a sample with the TextMate grammar (the same engine VS Code uses)
// and checks the scopes of key tokens: node test/grammar.js [--print]
const fs = require('fs')
const path = require('path')
const assert = require('assert/strict')
const { Registry, INITIAL } = require('vscode-textmate')
const oniguruma = require('vscode-oniguruma')

const wasm = fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')).buffer
const onigLib = oniguruma.loadWASM(wasm).then(() => ({
  createOnigScanner: (patterns) => new oniguruma.OnigScanner(patterns),
  createOnigString: (s) => new oniguruma.OnigString(s),
}))
const registry = new Registry({
  onigLib,
  loadGrammar: async () =>
    JSON.parse(fs.readFileSync(path.join(__dirname, '../syntaxes/stylet.tmLanguage.json'), 'utf8')),
})

const SAMPLE = `// comment
@import '/client/base' layer(base)
@custom-media --phone (width < 600px)

$button {
  padding: .5rem 1rem
}

.app > nav, #main:hover::before {
  --gap: 4px
  color: #fff !important
  grid-template-areas: 'a b'
                       'c d'
  width: calc(100% - var(--gap))
  background: url(img/a.png)
  @extend $button

  &.active { display: none }
  input[type="text"] {
    border: 0 /* c */
  }

  @media (--phone) {
    display: block
  }
}
`

registry.loadGrammar('source.stylet').then((grammar) => {
  let state = INITIAL
  const tokens = []
  for (const line of SAMPLE.split('\n')) {
    const result = grammar.tokenizeLine(line, state)
    for (const t of result.tokens) {
      const text = line.slice(t.startIndex, t.endIndex)
      if (text.trim()) tokens.push([text, t.scopes[t.scopes.length - 1]])
    }
    state = result.ruleStack
  }
  if (process.argv.includes('--print')) {
    for (const [text, scope] of tokens) console.log(JSON.stringify(text).padEnd(34), scope)
    return
  }
  const scope = (text) => tokens.find(([t]) => t === text)?.[1]
  const expect = {
    '// comment': 'comment.line.double-slash.stylet',
    import: 'keyword.control.at-rule.stylet',
    extend: 'keyword.control.at-rule.stylet',
    '--phone': 'variable.argument.css',
    '$button': 'entity.other.attribute-name.placeholder.stylet',
    padding: 'support.type.property-name.css',
    '.5': 'constant.numeric.css',
    rem: 'keyword.other.unit.css',
    '.app': 'entity.other.attribute-name.class.css',
    nav: 'entity.name.tag.css',
    '#main': 'entity.other.attribute-name.id.css',
    ':hover': 'entity.other.attribute-name.pseudo-class.css',
    '::before': 'entity.other.attribute-name.pseudo-element.css',
    '--gap': 'variable.css',
    '#fff': 'constant.other.color.rgb-value.hex.css',
    '!important': 'keyword.other.important.css',
    'c d': 'string.quoted.single.css',
    calc: 'support.function.misc.css',
    'img/a.png': 'variable.parameter.url.css',
    '&': 'entity.name.tag.nesting.css',
    input: 'entity.name.tag.css',
    ' c ': 'comment.block.css',
  }
  for (const [text, want] of Object.entries(expect)) assert.equal(scope(text), want, text)
  console.log(`grammar ok (${Object.keys(expect).length} checks)`)
})
