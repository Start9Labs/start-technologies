# --- cognitive-complexity census (no build; a tree-sitter pass over the sources) ---
COMPLEXITY := ./build/complexity/census.sh
BASE ?= origin/master

.PHONY: complexity complexity-top complexity-diff complexity-record

# Totals plus the worst 25 functions in the tree.
complexity:
	@$(COMPLEXITY) census

# The standing pay-down list.
complexity-top:
	@$(COMPLEXITY) top

# What this branch did to the numbers, against its merge-base. Paste into the PR body.
complexity-diff:
	@$(COMPLEXITY) diff $(BASE)

# Appends one row for HEAD to the log. Master CI runs this; a PR never writes it.
complexity-record:
	@./build/complexity/record.sh $(LOG)
