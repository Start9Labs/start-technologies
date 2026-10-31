COMPLEXITY := ./build/complexity/census.sh
BASE ?= origin/master

.PHONY: complexity complexity-top complexity-diff complexity-record complexity-verify complexity-test

complexity:
	@$(COMPLEXITY) census

complexity-top:
	@$(COMPLEXITY) top

complexity-diff:
	@$(COMPLEXITY) diff $(BASE)

complexity-record:
	@./build/complexity/record.sh

complexity-verify:
	@./build/complexity/verify.sh

complexity-test:
	node --test build/complexity/tests/*.test.mjs
