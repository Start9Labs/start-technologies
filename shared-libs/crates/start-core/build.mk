start-core-test: $(CORE_SRC) $(ENVIRONMENT_FILE)
	./shared-libs/crates/start-core/run-tests.sh

start-core-ts-bindings: shared-libs/crates/start-core/bindings/index.ts
	mkdir -p shared-libs/ts-modules/start-core/lib/osBindings
	rsync -ac --delete shared-libs/crates/start-core/bindings/ shared-libs/ts-modules/start-core/lib/osBindings/

shared-libs/crates/start-core/bindings/index.ts: $(call ls-files, shared-libs/crates/start-core) $(ENVIRONMENT_FILE)
	rm -rf shared-libs/crates/start-core/bindings
	./shared-libs/crates/start-core/build/build-ts.sh
	ls shared-libs/crates/start-core/bindings/*.ts | sed 's|.*/bindings/\([^.]*\)\.ts|export { \1 } from "./\1";|g' | grep -v '"./index"' | tee shared-libs/crates/start-core/bindings/index.ts
	if [ -d shared-libs/crates/start-core/bindings/tunnel ]; then \
		ls shared-libs/crates/start-core/bindings/tunnel/*.ts | sed 's|.*/bindings/tunnel/\([^.]*\)\.ts|export { \1 } from "./\1";|g' | grep -v '"./index"' > shared-libs/crates/start-core/bindings/tunnel/index.ts; \
		echo 'export * as Tunnel from "./tunnel";' >> shared-libs/crates/start-core/bindings/index.ts; \
	fi
	npm --prefix shared-libs/ts-modules/start-core exec -- prettier -w './shared-libs/crates/start-core/bindings/**/*.ts'
	touch shared-libs/crates/start-core/bindings/index.ts

# --- generated-artifact freshness (CI gates: regenerate, then fail on any diff) ---
MAN_DIRS := projects/start-cli/man projects/start-registry/man projects/start-tunnel/man projects/start-os/man
OSBINDINGS_DIR := shared-libs/ts-modules/start-core/lib/osBindings

.PHONY: manpages manpages-check start-core-ts-bindings-check
# Regenerate the committed clap man pages (roff) for every product binary.
manpages:
	./shared-libs/crates/start-core/build/build-manpage.sh

# Fail if the committed man pages don't match the current CLI definitions.
manpages-check: manpages
	@if [ -n "$$(git status --porcelain -- $(MAN_DIRS))" ]; then \
		echo "Man pages are out of date — run 'make manpages' and commit the result:"; \
		git --no-pager diff --stat -- $(MAN_DIRS); git status --porcelain -- $(MAN_DIRS); \
		exit 1; \
	fi

# Fail if the committed TS osBindings don't match the current Rust types.
start-core-ts-bindings-check: start-core-ts-bindings
	@if [ -n "$$(git status --porcelain -- $(OSBINDINGS_DIR))" ]; then \
		echo "TS bindings are out of date — run 'make start-core-ts-bindings' and commit the result:"; \
		git --no-pager diff --stat -- $(OSBINDINGS_DIR); git status --porcelain -- $(OSBINDINGS_DIR); \
		exit 1; \
	fi

.PHONY: start-core-clean
# Owns the shared Rust build root, generated bindings, and global build metadata.
start-core-clean:
	rm -rf target shared-libs/crates/start-core/bindings shared-libs/crates/patch-db/target
	rm -f build/env/*.txt

# Formats every Rust crate under shared-libs/crates/ (the product crates have their own format targets).
SHARED_CRATE_PKGS := -p start-core -p exver -p imbl-value -p yasi -p rpc-toolkit -p jsonpath_lib -p pi-beep -p patch-db -p patch-db-macro -p patch-db-macro-internals -p patch-db-util -p json-patch -p json-ptr

.PHONY: start-core-format start-core-format-check
start-core-format:
	$(FMT) cargo fmt $(SHARED_CRATE_PKGS)

start-core-format-check:
	$(FMT) cargo fmt --check $(SHARED_CRATE_PKGS)
