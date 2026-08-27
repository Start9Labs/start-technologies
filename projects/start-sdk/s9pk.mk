# ** Plumbing. DO NOT EDIT **.
# This file is imported by ./Makefile. Make edits there

PACKAGE_ID := $(shell awk -F"'" '/id:/ {print $$2}' startos/manifest/index.ts)
# INGREDIENTS is the whole source-dependency list, javascript/index.js included —
# nothing else ties the s9pk to your TypeScript. $(shell) discards exit status,
# so a failure here would leave it empty, detaching every source file from the
# s9pk's prerequisites; make would then call an existing s9pk up to date and
# print "Build Complete" over the *previous* build. The sentinel turns that into
# an error. Not $(.SHELLSTATUS): it is undefined on the GNU Make 3.81 that
# macOS still ships, where the guard would fire on every build instead.
INGREDIENTS := $(shell start-cli s9pk list-ingredients 2>/dev/null || echo __LIST_INGREDIENTS_FAILED__)
ifneq ($(filter __LIST_INGREDIENTS_FAILED__,$(INGREDIENTS)),)
$(error `start-cli s9pk list-ingredients` failed, so make cannot tell which files the s9pk depends on and would silently repack the previous build. Run it directly to see the error.)
endif
# Resolve the actual git dir so this works inside git worktrees, where .git
# is a file pointing at <main>/.git/worktrees/<name> rather than a directory.
GIT_DIR := $(shell git rev-parse --git-dir 2>/dev/null)
# These are rebuild triggers (the manifest embeds the commit hash), not build
# requirements — pack degrades to a null gitHash on its own. $(wildcard) drops
# whichever don't exist: a freshly `git init`ed repo has no index until the
# first `git add`, and a hard prerequisite on it halts make with "No rule to
# make target".
GIT_DEPS := $(if $(GIT_DIR),$(wildcard $(GIT_DIR)/HEAD $(GIT_DIR)/index))
ARCHES ?= x86 arm riscv
# TARGETS is the list of leaf make-targets the build matrix fans out over.
# Defaults to the arches; variant packages override (e.g. immich, ollama, vllm
# set this to a list of variant or variant-arch leaf targets).
TARGETS ?= $(ARCHES)
ifdef VARIANT
BASE_NAME := $(PACKAGE_ID)_$(VARIANT)
else
BASE_NAME := $(PACKAGE_ID)
endif

.PHONY: all arches aarch64 x86_64 riscv64 arm arm64 x86 riscv arch/* clean install check-deps check-init package ingredients format
.DELETE_ON_ERROR:
.SECONDARY:

SDK_DIR := node_modules/@start9labs/start-sdk
PRETTIER_CONFIG := $(SDK_DIR)/prettier.config.json

# The SDK supplies typescript, prettier and ncc, so these resolve from your
# node_modules without the package declaring them. Override any of them in
# ./Makefile above the include to customize a step.
TS_CHECK ?= npx tsc --noEmit
FORMAT_CHECK ?= npx prettier --config $(PRETTIER_CONFIG) --check startos
JS_BUNDLE ?= rm -rf javascript && npx ncc build startos/index.ts -o javascript

define SUMMARY
	@manifest=$$(start-cli s9pk inspect $(1) manifest); \
	size=$$(du -h $(1) | awk '{print $$1}'); \
	title=$$(printf '%s' "$$manifest" | jq -r .title); \
	version=$$(printf '%s' "$$manifest" | jq -r .version); \
	arches=$$(printf '%s' "$$manifest" | jq -r '[.images[].arch // []] | flatten | unique | join(", ")'); \
	sdkv=$$(printf '%s' "$$manifest" | jq -r .sdkVersion); \
	gitHash=$$(printf '%s' "$$manifest" | jq -r .gitHash | sed -E 's/(.*-modified)$$/\x1b[0;31m\1\x1b[0m/'); \
	printf "\n"; \
	printf "\033[1;32m✅ Build Complete!\033[0m\n"; \
	printf "\n"; \
	printf "\033[1;37m📦 $$title\033[0m   \033[36mv$$version\033[0m\n"; \
	printf "───────────────────────────────\n"; \
	printf " \033[1;36mFilename:\033[0m   %s\n" "$(1)"; \
	printf " \033[1;36mSize:\033[0m       %s\n" "$$size"; \
	printf " \033[1;36mArch:\033[0m       %s\n" "$$arches"; \
	printf " \033[1;36mSDK:\033[0m        %s\n" "$$sdkv"; \
	printf " \033[1;36mGit:\033[0m        %s\n" "$$gitHash"; \
	echo ""
endef

all: $(TARGETS)

arches: $(ARCHES)

# Generic make-variable introspection. Used by the release workflow to
# read $(TARGETS) and fan out one matrix runner per target. `make -s
# print-TARGETS` echoes the list with no other output.
print-%:
	@echo '$($*)'

universal: $(BASE_NAME).s9pk
	$(call SUMMARY,$<)

arch/%: $(BASE_NAME)_%.s9pk
	$(call SUMMARY,$<)

x86 x86_64: arch/x86_64
arm arm64 aarch64: arch/aarch64
riscv riscv64: arch/riscv64

$(BASE_NAME).s9pk: $(INGREDIENTS) $(GIT_DEPS) | check-deps
	@$(MAKE) --no-print-directory ingredients
	@echo "   Packing '$@'..."
	start-cli s9pk pack -o $@

$(BASE_NAME)_%.s9pk: $(INGREDIENTS) $(GIT_DEPS) | check-deps
	@$(MAKE) --no-print-directory ingredients
	@echo "   Packing '$@'..."
	start-cli s9pk pack --arch=$* -o $@

ingredients: $(INGREDIENTS)
	@echo "   Re-evaluating ingredients..."

install: | check-deps check-init
	@if [ -z "$$(ls *.s9pk 2>/dev/null)" ]; then \
		echo "Error: No .s9pk file found. Run 'make' first."; \
		exit 1; \
	fi; \
	S9PK=$$(start-cli s9pk select) || exit 1; \
	printf "\n🚀 Installing %s ...\n" "$$S9PK"; \
	start-cli package install -s "$$S9PK"

publish: | all
	@command -v s3cmd >/dev/null || \
		(echo "Error: s3cmd not found. It must be installed to publish using s3." && exit 1); \
	printf "\n🚀 Publishing ...\n"; \
	for s9pk in *.s9pk; do \
		age=$$(( $$(date +%s) - $$(stat -c %Y "$$s9pk") )); \
		if [ "$$age" -gt 3600 ]; then \
			printf "\033[1;33m⚠️  %s is %d minutes old. Publish anyway? [y/N] \033[0m" "$$s9pk" "$$((age / 60))"; \
			read -r ans; \
			case "$$ans" in [yY]*) ;; *) echo "Skipping $$s9pk"; continue ;; esac; \
		fi; \
		start-cli s9pk publish "$$s9pk"; \
	done

check-deps:
	@command -v start-cli >/dev/null || \
		(echo "Error: start-cli not found. See https://docs.start9.com/packaging/environment-setup.html" && exit 1)
	@command -v npm >/dev/null || \
		(echo "Error: npm not found. Install Node.js and npm. See https://docs.start9.com/packaging/environment-setup.html" && exit 1)
	@command -v git >/dev/null || \
		(echo "Error: git not found. Install git. See https://docs.start9.com/packaging/environment-setup.html" && exit 1)
	@command -v jq >/dev/null || \
		(echo "Error: jq not found. Install jq. See https://docs.start9.com/packaging/environment-setup.html" && exit 1)

check-init:
	@start-cli init-key

javascript/index.js: $(shell find startos -type f) tsconfig.json node_modules
	$(TS_CHECK)
	@if [ -f $(SDK_DIR)/lint.mjs ]; then node $(SDK_DIR)/lint.mjs; else echo "   ⚠ SDK lint runner not found; skipping (update @start9labs/start-sdk)"; fi
	$(FORMAT_CHECK)
	$(JS_BUNDLE)

format: | node_modules
	npx prettier --config $(PRETTIER_CONFIG) --write startos

node_modules: package-lock.json package.json
	npm ci

clean:
	@echo "Cleaning up build artifacts..."
	@rm -rf $(PACKAGE_ID).s9pk $(PACKAGE_ID)_x86_64.s9pk $(PACKAGE_ID)_aarch64.s9pk $(PACKAGE_ID)_riscv64.s9pk javascript node_modules
