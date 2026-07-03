TS_FILES := $(shell git ls-files lib)

.PHONY: all dist test check fmt check-fmt clean node_modules

all: dist

lib/exver/exver.ts: node_modules lib/exver/exver.pegjs
	npm run peggy

# Built, self-contained package consumed via file: deps by web, container-runtime,
# and the SDK (which vendors it into its published bundle).
dist: $(TS_FILES) lib/exver/exver.ts package.json node_modules README.md LICENSE
	rm -rf dist
	npm run tsc
	npm run copy
	-@touch dist

test: node_modules
	npm test

check: node_modules
	npm run check

fmt: node_modules
	npx prettier "**/*.ts" --write

check-fmt: node_modules
	npx prettier "**/*.ts" --check

clean:
	rm -rf dist node_modules lib/coverage lib/test/output.ts

package-lock.json: package.json
	npm i

node_modules: package-lock.json
	npm ci
