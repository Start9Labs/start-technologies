# --- cognitive-complexity census (no build; a tree-sitter pass over the sources) ---
COMPLEXITY := ./build/complexity/census.sh
BASE ?= origin/master

.PHONY: complexity complexity-top complexity-diff complexity-check

# Totals plus the worst 25 functions in the tree.
complexity:
	@$(COMPLEXITY) census

# The standing pay-down list.
complexity-top:
	@$(COMPLEXITY) top

# What this branch did to the numbers, against its merge-base. Paste into the PR body.
complexity-diff:
	@$(COMPLEXITY) diff $(BASE)

# Fail when a PR body's pasted block is missing, unanswered, or stale.
complexity-check:
	@$(COMPLEXITY) check "$(PR_BODY_FILE)" $(BASE)
