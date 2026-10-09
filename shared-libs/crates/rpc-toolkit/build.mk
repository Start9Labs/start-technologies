define VISIT_RS_TEST_COMMANDS
cargo test -p visit-rs -p visit-rs-derive --locked
cargo test -p visit-rs -p visit-rs-derive --no-default-features --locked
cargo test -p visit-rs -p visit-rs-derive --no-default-features --features visit-rs/meta --locked
cargo test -p visit-rs -p visit-rs-derive --all-features --locked
endef

.PHONY: visit-rs-test rpc-toolkit-test
visit-rs-test:
	$(VISIT_RS_TEST_COMMANDS)

rpc-toolkit-test: node_modules/.package-lock.json
	$(VISIT_RS_TEST_COMMANDS)
	cargo test -p rpc-toolkit --locked
	cargo test -p rpc-toolkit --no-default-features --locked
	cargo test -p rpc-toolkit --features ts --locked
	cargo test -p rpc-toolkit --no-default-features --features ts --locked
	cargo test -p rpc-toolkit --all-features --locked
	cargo test -p rpc-toolkit --no-default-features --features ts,chrono,ipnet,josekit,url,yajrc,exver,patch-db --locked
