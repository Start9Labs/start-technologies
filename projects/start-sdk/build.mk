test-sdk: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/lib/osBindings/index.ts shared-libs/ts-modules/start-core/dist/package.json
	$(MAKE) -C shared-libs/ts-modules/start-core test
	cd projects/start-sdk && make test

projects/start-sdk/dist/package.json: $(call ls-files, projects/start-sdk) shared-libs/ts-modules/start-core/dist/package.json
	(cd projects/start-sdk && make bundle)
	touch projects/start-sdk/dist/package.json

.PHONY: clean-sdk
clean-sdk:
	cd projects/start-sdk && make clean
	rm -rf projects/start-sdk/docs/book

.PHONY: format-sdk format-check-sdk
format-sdk:
	cd projects/start-sdk && $(MAKE) fmt

format-check-sdk:
	cd projects/start-sdk && $(MAKE) check-fmt
