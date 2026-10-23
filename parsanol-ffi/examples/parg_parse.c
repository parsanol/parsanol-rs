/*
 * Working C consumer of the parsanol PARG artifact ABI.
 *
 * Build (after `cargo build -p parsanol-ffi --release`):
 *   cc examples/parg_parse.c -I include \
 *      -o parg_parse -L ../target/release -lparsanol
 *   LD_LIBRARY_PATH=../target/release ./parg_parse demo.artifact.json "iso 12345"
 *
 * Exit status 0 = the input parsed; 1 = rejected (failure wire printed);
 * 2 = usage/IO error.
 */
#include <parsanol.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char *read_file(const char *path) {
    FILE *file = fopen(path, "rb");
    if (!file) return NULL;
    if (fseek(file, 0, SEEK_END) != 0) { fclose(file); return NULL; }
    long size = ftell(file);
    if (size < 0) { fclose(file); return NULL; }
    rewind(file);
    char *text = malloc((size_t)size + 1);
    if (!text) { fclose(file); return NULL; }
    if (fread(text, 1, (size_t)size, file) != (size_t)size) {
        fclose(file);
        free(text);
        return NULL;
    }
    text[size] = '\0';
    fclose(file);
    return text;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s <artifact.json> <input>\n", argv[0]);
        return 2;
    }
    printf("parsanol engine version: %s\n", parsanol_version());

    char *envelope = read_file(argv[1]);
    if (!envelope) {
        fprintf(stderr, "cannot read %s\n", argv[1]);
        return 2;
    }

    char *shaped = parsanol_parg_parse(envelope, argv[2]);
    if (shaped) {
        printf("tree: %s\n", shaped);
        parsanol_parg_free(shaped);

        /* Recovery on a successful parse reports the clean shape. */
        char *recovered = parsanol_parg_parse_recovering(envelope, NULL, argv[2]);
        if (recovered) {
            printf("recovery: %s\n", recovered);
            parsanol_parg_free(recovered);
        }
        free(envelope);
        return 0;
    }

    char *wire = parsanol_parg_error(envelope, argv[2]);
    printf("rejected: %s\n", wire ? wire : parsanol_c_last_error());
    if (wire) parsanol_parg_free(wire);

    /* Recovery: the deepest failure, the partial tree, and the declared
     * resync spans — byte-identical to every other engine. */
    char *recovered = parsanol_parg_parse_recovering(envelope, NULL, argv[2]);
    if (recovered) {
        printf("recovery: %s\n", recovered);
        parsanol_parg_free(recovered);
    }
    free(envelope);
    return 1;
}
