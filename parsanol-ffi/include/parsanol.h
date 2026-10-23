/*
 * parsanol — the C ABI for PARG artifacts.
 *
 * One cdylib (`libparsanol.{so,dylib,dll}`), every consumer: this is the
 * same binary the Ruby ffi tier ships, and the same wire every runtime
 * parses with. All strings are UTF-8; returned strings are owned by the
 * caller and released with `parsanol_parg_free` unless marked static.
 *
 * See examples/parg_parse.c for a working consumer.
 */
#ifndef PARSANOL_H
#define PARSANOL_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Static version string (e.g. "0.18.19"). Never freed. */
const char *parsanol_version(void);

/* Static error text for the last failing call. Never freed. */
const char *parsanol_c_last_error(void);

/* -------------------------------------------------------------------------
 * PARG artifact ABI: strict parse, failure wire, recovery.
 *
 * `envelope_json` is a full parsanol-tree/v2 artifact (checksum-verified
 * here). `entry` selects the entry point (NULL = the artifact's default).
 * ------------------------------------------------------------------------- */

/* Shaped tree JSON on success (owned; parsanol_parg_free), NULL when the
 * input is rejected — call parsanol_parg_error for the failure wire. */
char *parsanol_parg_parse(const char *grammar_or_envelope_json,
                          const char *input);

/* Failure-wire JSON (owned; parsanol_parg_free): {offset, ranked}. */
char *parsanol_parg_error(const char *grammar_or_envelope_json,
                          const char *input);

/* Recovery envelope JSON (owned; parsanol_parg_free), never fails hard:
 *   success  -> {ok: true, shape: <tree>}
 *   failure  -> {ok: false, partial, error, resyncs, captures,
 *                consumed_upto}
 * Byte-identical to the wasm `parseShapeRecovering` wire, including the
 * spans of declared `resync` policies. */
char *parsanol_parg_parse_recovering(const char *envelope_json,
                                     const char *entry,
                                     const char *input);

/* Frees any string returned by the parsanol_parg_* functions. */
void parsanol_parg_free(char *ptr);

/* -------------------------------------------------------------------------
 * Batch ABI (register once, parse many): results cross the boundary as a
 * flat u64 array decoded by the caller (the Ruby ffi tier's format).
 * ------------------------------------------------------------------------- */

unsigned long long parsanol_c_register(const char *grammar_json);
int parsanol_c_release(unsigned long long handle);
long long parsanol_c_parse(unsigned long long handle, const char *input,
                           long long *out, long long out_len);
long long parsanol_c_parse_len(unsigned long long handle, const char *input,
                               size_t input_len, long long *out,
                               long long out_len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* PARSANOL_H */
