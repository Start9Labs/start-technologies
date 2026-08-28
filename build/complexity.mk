# --- cognitive-complexity census (no build; a tree-sitter pass over the sources) ---
COMPLEXITY := ./build/complexity/census.sh
BASE ?= origin/master

.PHONY: complexity complexity-top complexity-diff complexity-record complexity-verify

# Totals plus the worst 25 functions in the tree.
complexity:
	@$(COMPLEXITY) census

# The standing pay-down list.
complexity-top:
	@$(COMPLEXITY) top

# What this branch did to the numbers, against its merge-base. Paste into the PR body.
complexity-diff:
	@$(COMPLEXITY) diff $(BASE)

# Prints the delta and appends this tree's totals to the log. Run before opening a PR.
complexity-record:
	@./build/complexity/record.sh

# Fails when no row in the log describes this tree. What CI checks.
complexity-verify:
	@./build/complexity/verify.sh
