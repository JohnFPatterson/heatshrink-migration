PROJECT = heatshrink
OPTIMIZE = -O3
WARN = -Wall -Wextra -pedantic #-Werror
WARN += -Wmissing-prototypes
WARN += -Wstrict-prototypes
WARN += -Wmissing-declarations

# If libtheft is available, build additional property-based tests.
# Uncomment these to use it in test_heatshrink_dynamic.
#CFLAGS += -DHEATSHRINK_HAS_THEFT
#THEFT_PATH=	/usr/local/
#THEFT_INC=	-I${THEFT_PATH}/include/
#LDFLAGS += -L${THEFT_PATH}/lib -ltheft

CFLAGS += -std=c99 -g ${WARN} ${THEFT_INC} ${OPTIMIZE}

all: heatshrink test_runners libraries

libraries: libheatshrink_static.a libheatshrink_dynamic.a

test_runners: test_heatshrink_static test_heatshrink_dynamic
test: test_runners
	./test_heatshrink_static
	./test_heatshrink_dynamic
ci: test

# C↔Rust differential drivers and parity gate helpers
parity-build: build/oracle rust-driver

build/oracle: tools/heatshrink-oracle.c heatshrink_encoder.c heatshrink_decoder.c \
		heatshrink_encoder.h heatshrink_decoder.h heatshrink_common.h heatshrink_config.h
	mkdir -p build
	${CC} ${CFLAGS_DYNAMIC} -I. -o build/oracle.tmp tools/heatshrink-oracle.c \
		heatshrink_encoder.c heatshrink_decoder.c
	mv -f build/oracle.tmp build/oracle

rust-driver:
	cargo build --release --target-dir target -p heatshrink-driver

parity: parity-build
	printf '%s' '{"status":"completed","loop_count":0,"workspace_roots":["'"$$PWD"'"]}' \
		| ./.cursor/hooks/c-rust-parity/parity_gate.py --force

ffi-lib:
	cargo build --release --target-dir target -p heatshrink-ffi

# Link the public-API dynamic test suite against the Rust FFI staticlib.
ffi-tests: ffi-lib test_heatshrink_dynamic.od test_heatshrink_dynamic_theft.od
	${CC} -o test_heatshrink_dynamic_ffi test_heatshrink_dynamic.od \
		test_heatshrink_dynamic_theft.od \
		target/release/libheatshrink_ffi.a ${CFLAGS_DYNAMIC} -lpthread -ldl -lm
	./test_heatshrink_dynamic_ffi

export-check: ffi-lib
	@mkdir -p build
	@grep -hE '^(heatshrink_[a-z_]+)\(' heatshrink_encoder.h heatshrink_decoder.h \
		| sed 's/(.*//' | sort -u > build/header-fns.txt
	@nm -g target/release/libheatshrink_ffi.a 2>/dev/null \
		| awk 'NF>=3 && ($$2=="T"||$$2=="t"){print $$3}' | sed 's/^_//' | sort -u \
		> build/ffi-exports.txt
	@echo "Header functions:" && cat build/header-fns.txt
	@comm -23 build/header-fns.txt build/ffi-exports.txt > build/missing-exports.txt
	@if [ -s build/missing-exports.txt ]; then \
		echo "MISSING EXPORTS:"; cat build/missing-exports.txt; exit 1; \
	else echo "export-check: OK"; fi

asan-oracle: tools/heatshrink-oracle.c heatshrink_encoder.c heatshrink_decoder.c
	mkdir -p build/asan
	${CC} -g -fsanitize=address,undefined -fno-omit-frame-pointer \
		${CFLAGS_DYNAMIC} -I. -O1 -o build/asan/oracle \
		tools/heatshrink-oracle.c heatshrink_encoder.c heatshrink_decoder.c
	@for f in tests/inputs/*; do \
		ASAN_OPTIONS=detect_leaks=0 build/asan/oracle "$$f" >/dev/null \
			|| echo "ASAN: $$f"; \
	done
	@echo "asan-oracle: done"

clean:
	rm -f heatshrink test_heatshrink_{dynamic,static,dynamic_ffi} \
		*.o *.os *.od *.core *.a {dec,enc}_sm.png TAGS
	rm -rf ${BENCHMARK_OUT} build/oracle build/asan target

TAGS:
	etags *.[ch]

diagrams: dec_sm.png enc_sm.png

dec_sm.png: dec_sm.dot
	dot -o $@ -Tpng $<

enc_sm.png: enc_sm.dot
	dot -o $@ -Tpng $<

# Benchmarking
CORPUS_ARCHIVE=	cantrbry.tar.gz
CORPUS_URL=	http://corpus.canterbury.ac.nz/resources/${CORPUS_ARCHIVE}
BENCHMARK_OUT=	benchmark_out

## Uncomment one of these.
DL=	curl -o ${CORPUS_ARCHIVE}
#DL=	wget -O ${CORPUS_ARCHIVE}

bench: heatshrink corpus
	mkdir -p ${BENCHMARK_OUT}
	cd ${BENCHMARK_OUT} && tar vzxf ../${CORPUS_ARCHIVE}
	time ./benchmark

corpus: ${CORPUS_ARCHIVE}

${CORPUS_ARCHIVE}:
	${DL} ${CORPUS_URL}

# Installation
PREFIX ?=	/usr/local
INSTALL ?=	install
RM ?=		rm

install: libraries heatshrink
	${INSTALL} -c heatshrink ${PREFIX}/bin/
	${INSTALL} -c libheatshrink_static.a ${PREFIX}/lib/
	${INSTALL} -c libheatshrink_dynamic.a ${PREFIX}/lib/
	${INSTALL} -c heatshrink_common.h ${PREFIX}/include/
	${INSTALL} -c heatshrink_config.h ${PREFIX}/include/
	${INSTALL} -c heatshrink_encoder.h ${PREFIX}/include/
	${INSTALL} -c heatshrink_decoder.h ${PREFIX}/include/

uninstall:
	${RM} -f ${PREFIX}/lib/libheatshrink_static.a
	${RM} -f ${PREFIX}/lib/libheatshrink_dynamic.a
	${RM} -f ${PREFIX}/include/heatshrink_common.h
	${RM} -f ${PREFIX}/include/heatshrink_config.h
	${RM} -f ${PREFIX}/include/heatshrink_encoder.h
	${RM} -f ${PREFIX}/include/heatshrink_decoder.h

# Internal targets and rules

OBJS = heatshrink_encoder.o heatshrink_decoder.o

DYNAMIC_OBJS= $(OBJS:.o=.od)
STATIC_OBJS=  $(OBJS:.o=.os)

DYNAMIC_LDFLAGS= ${LDFLAGS} -L. -lheatshrink_dynamic
STATIC_LDFLAGS= ${LDFLAGS} -L. -lheatshrink_static

# Libraries should be built separately for versions
# with and without dynamic allocation.
CFLAGS_STATIC = ${CFLAGS} -DHEATSHRINK_DYNAMIC_ALLOC=0
CFLAGS_DYNAMIC = ${CFLAGS} -DHEATSHRINK_DYNAMIC_ALLOC=1

heatshrink: heatshrink.od libheatshrink_dynamic.a
	${CC} -o $@ $^ ${CFLAGS_DYNAMIC} -L. -lheatshrink_dynamic

test_heatshrink_dynamic: test_heatshrink_dynamic.od test_heatshrink_dynamic_theft.od libheatshrink_dynamic.a
	${CC} -o $@ $< ${CFLAGS_DYNAMIC} test_heatshrink_dynamic_theft.od ${DYNAMIC_LDFLAGS}

test_heatshrink_static: test_heatshrink_static.os libheatshrink_static.a
	${CC} -o $@ $< ${CFLAGS_STATIC} ${STATIC_LDFLAGS}

libheatshrink_static.a: ${STATIC_OBJS}
	ar -rcs $@ $^

libheatshrink_dynamic.a: ${DYNAMIC_OBJS}
	ar -rcs $@ $^

%.od: %.c
	${CC} -c -o $@ $< ${CFLAGS_DYNAMIC}

%.os: %.c
	${CC} -c -o $@ $< ${CFLAGS_STATIC}

*.os: Makefile *.h
*.od: Makefile *.h

