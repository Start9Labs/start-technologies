start-sdk-test: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/lib/osBindings/index.ts shared-libs/ts-modules/start-core/dist/package.json
	$(MAKE) -C shared-libs/ts-modules/start-core test
	cd projects/start-sdk && make test

projects/start-sdk/dist/package.json: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/dist/package.json
	(cd projects/start-sdk && make bundle)
	touch projects/start-sdk/dist/package.json

.PHONY: start-sdk-clean
start-sdk-clean:
	cd projects/start-sdk && make clean
	rm -rf projects/start-sdk/docs/book

.PHONY: start-sdk-format start-sdk-format-check
start-sdk-format:
	cd projects/start-sdk && $(MAKE) fmt

start-sdk-format-check:
	cd projects/start-sdk && $(MAKE) check-fmt
