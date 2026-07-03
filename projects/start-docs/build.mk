.PHONY: start-docs
start-docs:
	cd projects/start-docs && ./build.sh

.PHONY: start-docs-clean
start-docs-clean:
	rm -rf projects/start-docs/docs projects/start-docs/node_modules
