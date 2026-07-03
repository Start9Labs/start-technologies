test-sdk: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/lib/osBindings/index.ts shared-libs/ts-modules/start-core/dist/package.json
	$(MAKE) -C shared-libs/ts-modules/start-core test
	cd projects/start-sdk && make test

# Co-target the bundled dist/node_modules stamp (see shared-libs/ts-modules/build.mk).
projects/start-sdk/dist/package.json projects/start-sdk/dist/node_modules/.package-lock.json &: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/dist/package.json
	(cd projects/start-sdk && make bundle)
	touch projects/start-sdk/dist/package.json projects/start-sdk/dist/node_modules/.package-lock.json

.PHONY: clean-sdk
clean-sdk:
	cd projects/start-sdk && make clean
	rm -rf projects/start-sdk/docs/book

.PHONY: format-sdk format-check-sdk
format-sdk:
	cd projects/start-sdk && $(MAKE) fmt

format-check-sdk:
	cd projects/start-sdk && $(MAKE) check-fmt
