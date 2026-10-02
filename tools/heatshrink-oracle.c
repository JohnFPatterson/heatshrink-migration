/* Differential oracle driver for heatshrink. Public API only. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include "heatshrink_encoder.h"
#include "heatshrink_decoder.h"

#define WBITS 8
#define LBITS 4
#define IBS 32

static int want_encode = 1;
static int want_decode = 1;
static int want_roundtrip = 1;
static int want_stream = 1;

static void enable_section(const char *tok) {
    if (strcmp(tok, "encode") == 0) want_encode = 1;
    else if (strcmp(tok, "decode") == 0) want_decode = 1;
    else if (strcmp(tok, "roundtrip") == 0) want_roundtrip = 1;
    else if (strcmp(tok, "stream") == 0) want_stream = 1;
}

static void parse_sections(int argc, char **argv) {
    for (int i = 2; i < argc; i++) {
        if (strcmp(argv[i], "--sections") == 0 && i + 1 < argc) {
            want_encode = want_decode = want_roundtrip = want_stream = 0;
            char *list = argv[++i];
            char buf[256];
            size_t n = strlen(list);
            if (n >= sizeof(buf)) n = sizeof(buf) - 1;
            memcpy(buf, list, n);
            buf[n] = '\0';
            char *p = buf;
            while (*p) {
                char *start = p;
                while (*p && *p != ',') p++;
                if (*p == ',') { *p = '\0'; p++; }
                enable_section(start);
            }
        }
    }
}

static uint8_t *read_file(const char *path, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return NULL; }
    long n = ftell(f);
    if (n < 0) { fclose(f); return NULL; }
    rewind(f);
    uint8_t *buf = malloc((size_t)n + 1);
    if (!buf) { fclose(f); return NULL; }
    size_t got = fread(buf, 1, (size_t)n, f);
    fclose(f);
    if (got != (size_t)n) { free(buf); return NULL; }
    *out_len = (size_t)n;
    return buf;
}

static void print_hex(const uint8_t *buf, size_t len) {
    for (size_t i = 0; i < len; i++) printf("%02x", buf[i]);
}

static int grow(uint8_t **buf, size_t *cap, size_t need) {
    if (need <= *cap) return 0;
    size_t ncap = *cap ? *cap * 2 : 64;
    while (ncap < need) ncap *= 2;
    uint8_t *nbuf = realloc(*buf, ncap);
    if (!nbuf) return -1;
    *buf = nbuf;
    *cap = ncap;
    return 0;
}

/* Append at most src_cap bytes from src into a growable buffer. */
static int append_bounded(uint8_t **dst, size_t *dst_len, size_t *dst_cap,
                          const uint8_t *src, size_t n, size_t src_cap) {
    if (n > src_cap) return -1;
    if (grow(dst, dst_cap, *dst_len + n) != 0) return -1;
    if (n > 0) {
        memcpy(*dst + *dst_len, src, n);
    }
    *dst_len += n;
    return 0;
}

typedef int (*poll_fn)(void *ctx, uint8_t *out, size_t out_sz, size_t *out_len);

static int poll_until_empty(void *ctx, poll_fn poll, uint8_t *tmp, size_t tsz,
                            uint8_t **dst, size_t *dst_len, size_t *dst_cap) {
    for (;;) {
        size_t polled = 0;
        int pres = poll(ctx, tmp, tsz, &polled);
        if (pres < 0) return -1;
        /* Bound copy length to the poll buffer capacity (c:S3519). */
        if (append_bounded(dst, dst_len, dst_cap, tmp, polled, tsz) != 0) {
            return -1;
        }
        if (pres != 1) return 0; /* 1 == MORE for both encoder and decoder */
    }
}

static int enc_poll_adapter(void *ctx, uint8_t *out, size_t out_sz, size_t *out_len) {
    return (int)heatshrink_encoder_poll((heatshrink_encoder *)ctx, out, out_sz, out_len);
}

static int dec_poll_adapter(void *ctx, uint8_t *out, size_t out_sz, size_t *out_len) {
    return (int)heatshrink_decoder_poll((heatshrink_decoder *)ctx, out, out_sz, out_len);
}

static int encode_all(const uint8_t *in, size_t in_len,
                      uint8_t **out, size_t *out_len,
                      size_t chunk, const char **finish_name) {
    heatshrink_encoder *hse = heatshrink_encoder_alloc(WBITS, LBITS);
    if (!hse) return -1;

    size_t cap = 64;
    uint8_t *comp = malloc(cap);
    if (!comp) { heatshrink_encoder_free(hse); return -1; }
    size_t clen = 0;
    size_t tsz = chunk ? chunk : 1;
    uint8_t *tmp = malloc(tsz);
    if (!tmp) { free(comp); heatshrink_encoder_free(hse); return -1; }

    size_t offset = 0;
    while (offset < in_len) {
        size_t sunk = 0;
        size_t avail = in_len - offset;
        size_t to_sink = avail < tsz ? avail : tsz;
        if (heatshrink_encoder_sink(hse, (uint8_t *)(in + offset), to_sink, &sunk) < 0) {
            free(tmp); free(comp); heatshrink_encoder_free(hse); return -1;
        }
        offset += sunk;
        if (poll_until_empty(hse, enc_poll_adapter, tmp, tsz, &comp, &clen, &cap) != 0) {
            free(tmp); free(comp); heatshrink_encoder_free(hse); return -1;
        }
    }

    HSE_finish_res fres;
    for (;;) {
        fres = heatshrink_encoder_finish(hse);
        if (fres < 0) {
            free(tmp); free(comp); heatshrink_encoder_free(hse); return -1;
        }
        if (poll_until_empty(hse, enc_poll_adapter, tmp, tsz, &comp, &clen, &cap) != 0) {
            free(tmp); free(comp); heatshrink_encoder_free(hse); return -1;
        }
        if (fres == HSER_FINISH_DONE) break;
    }

    *finish_name = (fres == HSER_FINISH_DONE) ? "DONE" :
                   (fres == HSER_FINISH_MORE) ? "MORE" : "ERROR";
    free(tmp);
    heatshrink_encoder_free(hse);
    *out = comp;
    *out_len = clen;
    return 0;
}

static int decode_all(const uint8_t *in, size_t in_len,
                      uint8_t **out, size_t *out_len,
                      size_t chunk, const char **finish_name) {
    heatshrink_decoder *hsd = heatshrink_decoder_alloc(IBS, WBITS, LBITS);
    if (!hsd) return -1;

    size_t cap = 64;
    uint8_t *exp = malloc(cap);
    if (!exp) { heatshrink_decoder_free(hsd); return -1; }
    size_t elen = 0;
    size_t tsz = chunk ? chunk : 1;
    uint8_t *tmp = malloc(tsz);
    if (!tmp) { free(exp); heatshrink_decoder_free(hsd); return -1; }

    size_t offset = 0;
    while (offset < in_len) {
        size_t sunk = 0;
        size_t avail = in_len - offset;
        size_t to_sink = avail < tsz ? avail : tsz;
        HSD_sink_res sres = heatshrink_decoder_sink(hsd,
            (uint8_t *)(in + offset), to_sink, &sunk);
        if (sres < 0) {
            free(tmp); free(exp); heatshrink_decoder_free(hsd); return -1;
        }
        if (!(sres == HSDR_SINK_FULL && sunk == 0)) {
            offset += sunk;
        }
        if (poll_until_empty(hsd, dec_poll_adapter, tmp, tsz, &exp, &elen, &cap) != 0) {
            free(tmp); free(exp); heatshrink_decoder_free(hsd); return -1;
        }
    }

    HSD_finish_res fres;
    for (;;) {
        fres = heatshrink_decoder_finish(hsd);
        if (fres < 0) {
            free(tmp); free(exp); heatshrink_decoder_free(hsd); return -1;
        }
        if (poll_until_empty(hsd, dec_poll_adapter, tmp, tsz, &exp, &elen, &cap) != 0) {
            free(tmp); free(exp); heatshrink_decoder_free(hsd); return -1;
        }
        if (fres == HSDR_FINISH_DONE) break;
    }

    *finish_name = (fres == HSDR_FINISH_DONE) ? "DONE" :
                   (fres == HSDR_FINISH_MORE) ? "MORE" : "ERROR";
    free(tmp);
    heatshrink_decoder_free(hsd);
    *out = exp;
    *out_len = elen;
    return 0;
}

static void do_encode(const uint8_t *in, size_t in_len) {
    printf("=== encode ===\n");
    printf("w=%d l=%d ibs=%d\n", WBITS, LBITS, IBS);
    uint8_t *comp = NULL;
    size_t clen = 0;
    const char *fin = "ERROR";
    if (encode_all(in, in_len, &comp, &clen, 256, &fin) != 0) {
        printf("enc_ok=0\nenc_finish=ERROR\nenc_len=0\nenc_hex=\n");
        return;
    }
    printf("enc_ok=1\nenc_finish=%s\nenc_len=%zu\nenc_hex=", fin, clen);
    print_hex(comp, clen);
    printf("\n");
    free(comp);
}

static void do_decode(const uint8_t *in, size_t in_len) {
    printf("=== decode ===\n");
    printf("w=%d l=%d ibs=%d\n", WBITS, LBITS, IBS);
    uint8_t *comp = NULL;
    size_t clen = 0;
    const char *efin = "ERROR";
    if (encode_all(in, in_len, &comp, &clen, 256, &efin) != 0) {
        printf("dec_ok=0\ndec_finish=ERROR\ndec_len=0\ndec_hex=\n");
        return;
    }
    uint8_t *exp = NULL;
    size_t elen = 0;
    const char *dfin = "ERROR";
    int rc = decode_all(comp, clen, &exp, &elen, 256, &dfin);
    free(comp);
    if (rc != 0) {
        printf("dec_ok=0\ndec_finish=ERROR\ndec_len=0\ndec_hex=\n");
        return;
    }
    printf("dec_ok=1\ndec_finish=%s\ndec_len=%zu\ndec_hex=", dfin, elen);
    print_hex(exp, elen);
    printf("\n");
    free(exp);
}

static void do_roundtrip(const uint8_t *in, size_t in_len, size_t chunk) {
    if (chunk == 1) {
        printf("=== stream ===\n");
        printf("w=%d l=%d ibs=%d chunk=1\n", WBITS, LBITS, IBS);
    } else {
        printf("=== roundtrip ===\n");
        printf("w=%d l=%d ibs=%d\n", WBITS, LBITS, IBS);
    }
    uint8_t *comp = NULL;
    size_t clen = 0;
    const char *efin = "ERROR";
    if (encode_all(in, in_len, &comp, &clen, chunk, &efin) != 0) {
        printf("match=no\nin_len=%zu\nout_len=0\n", in_len);
        return;
    }
    uint8_t *exp = NULL;
    size_t elen = 0;
    const char *dfin = "ERROR";
    int rc = decode_all(comp, clen, &exp, &elen, chunk, &dfin);
    free(comp);
    if (rc != 0) {
        printf("match=no\nin_len=%zu\nout_len=0\n", in_len);
        return;
    }
    int match = (elen == in_len) && (in_len == 0 || memcmp(in, exp, in_len) == 0);
    printf("match=%s\nin_len=%zu\nout_len=%zu\n", match ? "yes" : "no", in_len, elen);
    free(exp);
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <fixture> [--sections name[,name…]]\n", argv[0]);
        return 1;
    }
    parse_sections(argc, argv);
    size_t in_len = 0;
    uint8_t *in = read_file(argv[1], &in_len);
    if (!in) {
        fprintf(stderr, "cannot read %s\n", argv[1]);
        return 1;
    }

    if (want_encode) do_encode(in, in_len);
    if (want_decode) do_decode(in, in_len);
    if (want_roundtrip) do_roundtrip(in, in_len, 256);
    if (want_stream) do_roundtrip(in, in_len, 1);

    free(in);
    return 0;
}
