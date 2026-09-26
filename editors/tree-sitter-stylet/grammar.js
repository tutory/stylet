/**
 * tree-sitter grammar for stylet.
 *
 * A newline ends a declaration or statement at-rule; the external scanner
 * emits that terminator unless the next line continues a value (it starts
 * with a string) or opens a block. Inside brackets and after commas the
 * parser doesn't expect a terminator, so values continue there.
 */
const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)))

module.exports = grammar({
  name: 'stylet',

  externals: ($) => [$._terminator],

  extras: ($) => [/\s/, $.comment],

  rules: {
    stylesheet: ($) => repeat($._statement),

    _statement: ($) =>
      choice($.import_statement, $.extend_statement, $.at_rule, $.placeholder_rule, $.rule_set, $.declaration),

    comment: (_) => token(choice(seq('//', /[^\n]*/), seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'))),

    import_statement: ($) => seq('@import', $._values, $._terminator),

    extend_statement: ($) => seq(choice('@extend', '@extends'), commaSep1($.placeholder_name), $._terminator),

    at_rule: ($) => seq($.at_keyword, optional($.prelude), choice($.block, $._terminator)),

    at_keyword: (_) => /@[a-zA-Z-][\w-]*/,

    prelude: ($) => seq($._prelude_item, repeat(choice($._prelude_item, seq(',', $._prelude_item)))),

    _prelude_item: ($) => choice($._value, ':'),

    placeholder_rule: ($) => seq($.placeholder_name, $.block),

    placeholder_name: (_) => /\$[a-zA-Z_-][\w-]*/,

    rule_set: ($) => seq($.selectors, $.block),

    selectors: ($) => repeat1(choice($._selector_part, ',')),

    _selector_part: ($) =>
      choice(
        $.class_selector,
        $.id_selector,
        $.pseudo_selector,
        $.attribute_selector,
        $.nesting_selector,
        $.universal_selector,
        $.combinator,
        alias($.identifier, $.tag_name),
        $.percentage,
        $.string,
      ),

    class_selector: (_) => /\.-?([a-zA-Z_]|\\.)([\w-]|\\.)*/,
    id_selector: (_) => /#-?([a-zA-Z_]|\\.)([\w-]|\\.)*/,
    pseudo_selector: ($) => seq(/::?[a-zA-Z-][\w-]*/, optional(seq(token.immediate('('), repeat(choice($._selector_part, ',', $.number)), ')'))),
    attribute_selector: ($) => seq('[', /[a-zA-Z_-][\w-]*/, optional(seq(/[~|^$*]?=/, choice($.string, $.identifier))), optional(/[is]/), ']'),
    nesting_selector: (_) => '&',
    universal_selector: (_) => '*',
    combinator: (_) => choice('>', '+', '~'),

    block: ($) => seq('{', repeat($._statement), '}'),

    declaration: ($) =>
      seq(field('property', choice($.property_name, $.custom_property_name)), ':', optional($.value), $._terminator),

    property_name: ($) => alias($.identifier, 'property_name'),
    custom_property_name: (_) => /--[\w-]*/,

    value: ($) => $._values,

    // A comma always needs a following value, so the parser expects no
    // terminator after it and the value continues on the next line.
    _values: ($) => seq($._value, repeat(choice($._value, seq(',', $._value)))),

    _value: ($) =>
      choice(
        $.identifier,
        $.number,
        $.percentage,
        $.color,
        $.string,
        $.url,
        $.call_expression,
        $.important,
        $.custom_property_name,
        $.placeholder_name,
        alias($.at_keyword, $.property_lookup),
        $.parenthesized_value,
        '/',
        '*',
        '+',
        '-',
        '=',
      ),

    identifier: (_) => /-?([a-zA-Z_]|\\.)([\w-]|\\.)*/,
    number: (_) => /[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?[a-zA-Z]*/,
    percentage: (_) => /[-+]?(\d+\.?\d*|\.\d+)%/,
    color: (_) => /#[0-9a-fA-F]{3,8}/,
    string: (_) => token(choice(seq("'", /([^'\\\n]|\\.)*/, "'"), seq('"', /([^"\\\n]|\\.)*/, '"'))),
    url: (_) => /url\([^)"']*\)/,
    important: (_) => /!\s*important/,
    call_expression: ($) => seq(alias($.identifier, $.function_name), token.immediate('('), repeat(choice($._value, ':', ',')), ')'),
    parenthesized_value: ($) => seq('(', repeat(choice($._value, ':', ',', /[<>]=?/)), ')'),
  },
})
