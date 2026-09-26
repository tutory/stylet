// External scanner: emits the statement terminator at the end of a line
// (unless the next line continues the value or opens a block), before `}`
// and at the end of the file.
#include "tree_sitter/parser.h"

enum TokenType { TERMINATOR };

void *tree_sitter_stylet_external_scanner_create(void) { return NULL; }
void tree_sitter_stylet_external_scanner_destroy(void *payload) {}
unsigned tree_sitter_stylet_external_scanner_serialize(void *payload, char *buffer) { return 0; }
void tree_sitter_stylet_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

static bool is_space(int32_t c) { return c == ' ' || c == '\t'; }

bool tree_sitter_stylet_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  if (!valid_symbols[TERMINATOR]) return false;
  while (is_space(lexer->lookahead)) lexer->advance(lexer, true);
  lexer->result_symbol = TERMINATOR;
  if (lexer->eof(lexer) || lexer->lookahead == '}' || lexer->lookahead == ';') {
    lexer->mark_end(lexer);
    return true;
  }
  if (lexer->lookahead != '\n' && lexer->lookahead != '\r') return false;
  lexer->mark_end(lexer);
  // Look past the line break: a string continues the value, `{` opens a block.
  while (lexer->lookahead == '\n' || lexer->lookahead == '\r' || is_space(lexer->lookahead)) {
    lexer->advance(lexer, true);
  }
  int32_t next = lexer->lookahead;
  return !(next == '"' || next == '\'' || next == '{');
}
