/* Heatshrink C oracle driver — public headers only.
 * Format: tools/DRIVER_FORMAT.md
 */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <errno.h>

#include "heatshrink_encoder.h"
#include "heatshrink_decoder.h"

#define WINDOW_SZ2 8
#define LOOKAHEAD_SZ2 4
#define DECODER_INPUT_SIZE 256
#define IO_CHUNK 16

static int want_encoder = 1;
static int want_decoder = 1;
static int want_roundtrip = 1;

static void die(const char *msg) {
    fprintf(stderr, "oracle: %s\n", msg);
    exit(2);
}

static uint8_t *read_file(const char *path, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    if (!f) die("fopen");
    if (fseek(f, 0, SEEK_END) != 0) die("fseek");
    long n = ftell(f);
    if (n < 0) die("ftell");
    if (fseek(f, 0, SEEK_SET) != 0) die("fseek");
    uint8_t *buf = malloc((size_t)n + 1);
    if (!buf) die("malloc");
    size_t got = fread(buf, 1, (size_t)n, f);
    fclose(f);
    if (got != (size_t)n) die("fread");
    *out_len = got;
    return buf;
}

static void print_hex(const uint8_t *data, size_t n) {
    fputs("hex:", stdout);
    for (size_t i = 0; i < n; i++) {
        printf("%02x", data[i]);
    }
    fputc('\n', stdout);
}

static int encode_buf(const uint8_t *in, size_t in_len,
                      uint8_t **out, size_t *out_len) {
    heatshrink_encoder *hse = heatshrink_encoder_alloc(WINDOW_SZ2, LOOKAHEAD_SZ2);
    if (!hse) return -1;

    size_t cap = in_len + (in_len / 8) + 16;
    if (cap < 64) cap = 64;
    uint8_t *buf = malloc(cap);
    if (!buf) { heatshrink_encoder_free(hse); return -1; }
    size_t filled = 0;

    size_t sunk_total = 0;
    while (sunk_total < in_len) {
        size_t sink_sz = 0;
        size_t chunk = in_len - sunk_total;
        if (chunk > IO_CHUNK) chunk = IO_CHUNK;
        HSE_sink_res sres = heatshrink_encoder_sink(hse,
            (uint8_t *)&in[sunk_total], chunk, &sink_sz);
        if (sres < 0) { free(buf); heatshrink_encoder_free(hse); return -1; }
        sunk_total += sink_sz;

        HSE_poll_res pres;
        do {
            size_t poll_sz = 0;
            if (filled + IO_CHUNK > cap) {
                cap *= 2;
                uint8_t *nb = realloc(buf, cap);
                if (!nb) { free(buf); heatshrink_encoder_free(hse); return -1; }
                buf = nb;
            }
            pres = heatshrink_encoder_poll(hse, &buf[filled], IO_CHUNK, &poll_sz);
            if (pres < 0) { free(buf); heatshrink_encoder_free(hse); return -1; }
            filled += poll_sz;
        } while (pres == HSER_POLL_MORE);
    }

    while (1) {
        HSE_finish_res fres = heatshrink_encoder_finish(hse);
        if (fres < 0) { free(buf); heatshrink_encoder_free(hse); return -1; }
        if (fres == HSER_FINISH_DONE) break;
        HSE_poll_res pres;
        do {
            size_t poll_sz = 0;
            if (filled + IO_CHUNK > cap) {
                cap *= 2;
                uint8_t *nb = realloc(buf, cap);
                if (!nb) { free(buf); heatshrink_encoder_free(hse); return -1; }
                buf = nb;
            }
            pres = heatshrink_encoder_poll(hse, &buf[filled], IO_CHUNK, &poll_sz);
            if (pres < 0) { free(buf); heatshrink_encoder_free(hse); return -1; }
            filled += poll_sz;
        } while (pres == HSER_POLL_MORE);
    }

    heatshrink_encoder_free(hse);
    *out = buf;
    *out_len = filled;
    return 0;
}

static int decode_buf(const uint8_t *in, size_t in_len,
                      uint8_t **out, size_t *out_len) {
    heatshrink_decoder *hsd = heatshrink_decoder_alloc(
        DECODER_INPUT_SIZE, WINDOW_SZ2, LOOKAHEAD_SZ2);
    if (!hsd) return -1;

    size_t cap = in_len * 2 + 64;
    if (cap < 64) cap = 64;
    uint8_t *buf = malloc(cap);
    if (!buf) { heatshrink_decoder_free(hsd); return -1; }
    size_t filled = 0;

    size_t sunk_total = 0;
    while (sunk_total < in_len) {
        size_t sink_sz = 0;
        size_t chunk = in_len - sunk_total;
        if (chunk > IO_CHUNK) chunk = IO_CHUNK;
        HSD_sink_res sres = heatshrink_decoder_sink(hsd,
            (uint8_t *)&in[sunk_total], chunk, &sink_sz);
        if (sres < 0) { free(buf); heatshrink_decoder_free(hsd); return -1; }
        sunk_total += sink_sz;

        HSD_poll_res pres;
        do {
            size_t poll_sz = 0;
            if (filled + IO_CHUNK > cap) {
                cap *= 2;
                uint8_t *nb = realloc(buf, cap);
                if (!nb) { free(buf); heatshrink_decoder_free(hsd); return -1; }
                buf = nb;
            }
            pres = heatshrink_decoder_poll(hsd, &buf[filled], IO_CHUNK, &poll_sz);
            if (pres < 0) { free(buf); heatshrink_decoder_free(hsd); return -1; }
            filled += poll_sz;
        } while (pres == HSDR_POLL_MORE);

        if (sres == HSDR_SINK_FULL && sink_sz == 0) {
            /* need to poll more before sinking; already polled */
            continue;
        }
    }

    while (1) {
        HSD_finish_res fres = heatshrink_decoder_finish(hsd);
        if (fres < 0) { free(buf); heatshrink_decoder_free(hsd); return -1; }
        if (fres == HSDR_FINISH_DONE) break;
        HSD_poll_res pres;
        do {
            size_t poll_sz = 0;
            if (filled + IO_CHUNK > cap) {
                cap *= 2;
                uint8_t *nb = realloc(buf, cap);
                if (!nb) { free(buf); heatshrink_decoder_free(hsd); return -1; }
                buf = nb;
            }
            pres = heatshrink_decoder_poll(hsd, &buf[filled], IO_CHUNK, &poll_sz);
            if (pres < 0) { free(buf); heatshrink_decoder_free(hsd); return -1; }
            filled += poll_sz;
        } while (pres == HSDR_POLL_MORE);
    }

    heatshrink_decoder_free(hsd);
    *out = buf;
    *out_len = filled;
    return 0;
}

static int path_is_compressed(const char *path) {
    return strstr(path, "/compressed/") != NULL
        || strstr(path, "\\compressed\\") != NULL;
}

static void parse_sections(const char *arg) {
    want_encoder = want_decoder = want_roundtrip = 0;
    char *copy = strdup(arg);
    if (!copy) die("strdup");
    char *save = NULL;
    for (char *tok = strtok_r(copy, ",", &save); tok;
         tok = strtok_r(NULL, ",", &save)) {
        if (strcmp(tok, "encoder") == 0) want_encoder = 1;
        else if (strcmp(tok, "decoder") == 0) want_decoder = 1;
        else if (strcmp(tok, "roundtrip") == 0) want_roundtrip = 1;
        else {
            fprintf(stderr, "unknown section: %s\n", tok);
            exit(2);
        }
    }
    free(copy);
}

int main(int argc, char **argv) {
    const char *path = NULL;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--sections") == 0) {
            if (i + 1 >= argc) die("--sections needs an argument");
            parse_sections(argv[++i]);
        } else if (argv[i][0] == '-') {
            die("unknown flag");
        } else {
            path = argv[i];
        }
    }
    if (!path) die("usage: oracle [--sections a,b] <fixture>");

    size_t in_len = 0;
    uint8_t *in = read_file(path, &in_len);
    int rc = 0;

    if (want_encoder) {
        uint8_t *enc = NULL;
        size_t enc_len = 0;
        if (encode_buf(in, in_len, &enc, &enc_len) != 0) {
            printf("encoder err encode_failed\n");
            rc = 1;
        } else {
            printf("encoder ok %zu\n", enc_len);
            print_hex(enc, enc_len);
            free(enc);
        }
    }

    if (want_decoder) {
        uint8_t *src = in;
        size_t src_len = in_len;
        uint8_t *tmp = NULL;
        size_t tmp_len = 0;
        int free_src = 0;
        if (!path_is_compressed(path)) {
            if (encode_buf(in, in_len, &tmp, &tmp_len) != 0) {
                printf("decoder err encode_failed\n");
                rc = 1;
                goto after_decoder;
            }
            src = tmp;
            src_len = tmp_len;
            free_src = 1;
        }
        uint8_t *dec = NULL;
        size_t dec_len = 0;
        if (decode_buf(src, src_len, &dec, &dec_len) != 0) {
            printf("decoder err decode_failed\n");
            rc = 1;
        } else {
            printf("decoder ok %zu\n", dec_len);
            print_hex(dec, dec_len);
            free(dec);
        }
        if (free_src) free(tmp);
    }
after_decoder:

    if (want_roundtrip) {
        uint8_t *enc = NULL;
        size_t enc_len = 0;
        uint8_t *dec = NULL;
        size_t dec_len = 0;
        if (encode_buf(in, in_len, &enc, &enc_len) != 0
            || decode_buf(enc, enc_len, &dec, &dec_len) != 0) {
            printf("roundtrip mismatch\n");
            free(enc);
            free(dec);
            rc = 1;
        } else if (dec_len != in_len || memcmp(dec, in, in_len) != 0) {
            printf("roundtrip mismatch\n");
            free(enc);
            free(dec);
            rc = 1;
        } else {
            printf("roundtrip ok %zu\n", dec_len);
            print_hex(dec, dec_len);
            free(enc);
            free(dec);
        }
    }

    free(in);
    return rc;
}
