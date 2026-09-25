/*
 * highlight.js language definition for stylet.
 *
 *   import hljs from 'highlight.js/lib/core'
 *   import stylet from './stylet-hljs.js'
 *   hljs.registerLanguage('stylet', stylet)
 */
export default function stylet(hljs) {
  const LINE_COMMENT = hljs.COMMENT('//', '$')
  const BLOCK_COMMENT = hljs.C_BLOCK_COMMENT_MODE
  const COMMENTS = [LINE_COMMENT, BLOCK_COMMENT]
  const STRINGS = [hljs.APOS_STRING_MODE, hljs.QUOTE_STRING_MODE]

  const PLACEHOLDER = { scope: 'variable', match: /\$[\w-]+/ }
  const NUMBER = { scope: 'number', match: /[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?(%|[a-z]+)?/i, relevance: 0 }
  const HEX = { scope: 'number', match: /#[0-9a-f]{3,8}\b/i }
  const IMPORTANT = { scope: 'meta', match: /!important\b/ }
  const CUSTOM_PROPERTY = { scope: 'variable', match: /--[\w-]+/ }
  const FUNCTION = { scope: 'built_in', match: /[\w-]+(?=\()/ }

  const VALUE = {
    begin: /:/,
    end: /$|(?=})/,
    excludeBegin: true,
    contains: [...COMMENTS, ...STRINGS, IMPORTANT, HEX, NUMBER, CUSTOM_PROPERTY, FUNCTION],
  }

  const DECLARATION = {
    begin: /(--)?[a-z][\w-]*(?=\s*:(\s|$))/i,
    returnBegin: true,
    end: /$|(?=})/,
    contains: [{ scope: 'attribute', match: /(--)?[a-z][\w-]*/i }, VALUE],
  }

  const AT_RULE = {
    scope: 'keyword',
    match: /@[\w-]+/,
  }

  const PRELUDE_PARTS = [...COMMENTS, ...STRINGS, PLACEHOLDER, NUMBER, CUSTOM_PROPERTY, FUNCTION]
  const AT_STATEMENT = {
    begin: /@[\w-]+/,
    beginScope: 'keyword',
    end: /$|(?=[{}])/,
    contains: PRELUDE_PARTS,
  }

  const SELECTOR_PARTS = [
    { scope: 'selector-class', match: /\.[\w-]+/ },
    { scope: 'selector-id', match: /#[\w-]+/ },
    { scope: 'selector-pseudo', match: /::?[\w-]+/ },
    { scope: 'selector-attr', begin: /\[/, end: /\]/, contains: STRINGS },
    { scope: 'selector-tag', match: /&|\*/ },
    PLACEHOLDER,
  ]

  const BLOCK = {
    begin: /\{/,
    end: /\}/,
    contains: [],
  }
  BLOCK.contains.push(...COMMENTS, AT_STATEMENT, DECLARATION, ...SELECTOR_PARTS, BLOCK)

  return {
    name: 'stylet',
    aliases: ['styl'],
    case_insensitive: true,
    contains: [...COMMENTS, AT_STATEMENT, AT_RULE, ...SELECTOR_PARTS, { scope: 'selector-tag', match: /\b[a-z][\w-]*\b/, relevance: 0 }, BLOCK],
  }
}
