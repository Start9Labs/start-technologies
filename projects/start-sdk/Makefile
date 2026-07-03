TS_FILES := $(shell git ls-files lib)
START_CORE := ../../shared-libs/ts-modules/start-core
START_CORE_DIST := $(START_CORE)/dist

.PHONY: all test clean bundle fmt check-fmt check publish link node_modules

all: bundle

# start-core is an external dependency now: the SDK imports it and bundles its
# built lib into the published dist so external authors still install one package.
$(START_CORE_DIST)/package.json:
	$(MAKE) -C $(START_CORE) dist

test: node_modules
	npm test

clean:
	rm -rf dist node_modules lib/test/output.ts

# Build artifacts only — verification (test/check-fmt) is intentionally NOT a
# prerequisite so consumers (the OS build) don't re-run the jest suite every
# build. CI and `publish` run test/check-fmt explicitly.
bundle: dist
	touch dist

dist: $(TS_FILES) package.json .npmignore node_modules README.md LICENSE CHANGELOG.md s9pk.mk tsconfig.base.json eslint.config.base.mjs lint.mjs $(START_CORE_DIST)/package.json
	rm -rf dist
	npm run tsc
	rsync -ac node_modules dist/
	rm -rf dist/node_modules/@start9labs/start-core
	mkdir -p dist/node_modules/@start9labs
	rsync -aL $(START_CORE_DIST)/ dist/node_modules/@start9labs/start-core/
	cp package.json dist/package.json
	cp .npmignore dist/.npmignore
	cp README.md dist/README.md
	cp LICENSE dist/LICENSE
	cp CHANGELOG.md dist/CHANGELOG.md
	cp s9pk.mk dist/s9pk.mk
	cp tsconfig.base.json dist/tsconfig.base.json
	cp eslint.config.base.mjs dist/eslint.config.base.mjs
	cp lint.mjs dist/lint.mjs
	touch dist

full-bundle: bundle

check: node_modules
	npm run check

fmt: node_modules
	npx prettier "**/*.ts" --write

check-fmt: node_modules
	npx prettier "**/*.ts" --check

package-lock.json: package.json $(START_CORE_DIST)/package.json
	npm i

node_modules: package-lock.json $(START_CORE_DIST)/package.json
	npm ci

publish: bundle test check-fmt package.json README.md LICENSE CHANGELOG.md
	cd dist && npm publish --access=public --tag=latest $(if $(OTP),--otp=$(OTP))

link: bundle
	cd dist && npm link
