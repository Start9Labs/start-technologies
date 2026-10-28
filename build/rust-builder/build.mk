RUST_BUILDER_TEST_IMAGE ?= start9-rust-builder:local

.PHONY: rust-builder rust-builder-check rust-builder-selection-test
rust-builder:
	docker buildx build --load --build-arg SOURCE_REVISION=$(shell git rev-parse HEAD) -t $(RUST_BUILDER_TEST_IMAGE) build/rust-builder

rust-builder-check:
	docker run --rm $(RUST_BUILDER_TEST_IMAGE) bash /usr/local/libexec/start9/verify-rust-builder

rust-builder-selection-test: build/tests/rust-builder-selection-test.sh $(RUST_BUILDER_INPUTS)
	bash build/tests/rust-builder-selection-test.sh
