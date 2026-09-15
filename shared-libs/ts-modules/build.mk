WEB_SHARED_SRC := $(call ls-files, shared-libs/ts-modules/shared) $(call ls-files, shared-libs/ts-modules/marketplace) $(shell ls -p shared-libs/ts-modules/ | grep -v / | sed 's|^|shared-libs/ts-modules/|g') package.json angular.json tsconfig.json tsconfig.lib.json node_modules/.package-lock.json config.json shared-libs/crates/patch-db/client/dist/index.js shared-libs/ts-modules/start-core/dist/package.json projects/start-os/web/patchdb-ui-seed.json
WEB_UI_SRC := $(call ls-files, projects/start-os/web/ui)
WEB_SETUP_WIZARD_SRC := $(call ls-files, projects/start-os/web/setup-wizard)
WEB_START_TUNNEL_SRC := $(call ls-files, projects/start-tunnel/web)
IMMUTABLE_ASSETS_GENERATOR := shared-libs/ts-modules/scripts/generate-immutable-assets.mjs
WEB_UIS := projects/start-os/web/dist/raw/ui/immutable-assets.txt projects/start-os/web/dist/raw/setup-wizard/immutable-assets.txt
COMPRESSED_WEB_UIS := projects/start-os/web/dist/static/ui/immutable-assets.txt projects/start-os/web/dist/static/setup-wizard/immutable-assets.txt

# start-core (the shared TS lib formerly start-sdk/base): web consumes its built
# dist via the root file: dep; the SDK and container-runtime consume it too.
shared-libs/ts-modules/start-core/dist/package.json: $(call ls-files, shared-libs/ts-modules/start-core) shared-libs/ts-modules/start-core/lib/osBindings/index.ts
	$(MAKE) -C shared-libs/ts-modules/start-core dist
	touch shared-libs/ts-modules/start-core/dist/package.json

package-lock.json: package.json shared-libs/ts-modules/start-core/dist/package.json
	npm --prefix . i
	touch package-lock.json

node_modules/.package-lock.json: package-lock.json
	npm --prefix . ci
	touch node_modules/.package-lock.json

.PHONY: immutable-assets-test
immutable-assets-test:
	npm --prefix . run test:immutable-assets

.angular/.updated: shared-libs/crates/patch-db/client/dist/index.js shared-libs/ts-modules/start-core/dist/package.json node_modules/.package-lock.json
	rm -rf .angular
	mkdir -p .angular
	touch .angular/.updated

.i18n-checked: $(WEB_SHARED_SRC) $(WEB_UI_SRC) $(WEB_SETUP_WIZARD_SRC) $(WEB_START_TUNNEL_SRC)
	npm --prefix . run check:i18n
	touch .i18n-checked

projects/start-os/web/dist/raw/ui/immutable-assets.txt: $(WEB_UI_SRC) $(WEB_SHARED_SRC) $(IMMUTABLE_ASSETS_GENERATOR) .angular/.updated .i18n-checked
	npm --prefix . run build:ui
	touch projects/start-os/web/dist/raw/ui/immutable-assets.txt

projects/start-os/web/dist/raw/setup-wizard/immutable-assets.txt: $(WEB_SETUP_WIZARD_SRC) $(WEB_SHARED_SRC) $(IMMUTABLE_ASSETS_GENERATOR) .angular/.updated .i18n-checked
	npm --prefix . run build:setup
	touch projects/start-os/web/dist/raw/setup-wizard/immutable-assets.txt

projects/start-tunnel/web/dist/raw/start-tunnel/immutable-assets.txt: $(WEB_START_TUNNEL_SRC) $(WEB_SHARED_SRC) $(IMMUTABLE_ASSETS_GENERATOR) .angular/.updated .i18n-checked
	npm --prefix . run build:tunnel
	touch projects/start-tunnel/web/dist/raw/start-tunnel/immutable-assets.txt

projects/start-os/web/dist/static/%/immutable-assets.txt: projects/start-os/web/dist/raw/%/immutable-assets.txt
	./shared-libs/ts-modules/compress-uis.sh $* projects/start-os/web

projects/start-tunnel/web/dist/static/%/immutable-assets.txt: projects/start-tunnel/web/dist/raw/%/immutable-assets.txt
	./shared-libs/ts-modules/compress-uis.sh $* projects/start-tunnel/web

config.json: $(GIT_HASH_FILE) $(ENVIRONMENT_FILE) shared-libs/ts-modules/config-sample.json shared-libs/ts-modules/update-config.sh
	./shared-libs/ts-modules/update-config.sh	

# convenience steps to build the StartOS web UIs (OS-product targets; not referenced elsewhere)
start-os-uis: $(WEB_UIS)

start-os-ui: projects/start-os/web/dist/raw/ui/immutable-assets.txt

.PHONY: web-clean
# Owns the Angular workspace, the patch-db TS client it consumes, and brochure (built via this workspace).
web-clean:
	rm -rf node_modules .angular projects/brochure-marketplace/dist
	rm -rf shared-libs/crates/patch-db/client/node_modules shared-libs/crates/patch-db/client/dist
	$(MAKE) -C shared-libs/ts-modules/start-core clean
	rm -f config.json

# Runs prettier over the whole repo (one root config); `web-` name kept for the
# many docs that reference it. Repo-wide, so it also covers the shared TS modules.
.PHONY: web-format web-format-check
web-format:
	npm --prefix . run format

web-format-check:
	npm --prefix . run format:check
