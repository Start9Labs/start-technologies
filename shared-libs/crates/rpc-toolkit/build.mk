.PHONY: rpc-toolkit-test
rpc-toolkit-test: node_modules/.package-lock.json
	cargo test -p visit-rs -p visit-rs-derive --features visit-rs/ts --locked
	cargo test -p visit-rs --all-features --locked
	cargo test -p visit-rs --no-default-features --locked
	cargo test -p rpc-toolkit --features ts --locked
	cargo test -p rpc-toolkit --no-default-features --locked
	cargo test -p rpc-toolkit --no-default-features --features ts --locked
