TS_FILES := $(shell git ls-files lib)
START_CORE := ../../shared-libs/ts-modules/start-core
START_CORE_DIST := $(START_CORE)/dist
START_CORE_SRC := $(shell git ls-files $(START_CORE)/lib) $(START_CORE)/package.json

.DELETE_ON_ERROR:

.PHONY: all test clean bundle fmt check-fmt check publish link sync-template

all: bundle

# Prereqs so a rebuilt start-core restales the SDK dist; touch after because start-core's
# build regenerates a tracked source (exver.ts) without bumping dist (else it rebuilds every time).
$(START_CORE_DIST)/package.json: $(START_CORE_SRC)
	$(MAKE) -C $(START_CORE) dist
	touch $(START_CORE_DIST)/package.json

test: node_modules
	npm test

clean:
	rm -rf dist node_modules lib/test/output.ts

# Build artifacts only — verification (test/check-fmt) is intentionally NOT a
# prerequisite so consumers (the OS build) don't re-run the jest suite every
# build. CI and `publish` run test/check-fmt explicitly.
bundle: dist/package.json

# The sentinel is dist/package.json, not the dist/ dir: make can't see content
# changes under a directory target, so a stale dist/ once published 2.0.5 as 2.0.4.
dist/package.json: $(TS_FILES) package.json .npmignore node_modules README.md LICENSE CHANGELOG.md s9pk.mk tsconfig.base.json eslint.config.base.mjs lint.mjs $(START_CORE_DIST)/package.json
	rm -rf dist
	npm run tsc
	# Don't stage npm's hidden lockfile: it pins start-core to an out-of-tree path.
	rsync -ac --exclude=.package-lock.json node_modules dist/
	rm -rf dist/node_modules/@start9labs/start-core
	mkdir -p dist/node_modules/@start9labs
	rsync -aL $(START_CORE_DIST)/ dist/node_modules/@start9labs/start-core/
	# Point the bundled dep at its in-tarball copy so npm never resolves start-core
	# out of tree (the source spec is a file: link out to shared-libs).
	jq '.dependencies["@start9labs/start-core"]="file:./node_modules/@start9labs/start-core"' package.json > dist/package.json
	cp .npmignore dist/.npmignore
	cp README.md dist/README.md
	cp LICENSE dist/LICENSE
	cp CHANGELOG.md dist/CHANGELOG.md
	cp s9pk.mk dist/s9pk.mk
	cp tsconfig.base.json dist/tsconfig.base.json
	cp eslint.config.base.mjs dist/eslint.config.base.mjs
	cp lint.mjs dist/lint.mjs
	touch dist/package.json

full-bundle: bundle

check: node_modules
	npm run check

fmt: node_modules
	npx prettier "**/*.ts" --write

check-fmt: node_modules
	npx prettier "**/*.ts" --check

# Touch after install: npm may leave these no newer than their prereqs, so without
# it make reruns npm every build (node_modules was also .PHONY, forcing npm ci always).
package-lock.json: package.json $(START_CORE_DIST)/package.json
	npm i
	touch package-lock.json

node_modules: package-lock.json $(START_CORE_DIST)/package.json
	npm ci
	touch node_modules

# Prompt for the npm 2FA OTP here (after the build) so it can't expire; OTP=… skips.
publish: bundle test check-fmt package.json README.md LICENSE CHANGELOG.md
	@otp='$(OTP)'; \
	if [ -z "$$otp" ] && [ -t 0 ]; then \
		printf 'npm one-time password (OTP): ' >&2; \
		read -r otp; \
	fi; \
	cd dist && npm publish --access=public --tag=latest $${otp:+--otp=$$otp}

link: bundle
	cd dist && npm link

# Point the package template's @start9labs/start-sdk pin at this SDK version, so a
# scaffolded package builds against the release being cut. The template ships no
# lockfile (each scaffold's `npm install` generates its own), so the pin is the only
# thing to sync; manage-release.sh pre-check enforces they match.
sync-template:
	npm --prefix docs/package-template pkg set dependencies.@start9labs/start-sdk=$(shell jq -r .version package.json)
